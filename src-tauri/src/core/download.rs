//! `download`: yt-dlp writes into a temporary folder inside the output folder, and the
//! finished file is then moved next to its neighbours, never over one of them. With `to_mp4`,
//! ffmpeg turns it into an .mp4 first, in the same temporary folder. Nothing partial, and
//! nothing of another download, is ever in the output folder's way.

use std::ffi::OsStr;
use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Mutex;
use std::thread::{self, JoinHandle};

use serde::{Deserialize, Serialize};

use super::process::{describe_failure, run_captured, run_streaming, CancelHandle};
use super::progress::{FfmpegProgress, Progress, Stage, YtDlpProgress};
use super::{http_url, tool_path, yt_dlp_error, Tool, ToolPaths};

/// The `-preset` values libx264 accepts. The UI's picker (`src/lib/UrlInfo.svelte`) lists the same.
const PRESETS: [&str; 9] = [
    "ultrafast",
    "superfast",
    "veryfast",
    "faster",
    "fast",
    "medium",
    "slow",
    "slower",
    "veryslow",
];

/// Parallel connections for aria2c, or for yt-dlp's own downloader without it.
const CONNECTIONS: u32 = 8;

/// Extensions yt-dlp's output is picked from first, in case other files end up beside it.
const MEDIA_EXTS: [&str; 9] = [
    "mp4", "mkv", "webm", "mov", "m4a", "opus", "ogg", "aac", "mp3",
];

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct DownloadOptions {
    pub url: String,
    /// Folder the finished file is written to.
    pub output_dir: PathBuf,
    /// The quality picker's value: `best`, a maximum height such as `1080`,
    /// `audio` (audio only) or `muxed` (one combined stream: fast, lower resolution).
    pub quality: String,
    /// Name of the output file without extension. Defaults to the video title.
    pub filename: Option<String>,
    /// Convert to .mp4 after downloading. H.264 and AAC streams are copied, others re-encoded.
    /// Ignored for audio only.
    pub to_mp4: bool,
    /// libx264 quality for a re-encode, 0 to 51 (lower is better).
    pub crf: u32,
    /// libx264 speed/size trade-off for a re-encode, `ultrafast` to `veryslow`.
    pub preset: String,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        DownloadOptions {
            url: String::new(),
            output_dir: PathBuf::new(),
            quality: "best".into(),
            filename: None,
            to_mp4: false,
            crf: 18,
            preset: "medium".into(),
        }
    }
}

impl DownloadOptions {
    fn is_audio_only(&self) -> bool {
        self.quality.eq_ignore_ascii_case("audio")
    }

    fn is_muxed(&self) -> bool {
        self.quality.eq_ignore_ascii_case("muxed")
    }
}

/// Why a download failed, as far as the fix differs for the person: another quality, another
/// link, or just another try.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FailureKind {
    FormatUnavailable,
    VideoUnavailable,
    /// The .mp4 conversion failed; the original download was saved instead.
    ConversionFailed,
    Other,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DownloadError {
    Cancelled,
    Failed {
        kind: FailureKind,
        message: String,
        /// The file that was still saved: the original, when only the conversion failed.
        saved: Option<PathBuf>,
    },
}

// Only yt-dlp's own errors have a kind; every other message is `Other`.
impl From<String> for DownloadError {
    fn from(message: String) -> Self {
        DownloadError::Failed {
            kind: FailureKind::Other,
            message,
            saved: None,
        }
    }
}

impl From<&str> for DownloadError {
    fn from(message: &str) -> Self {
        message.to_string().into()
    }
}

impl fmt::Display for DownloadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DownloadError::Cancelled => f.write_str("Download cancelled"),
            DownloadError::Failed { message, .. } => f.write_str(message),
        }
    }
}

impl std::error::Error for DownloadError {}

/// A running download. Wait for its result with [`DownloadTask::wait`], or stop it with
/// [`DownloadTask::cancel`].
pub struct DownloadTask {
    cancel: CancelHandle,
    thread: JoinHandle<Result<PathBuf, DownloadError>>,
}

impl DownloadTask {
    /// Block until the download ends. Returns the path of the finished file.
    pub fn wait(self) -> Result<PathBuf, DownloadError> {
        self.thread
            .join()
            .unwrap_or_else(|_| Err("The download crashed.".into()))
    }

    /// Stop the download: the Tool processes are killed, partial files are removed, and
    /// [`DownloadTask::wait`] returns [`DownloadError::Cancelled`].
    pub fn cancel(&self) {
        self.cancel.cancel();
    }

    /// A handle that cancels this download from elsewhere, for instance after
    /// [`DownloadTask::wait`] has taken the task.
    pub fn canceller(&self) -> CancelHandle {
        self.cancel.clone()
    }
}

/// Start downloading `options.url`. Bad options are rejected here; everything that goes
/// wrong later comes back from [`DownloadTask::wait`].
pub fn download(
    paths: &ToolPaths,
    mut options: DownloadOptions,
    on_progress: impl Fn(Progress) + Send + 'static,
) -> Result<DownloadTask, String> {
    options.url = http_url(&options.url)?.to_string();
    if options.output_dir.as_os_str().is_empty() {
        return Err("Choose a folder to save to.".into());
    }

    // Audio only never converts, so its hidden crf and preset do not matter.
    if options.to_mp4
        && !options.is_audio_only()
        && (options.crf > 51 || !PRESETS.contains(&options.preset.as_str()))
    {
        return Err("Choose a crf from 0 to 51 and a preset from ultrafast to veryslow.".into());
    }

    let paths = paths.clone();
    let cancel = CancelHandle::default();
    let job_cancel = cancel.clone();
    let thread = thread::spawn(move || {
        let job = Job {
            cancel: &job_cancel,
            on_progress: &on_progress,
        };
        match pipeline(&job, &paths, &options) {
            // Whatever went wrong after a cancel is the cancel's doing, unless a file was saved:
            // then it is reported, as a finished download is.
            Err(DownloadError::Failed { saved: None, .. }) if job_cancel.is_cancelled() => {
                Err(DownloadError::Cancelled)
            }
            result => result,
        }
    });
    Ok(DownloadTask { cancel, thread })
}

/// What every stage of a download shares: the cancel flag and where events go. A later
/// stage (transcode) runs its Tools with [`Job::run_tool`] and reports with [`Job::emit`].
struct Job<'a> {
    cancel: &'a CancelHandle,
    on_progress: &'a dyn Fn(Progress),
}

impl Job<'_> {
    /// Events stop as soon as the download is cancelled.
    fn emit(&self, progress: Progress) {
        if !self.cancel.is_cancelled() {
            (self.on_progress)(progress);
        }
    }

    /// Run a Tool to the end. Does not start it if the download is already cancelled.
    fn run_tool(
        &self,
        name: &str,
        cmd: Command,
        on_line: impl FnMut(&str),
    ) -> Result<ExitStatus, String> {
        if self.cancel.is_cancelled() {
            return Err("Download cancelled".into());
        }
        run_streaming(cmd, self.cancel, on_line).map_err(|e| format!("Could not run {name}: {e}"))
    }
}

/// The stages, in order. Each leaves its output in `tmp`; the last one moves the result into
/// the output folder. A cancel, from any stage, ends up in the `Err` and removes `tmp`.
fn pipeline(
    job: &Job,
    paths: &ToolPaths,
    opts: &DownloadOptions,
) -> Result<PathBuf, DownloadError> {
    job.emit(Progress::at(Stage::Info, None));

    fs::create_dir_all(&opts.output_dir)
        .map_err(|e| format!("Could not create the output folder: {e}"))?;
    let tmp = TempDir::new_in(&opts.output_dir)
        .map_err(|e| format!("Could not create a temporary folder: {e}"))?;

    let downloaded = download_stage(job, paths, opts, tmp.path())?;

    let (finished, name) = if opts.to_mp4 && !opts.is_audio_only() {
        // Hidden, so it can never be the downloaded file itself (`find_downloaded` skips those).
        let converted = tmp.path().join(".converted.mp4");
        if let Err(error) = transcode_stage(job, paths, opts, &downloaded, &converted) {
            // The download itself is complete, so a failed conversion keeps it rather than
            // making the person download it all again.
            if job.cancel.is_cancelled() {
                return Err(error.into());
            }
            let kept = place(&downloaded, &opts.output_dir)?;
            return Err(DownloadError::Failed {
                kind: FailureKind::ConversionFailed,
                message: format!(
                    "Converting to .mp4 failed: {error}. The original was saved as {}.",
                    kept.display()
                ),
                saved: Some(kept),
            });
        }
        let mut name = downloaded.file_stem().unwrap_or_default().to_os_string();
        name.push(".mp4");
        (converted, name)
    } else {
        let name = downloaded.file_name().unwrap_or_default().to_os_string();
        (downloaded, name)
    };

    // Even a finished run is dropped if the download was cancelled meanwhile.
    if job.cancel.is_cancelled() {
        return Err("Download cancelled".into());
    }
    let output = place_as(&finished, &opts.output_dir.join(name))?;

    job.emit(Progress::at(Stage::Done, Some(100.0)));
    Ok(output)
}

/// Move `file` into `dir` under its own name, never over another file.
fn place(file: &Path, dir: &Path) -> Result<PathBuf, String> {
    place_as(file, &dir.join(file.file_name().unwrap_or_default()))
}

/// Move `file` to `path`, or `name (1).ext` and so on if that is taken (see [`claim_path`]).
fn place_as(file: &Path, path: &Path) -> Result<PathBuf, String> {
    let output = claim_path(path).map_err(|e| format!("Could not move the download: {e}"))?;
    // A failed move would leave the file in the temporary folder, which is then removed. A copy
    // still works where a move does not, e.g. while antivirus has the new file open on Windows.
    fs::rename(file, &output)
        .or_else(|_| copy_via_part(file, &output))
        .map_err(|e| {
            let _ = fs::remove_file(&output);
            format!("Could not move the download: {e}")
        })?;
    Ok(output)
}

/// Copy `file` to `output` through a hidden `.<name>.part` beside it, so a crash mid-copy leaves
/// a hidden partial file, never a cut-off one under the real name. If renaming the copy fails
/// too (antivirus now has it open), it is copied straight to `output`, as before.
fn copy_via_part(file: &Path, output: &Path) -> io::Result<()> {
    let name = output.file_name().unwrap_or_default().to_string_lossy();
    let part = output.with_file_name(format!(".{name}.part"));
    let result = fs::copy(file, &part)
        .and_then(|_| fs::rename(&part, output).or_else(|_| fs::copy(&part, output).map(drop)));
    let _ = fs::remove_file(&part);
    result
}

/// yt-dlp downloads into `dest`. Returns the file it wrote.
fn download_stage(
    job: &Job,
    paths: &ToolPaths,
    opts: &DownloadOptions,
    dest: &Path,
) -> Result<PathBuf, DownloadError> {
    let yt_dlp =
        tool_path(paths, Tool::YtDlp).ok_or("yt-dlp was not found. Install it and try again.")?;

    // aria2c is used whenever it is present.
    let aria2c = tool_path(paths, Tool::Aria2c);

    let mut cmd = Command::new(yt_dlp);
    // yt-dlp merges streams with ffmpeg. A Managed ffmpeg is on no `PATH`, and neither is a
    // System one when the app was started from the Dock, so yt-dlp is told where it is.
    let ffmpeg = tool_path(paths, Tool::Ffmpeg);
    cmd.args(yt_dlp_args(
        opts,
        aria2c.as_deref(),
        ffmpeg.as_deref(),
        dest,
    ));
    let mut last_error = None;
    let mut progress = YtDlpProgress::default();
    let status = job.run_tool("yt-dlp", cmd, |line| {
        if let Some(progress) = progress.line(line) {
            job.emit(progress);
        } else if line.starts_with("ERROR:") || is_usage_error(line) {
            last_error = Some(line.to_string());
        }
    })?;
    if !status.success() {
        return Err(match last_error.as_deref().and_then(yt_dlp_error) {
            Some((kind, message)) => DownloadError::Failed {
                kind,
                message,
                saved: None,
            },
            None => describe_failure("yt-dlp", status).into(),
        });
    }

    Ok(find_downloaded(dest)?)
}

/// yt-dlp's option parser error, `yt-dlp: error: no such option: --progress-delta`, which an
/// older System yt-dlp prints instead of an `ERROR:` line. The program name has no spaces, so a
/// title such as `Destination: My: error: video` is not one.
fn is_usage_error(line: &str) -> bool {
    line.split_once(": error: ")
        .is_some_and(|(prog, _)| !prog.is_empty() && !prog.contains(char::is_whitespace))
}

/// ffmpeg turns `input` into an .mp4 at `output`. Streams that already are H.264 or AAC are
/// copied. A failed run may leave a partial `output`, which is in the temporary folder.
fn transcode_stage(
    job: &Job,
    paths: &ToolPaths,
    opts: &DownloadOptions,
    input: &Path,
    output: &Path,
) -> Result<(), String> {
    let ffmpeg =
        tool_path(paths, Tool::Ffmpeg).ok_or("ffmpeg was not found. Install it and try again.")?;
    let ffprobe = tool_path(paths, Tool::Ffprobe)
        .ok_or("ffprobe was not found. Install it and try again.")?;

    let probed = probe(job, &ffprobe, input)?;
    let streams: serde_json::Value = serde_json::from_str(&probed).unwrap_or_default();
    // Not knowing the duration only costs the percent.
    let duration = streams["format"]["duration"]
        .as_str()
        .and_then(|d| d.trim().parse::<f64>().ok());
    let codec_of = |kind: &str| {
        streams["streams"]
            .as_array()?
            .iter()
            .find(|s| s["codec_type"] == kind)
            .and_then(|s| s["codec_name"].as_str())
    };
    let (video, audio) = (codec_of("video"), codec_of("audio"));

    // `-v error`: stderr then holds only the reason a run fails, which the error reports.
    let mut args: Vec<String> = ["-v", "error", "-y", "-i"].map(String::from).into();
    args.push(input.to_string_lossy().into_owned());
    args.extend(["-progress", "pipe:1", "-nostats"].map(String::from));
    if video == Some("h264") {
        args.extend(["-c:v", "copy"].map(String::from));
    } else {
        args.extend(["-c:v", "libx264", "-preset", &opts.preset, "-crf"].map(String::from));
        args.extend([opts.crf.to_string(), "-pix_fmt".into(), "yuv420p".into()]);
        // yuv420p needs an even width and height: a 607x1080 clip loses one column.
        args.extend(["-vf", "scale=trunc(iw/2)*2:trunc(ih/2)*2"].map(String::from));
    }
    match audio {
        None => {}
        Some("aac") => args.extend(["-c:a", "copy"].map(String::from)),
        Some(_) => args.extend(["-c:a", "aac", "-b:a", "192k"].map(String::from)),
    }
    args.extend(["-movflags", "+faststart"].map(String::from));
    args.push(output.to_string_lossy().into_owned());

    let mut progress = FfmpegProgress::new(duration);
    let mut reason = None;
    let mut cmd = Command::new(ffmpeg);
    cmd.args(args);
    let status = job.run_tool("ffmpeg", cmd, |line| {
        if let Some(event) = progress.line(line) {
            job.emit(event);
        } else if reason.is_none() && !is_progress_field(line) {
            reason = Some(ffmpeg_reason(line));
        }
    })?;
    match (status.success(), reason) {
        (true, _) => Ok(()),
        (false, Some(reason)) => Err(format!("{}: {reason}", describe_failure("ffmpeg", status))),
        (false, None) => Err(describe_failure("ffmpeg", status)),
    }
}

/// A `key=value` line of `-progress` output, as opposed to a message on stderr.
fn is_progress_field(line: &str) -> bool {
    line.split_once('=').is_some_and(|(key, _)| {
        !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
    })
}

/// ffmpeg's first error line without its `[libx264 @ 0x7f...]` prefix. The first line is the
/// cause; the ones after it only say that the run failed.
fn ffmpeg_reason(line: &str) -> String {
    let line = line.trim();
    match line.strip_prefix('[').and_then(|s| s.split_once("] ")) {
        Some((_, rest)) if !rest.is_empty() => rest.to_string(),
        _ => line.to_string(),
    }
}

/// The codecs of `input`'s streams and its duration, as ffprobe's JSON. Only stdout: a warning
/// on stderr would break the JSON and turn a stream copy into a full re-encode.
fn probe(job: &Job, ffprobe: &Path, input: &Path) -> Result<String, String> {
    if job.cancel.is_cancelled() {
        return Err("Download cancelled".into());
    }
    let mut cmd = Command::new(ffprobe);
    cmd.args([
        "-v",
        "error",
        "-show_entries",
        "stream=codec_type,codec_name:format=duration",
        "-of",
        "json",
    ]);
    cmd.arg(input);
    let out = run_captured(cmd, job.cancel).map_err(|e| format!("Could not run ffprobe: {e}"))?;
    if out.status.success() {
        Ok(out.stdout)
    } else {
        Err(describe_failure("ffprobe", out.status))
    }
}

/// `-f` value for yt-dlp. Anything that is not a height, `audio` or `muxed` means best.
fn format_selector(opts: &DownloadOptions) -> String {
    let digits = opts.quality.as_str();
    let height = (1..=5)
        .contains(&digits.len())
        .then_some(digits)
        .filter(|d| d.bytes().all(|b| b.is_ascii_digit()));

    match (opts.is_audio_only(), opts.is_muxed(), height) {
        (true, _, _) => "bestaudio/best".into(),
        (_, true, _) => "b".into(),
        (_, _, Some(h)) => format!("bv*[height<={h}]+ba/b[height<={h}]"),
        (_, _, None) => "bv*+ba/b".into(),
    }
}

/// Machine-readable progress comes from `--progress-template`; aria2c prints its own readout.
fn yt_dlp_args(
    opts: &DownloadOptions,
    aria2c: Option<&Path>,
    ffmpeg: Option<&Path>,
    dest: &Path,
) -> Vec<String> {
    // The name goes into yt-dlp's output template: `%` is escaped, separators are replaced so
    // the file cannot leave the output folder, and leading dots are dropped because
    // `find_downloaded` skips hidden files. A typed media extension is dropped, since yt-dlp
    // adds the real one: `clip.mp4` would otherwise become `clip.mp4.webm`. Like a title, the
    // name is cut to [`TITLE_BYTES`], or yt-dlp fails with "File name too long". The leading `./`
    // keeps `Chapter: 2` from reading as yt-dlp's `TYPE:` prefix and, on Windows, `C:clip`
    // from reading as a path on drive C.
    let name = opts
        .filename
        .as_deref()
        .map(|name| name.trim_start_matches('.'))
        .map(|name| match name.rsplit_once('.') {
            Some((stem, ext)) if MEDIA_EXTS.contains(&ext.to_ascii_lowercase().as_str()) => stem,
            _ => name,
        })
        .map(|name| &name[..floor_char_boundary(name, TITLE_BYTES)])
        .filter(|name| !name.is_empty());
    let template = match name {
        Some(name) => format!(
            "./{}.%(ext)s",
            name.replace(['/', '\\'], "_").replace('%', "%%")
        ),
        // At most 200 bytes of title: names may have 255, and yt-dlp adds `.f137.mp4.part`. A
        // 100-character CJK title is 300 bytes, and yt-dlp does not shorten it by itself.
        None => format!("%(title).{TITLE_BYTES}B.%(ext)s"),
    };
    let mut args: Vec<String> = [
        // The user's own yt-dlp config (a download archive, `-x`, `--quiet`) would break
        // finding the file and reading progress.
        "--ignore-config",
        "--no-playlist",
        "--newline",
        // Progress at most 4 times a second: every line becomes an event and a re-render.
        "--progress-delta",
        "0.25",
        "--progress-template",
        "download:GHPROGRESS %(progress.downloaded_bytes)s %(progress.total_bytes)s \
         %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
        "-P",
    ]
    .map(String::from)
    .into();
    args.extend([
        dest.to_string_lossy().into_owned(),
        "-o".into(),
        template,
        "-f".into(),
        format_selector(opts),
    ]);

    if let Some(ffmpeg) = ffmpeg {
        args.extend([
            "--ffmpeg-location".into(),
            ffmpeg.to_string_lossy().into_owned(),
        ]);
    }

    if let Some(aria2c) = aria2c {
        let n = CONNECTIONS;
        args.extend([
            "--downloader".into(),
            aria2c.to_string_lossy().into_owned(),
            "--downloader-args".into(),
            format!("aria2c:-x {n} -s {n} -k 1M --summary-interval=1"),
        ]);
    } else {
        args.extend(["-N".into(), CONNECTIONS.to_string()]);
    }

    // `--` ends option parsing, so a URL can never be read as a yt-dlp flag.
    args.extend(["--".into(), opts.url.clone()]);
    args
}

/// Longest file name, in bytes, before yt-dlp's extensions: a typed name is cut like a title.
const TITLE_BYTES: usize = 200;

/// The largest index up to `max` that does not split a character of `s`.
// shortcut: `str::floor_char_boundary` once rust-version reaches 1.91.
fn floor_char_boundary(s: &str, max: usize) -> usize {
    let mut end = max.min(s.len());
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    end
}

/// The media file yt-dlp left in `dir`.
fn find_downloaded(dir: &Path) -> Result<PathBuf, String> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .map_err(|e| format!("Could not read the download folder: {e}"))?
        .filter_map(|entry| entry.ok())
        .filter(|entry| !entry.file_name().to_string_lossy().starts_with('.'))
        .map(|entry| entry.path())
        .collect();
    files.sort();

    let is_media = |p: &PathBuf| {
        p.extension()
            .and_then(OsStr::to_str)
            .is_some_and(|ext| MEDIA_EXTS.contains(&ext.to_ascii_lowercase().as_str()))
    };
    let index = files.iter().position(is_media).unwrap_or(0);
    if files.is_empty() {
        return Err("yt-dlp produced no output file.".into());
    }
    Ok(files.swap_remove(index))
}

/// Claim `path`, or `name (1).ext`, `name (2).ext` and so on, by creating an empty file
/// there. Creating fails if the name is taken, so two downloads never claim the same one;
/// the finished file then replaces the claimed one.
fn claim_path(path: &Path) -> io::Result<PathBuf> {
    let stem = path.file_stem().unwrap_or_default().to_string_lossy();
    let ext = path
        .extension()
        .map(|e| format!(".{}", e.to_string_lossy()))
        .unwrap_or_default();
    let mut candidate = path.to_path_buf();
    let mut n = 0;
    loop {
        match fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(_) => return Ok(candidate),
            Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {
                n += 1;
                candidate = path.with_file_name(format!("{stem} ({n}){ext}"));
            }
            Err(e) => return Err(e),
        }
    }
}

/// Temporary folders are named `.grapplehook-<pid>-<n>`; this file inside one is locked for as
/// long as the download that owns it runs.
const TEMP_PREFIX: &str = ".grapplehook-";
const TEMP_LOCK: &str = ".lock";

/// A folder that is removed, with everything in it, when dropped.
struct TempDir {
    path: PathBuf,
    /// Held locked while the folder is in use; `None` while it is being set up, or on a
    /// filesystem that cannot lock.
    lock: Option<fs::File>,
}

impl TempDir {
    /// Also removes the folders of downloads that never ended in `parent` (see [`find_stale`]).
    fn new_in(parent: &Path) -> io::Result<Self> {
        // A folder is created, then locked: in between, a sweep in this process must not see it.
        static SETUP: Mutex<()> = Mutex::new(());
        let setup = SETUP.lock().unwrap_or_else(|e| e.into_inner());
        let stale = find_stale(parent);
        let dir = Self::create_in(parent);
        drop(setup);
        // Deleted outside the lock: a big partial file on a slow drive would otherwise hold up
        // every other download's start.
        for stale in stale {
            let _ = fs::remove_dir_all(stale);
        }
        dir
    }

    fn create_in(parent: &Path) -> io::Result<Self> {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        loop {
            let n = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!("{TEMP_PREFIX}{}-{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => {
                    // Dropped, so removed again, if the lock file cannot be created.
                    let mut dir = TempDir { path, lock: None };
                    let lock = fs::File::create(dir.path.join(TEMP_LOCK))?;
                    // Some network shares (NFS without a lock service) cannot lock at all. The
                    // download still runs; a sweep there cannot lock the file either, so it
                    // never removes this folder, and only a crash leftover stays behind.
                    match lock.lock() {
                        Ok(()) => dir.lock = Some(lock),
                        Err(e) if cannot_lock(&e) => {}
                        Err(e) => return Err(e),
                    }
                    return Ok(dir);
                }
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => {}
                Err(e) => return Err(e),
            }
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

/// The filesystem has no locks at all, as opposed to a lock that failed this once.
fn cannot_lock(e: &io::Error) -> bool {
    // macOS answers ENOTSUP for NFS mounted `nolocks` and most macFUSE drives; Rust files that
    // under `Uncategorized`, not `Unsupported`. ENOTSUP and EOPNOTSUPP differ on macOS only.
    #[cfg(unix)]
    if matches!(
        e.raw_os_error(),
        Some(libc::ENOLCK | libc::ENOTSUP | libc::EOPNOTSUPP)
    ) {
        return true;
    }
    e.kind() == io::ErrorKind::Unsupported
}

// A unit test: no test filesystem without locks can be made from `tests/`.
#[cfg(all(test, unix))]
mod tests {
    use super::cannot_lock;
    use std::io;

    #[test]
    fn a_filesystem_without_locks_is_recognised_by_every_errno_it_may_answer() {
        for errno in [libc::ENOLCK, libc::ENOTSUP, libc::EOPNOTSUPP] {
            assert!(cannot_lock(&io::Error::from_raw_os_error(errno)), "{errno}");
        }
        // A lock held elsewhere is not "no locks": the download must stop instead.
        assert!(!cannot_lock(&io::Error::from_raw_os_error(
            libc::EWOULDBLOCK
        )));
    }

    // A unit test: a move that fails but a copy that works cannot be set up from `tests/`.
    #[test]
    fn the_copy_fallback_replaces_the_claimed_file_and_leaves_no_part_behind() {
        let dir = tempfile::tempdir().unwrap();
        let (file, output) = (dir.path().join("in.webm"), dir.path().join("clip.webm"));
        std::fs::write(&file, "video").unwrap();
        std::fs::write(&output, "").unwrap(); // claimed, as `claim_path` leaves it

        super::copy_via_part(&file, &output).unwrap();

        assert_eq!(std::fs::read_to_string(&output).unwrap(), "video");
        assert!(!dir.path().join(".clip.webm.part").exists());
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        // Closed first: Windows cannot remove a file that is still open.
        drop(self.lock.take());
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// The temporary folders in `parent` whose download never ended because the app crashed or was
/// force-quit: their lock is free, since the OS drops a lock when its process dies. Their names
/// are never created again, so they can be removed after `SETUP` is released.
/// The app runs one instance at a time, so no other process creates a folder here between
/// `create_dir` and its lock, and the sweep in this process waits on `SETUP`.
/// This process's own folders are never swept: on NFS (and some SMB mounts) Linux emulates these
/// locks with per-process ones, which a process can always take again, so a running download
/// there would look stale.
// shortcut: only the folder a new download goes to is swept, so a crash in a folder never used
// again leaves its partial file there, hidden; remember past output folders if that matters.
// A crash leftover whose pid this process now reuses also stays until a later launch.
fn find_stale(parent: &Path) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(parent) else {
        return Vec::new();
    };
    let own = format!("{TEMP_PREFIX}{}-", std::process::id());
    entries
        .flatten()
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            name.starts_with(TEMP_PREFIX) && !name.starts_with(&own)
        })
        .filter_map(|entry| {
            let dir = entry.path();
            // No lock file: a folder another instance is just setting up.
            let lock = fs::File::open(dir.join(TEMP_LOCK)).ok()?;
            // Closed again at once: Windows cannot remove a file that is still open.
            lock.try_lock().is_ok().then_some(dir)
        })
        .collect()
}
