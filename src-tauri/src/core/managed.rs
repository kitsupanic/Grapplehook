//! Tool Changes: installing, deleting and updating Managed Tools. Each one gets the Tools to
//! itself through [`Running::change`], so it is refused while any Tool Run is in flight.

use std::fmt;
use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

const PROGRESS_EVERY: Duration = Duration::from_millis(250);

/// How long `yt-dlp -U` may take. It needs the Tools to itself and has no cancel button, so a
/// host that keeps sending a little would otherwise hold off every download until the app quits.
const UPDATE_WAIT: Duration = Duration::from_secs(5 * 60);

/// The error of an update stopped at [`UPDATE_WAIT`].
const UPDATE_TOO_SLOW: &str = "Updating yt-dlp took too long, so it was stopped. Try again later.";

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::process::{run_captured, CancelHandle, CANCELLED};
use super::running::{Busy, Running};
use super::tools::{check_tools, tool_state, Tool, ToolPaths, ToolState};
use super::update::compare_versions;

/// One download that provides one or more Tools, passed in as data so tests can point
/// it at a local HTTP server. An archive that holds several Tools (BtbN's ffmpeg and
/// ffprobe) is one source with several `ToolFile`s, so it is downloaded once.
#[derive(Debug, Clone)]
pub struct ToolSource {
    pub url: String,
    pub format: Format,
    pub checksum: Checksum,
    /// The Tools this download yields (an archive can hold several).
    pub tools: Vec<ToolFile>,
}

#[derive(Debug, Clone)]
pub struct ToolFile {
    pub tool: Tool,
    /// Path of the Tool inside an archive, matched against the end of each entry's path
    /// (`bin/ffmpeg` finds `ffmpeg-n9.0/bin/ffmpeg`); `None` for a raw download.
    pub inner_path: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    /// The download is the Tool's executable itself.
    Raw,
    Zip,
    TarXz,
}

#[derive(Debug, Clone)]
pub enum Checksum {
    /// A `<sha256>  <asset name>` file published in the same release; the asset name
    /// is the last segment of the source URL.
    SumsFile { url: String },
    /// A SHA-256 written into the source list, for releases that publish none.
    Pinned(String),
    /// A `<sha256>  <name>` file at the URL the source redirected to, plus `.sha256`
    /// (martin-riedl.de has none behind its stable "latest" URL).
    ResolvedSidecar,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallProgress {
    pub tool: Tool,
    pub downloaded: u64,
    pub total: Option<u64>,
}

#[derive(Debug)]
pub enum ToolChangeError {
    /// Refused: Tool Runs are in flight, or another Tool Change is.
    Busy(Busy),
    /// Stopped because the app is quitting.
    Cancelled,
    Download(String),
    ChecksumMissing(String),
    ChecksumMismatch {
        expected: String,
        actual: String,
    },
    Io(String),
    /// Anything else, already worded for the user.
    Failed(String),
}

impl fmt::Display for ToolChangeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Busy(busy) => busy.fmt(f),
            Self::Cancelled => f.write_str(CANCELLED),
            Self::Download(e) => write!(f, "Download failed: {e}"),
            Self::ChecksumMissing(name) => write!(f, "No checksum published for {name}"),
            Self::ChecksumMismatch { expected, actual } => {
                write!(
                    f,
                    "Checksum mismatch: expected {expected}, got {actual}. If the Tool's \
                     makers are publishing a new release right now, try again in a few minutes."
                )
            }
            Self::Io(e) => write!(f, "Could not write the Tool: {e}"),
            Self::Failed(message) => f.write_str(message),
        }
    }
}

impl std::error::Error for ToolChangeError {}

impl From<Busy> for ToolChangeError {
    fn from(busy: Busy) -> Self {
        Self::Busy(busy)
    }
}

impl From<reqwest::Error> for ToolChangeError {
    fn from(e: reqwest::Error) -> Self {
        Self::Download(e.to_string())
    }
}

impl From<std::io::Error> for ToolChangeError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.to_string())
    }
}

impl From<zip::result::ZipError> for ToolChangeError {
    fn from(e: zip::result::ZipError) -> Self {
        Self::Io(e.to_string())
    }
}

/// Where each Tool comes from on this platform (see ADR-0002).
pub fn default_sources() -> Vec<ToolSource> {
    let mut sources = vec![yt_dlp_source()];
    sources.extend(ffmpeg_sources());
    // aria2 publishes official builds for Windows only; elsewhere it is a System Tool.
    if cfg!(windows) {
        sources.push(ToolSource {
            url: "https://github.com/aria2/aria2/releases/download/release-1.37.0/aria2-1.37.0-win-64bit-build1.zip".into(),
            format: Format::Zip,
            // aria2 publishes no checksum, so this is the hash of that zip, computed once.
            checksum: Checksum::Pinned(
                "67d015301eef0b612191212d564c5bb0a14b5b9c4796b76454276a4d28d9b288".into(),
            ),
            tools: vec![in_archive(Tool::Aria2c, "")],
        });
    }
    sources
}

fn yt_dlp_source() -> ToolSource {
    const BASE: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download";
    let asset = if cfg!(windows) {
        "yt-dlp.exe"
    } else if cfg!(target_os = "macos") {
        // A universal build: runs on Apple Silicon and Intel.
        "yt-dlp_macos"
    } else if cfg!(target_arch = "aarch64") {
        "yt-dlp_linux_aarch64"
    } else {
        "yt-dlp_linux"
    };
    ToolSource {
        url: format!("{BASE}/{asset}"),
        format: Format::Raw,
        checksum: Checksum::SumsFile {
            url: format!("{BASE}/SHA2-256SUMS"),
        },
        tools: vec![ToolFile {
            tool: Tool::YtDlp,
            inner_path: None,
        }],
    }
}

/// GPL builds, which have libx264. Windows and Linux: one BtbN archive holds both Tools.
/// macOS: martin-riedl.de ships a zip per Tool, with the Tool at the zip's root. Each build
/// is for one CPU type, so an Intel Mac or an ARM Linux machine gets its own.
fn ffmpeg_sources() -> Vec<ToolSource> {
    let arm = cfg!(target_arch = "aarch64");
    if cfg!(target_os = "macos") {
        let arch = if arm { "arm64" } else { "amd64" };
        return [Tool::Ffmpeg, Tool::Ffprobe]
            .map(|tool| {
                let name = tool.file_name();
                ToolSource {
                    url: format!(
                        "https://ffmpeg.martin-riedl.de/redirect/latest/macos/{arch}/release/{name}.zip"
                    ),
                    format: Format::Zip,
                    checksum: Checksum::ResolvedSidecar,
                    tools: vec![in_archive(tool, "")],
                }
            })
            .to_vec();
    }
    // The BtbN repo moved to the FFmpeg-Builds org; `latest` is a floating release tag.
    const BASE: &str = "https://github.com/FFmpeg-Builds/FFmpeg-Builds/releases/download/latest";
    // shortcut: Windows on ARM gets the x64 build, which it runs under emulation.
    let (asset, format) = if cfg!(windows) {
        ("ffmpeg-n9.0-latest-win64-gpl-9.0.zip", Format::Zip)
    } else if arm {
        (
            "ffmpeg-n9.0-latest-linuxarm64-gpl-9.0.tar.xz",
            Format::TarXz,
        )
    } else {
        ("ffmpeg-n9.0-latest-linux64-gpl-9.0.tar.xz", Format::TarXz)
    };
    vec![ToolSource {
        url: format!("{BASE}/{asset}"),
        format,
        checksum: Checksum::SumsFile {
            url: format!("{BASE}/checksums.sha256"),
        },
        tools: vec![
            in_archive(Tool::Ffmpeg, "bin/"),
            in_archive(Tool::Ffprobe, "bin/"),
        ],
    }]
}

fn in_archive(tool: Tool, dir: &str) -> ToolFile {
    ToolFile {
        tool,
        inner_path: Some(format!("{dir}{}", tool.file_name())),
    }
}

/// Total download size in bytes of what `install_tools` would fetch for the `missing` Tools
/// (as `check_tools` reported them), from each response's Content-Length, so the UI can show
/// it before anything is downloaded. `None` if any size is unknown. Runs no Tool, so it is no
/// Tool Run and never holds up a Tool Change.
pub fn install_size(missing: &[Tool], sources: &[ToolSource]) -> Option<u64> {
    let needed = sources_for(missing, sources);
    if needed.is_empty() {
        return Some(0);
    }
    let client = client().ok()?;
    needed
        .into_iter()
        .map(|source| content_length(&client, &source.url))
        .sum()
}

/// The sources worth downloading: a Tool that is already there and runs, Managed or System,
/// is never downloaded again; one `check_tools` reports Missing is. A source that yields several
/// Tools (ffmpeg and ffprobe) is needed if any one of them is Missing.
fn needed<'a>(
    paths: &ToolPaths,
    cancel: &CancelHandle,
    sources: &'a [ToolSource],
) -> Vec<&'a ToolSource> {
    // `check_tools` runs the version commands in parallel; one after another, a slow or hung
    // Tool would hold the install at 0% for its whole deadline, once per Tool.
    let missing: Vec<Tool> = check_tools(paths, cancel)
        .tools
        .into_iter()
        .filter(|t| t.state == ToolState::Missing)
        .map(|t| t.tool)
        .collect();
    sources_for(&missing, sources)
}

/// The sources that yield at least one of the `missing` Tools.
fn sources_for<'a>(missing: &[Tool], sources: &'a [ToolSource]) -> Vec<&'a ToolSource> {
    sources
        .iter()
        .filter(|s| s.tools.iter().any(|f| missing.contains(&f.tool)))
        .collect()
}

fn content_length(client: &reqwest::blocking::Client, url: &str) -> Option<u64> {
    let length = |response: reqwest::blocking::Response| {
        response
            .error_for_status()
            .ok()?
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)?
            .to_str()
            .ok()?
            .parse()
            .ok()
    };
    // Some hosts (martin-riedl.de) answer HEAD badly; a GET whose body is never read is
    // just as cheap.
    // A size is only a label, so a host that stalls gets no more than this.
    const WAIT: Duration = Duration::from_secs(10);
    client
        .head(url)
        .timeout(WAIT)
        .send()
        .ok()
        .and_then(length)
        .or_else(|| client.get(url).timeout(WAIT).send().ok().and_then(length))
}

/// Download the sources that are needed (see [`needed`]), verify their checksums and
/// move the Tools into the Managed Tools folder (`paths.tools_dir`). All or nothing:
/// everything is staged in a scratch folder first and only moved into place once every source
/// has downloaded, verified and unpacked. A failure leaves the folder as it was.
///
/// A Tool Change: refused with [`Busy`] while a Tool Run or another Tool Change is in flight.
/// Quitting stops it between two sources, never while the Tools are moved into place.
pub fn install_tools(
    paths: &ToolPaths,
    running: &Running,
    sources: &[ToolSource],
    on_progress: impl FnMut(InstallProgress),
) -> Result<(), ToolChangeError> {
    running.change(|cancel| install(paths, cancel, sources, on_progress))?
}

fn install(
    paths: &ToolPaths,
    cancel: &CancelHandle,
    sources: &[ToolSource],
    mut on_progress: impl FnMut(InstallProgress),
) -> Result<(), ToolChangeError> {
    let tools_dir = paths
        .tools_dir
        .as_deref()
        .ok_or_else(|| ToolChangeError::Failed("There is no folder for Managed Tools.".into()))?;
    let sources = needed(paths, cancel, sources);
    if sources.is_empty() {
        return Ok(());
    }
    fs::create_dir_all(tools_dir)?;
    let staging = tools_dir.join(".install");
    let _ = fs::remove_dir_all(&staging);
    fs::create_dir(&staging)?;

    let result = sources
        .iter()
        .enumerate()
        .try_for_each(|(i, source)| {
            // shortcut: cancel is only seen between sources, so quitting can wait out the
            // download in progress; check it in the download loop if that proves too slow.
            if cancel.is_cancelled() {
                return Err(ToolChangeError::Cancelled);
            }
            stage_source(i, source, &staging, &mut on_progress)
        })
        .and_then(|()| {
            let tools: Vec<Tool> = sources
                .iter()
                .flat_map(|s| &s.tools)
                .map(|f| f.tool)
                .collect();
            commit(&staging, tools_dir, &tools)
        });
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Delete every Managed Tool by removing the Managed Tools folder. System Tools are never
/// touched. Nothing to delete is not an error. A Tool Change, like [`install_tools`]; once it
/// has started it always runs to the end.
pub fn delete_tools(paths: &ToolPaths, running: &Running) -> Result<(), ToolChangeError> {
    running.change(|_| {
        let Some(tools_dir) = paths.tools_dir.as_deref() else {
            return Ok(());
        };
        match fs::remove_dir_all(tools_dir) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        }
    })?
}

/// A Tool file as it was before a Tool Change: kept aside as a hard link, or not there at all.
struct Previous {
    dest: PathBuf,
    kept: Option<PathBuf>,
}

impl Previous {
    /// Keep the file at `dest` aside, at `kept`, which must be on the same filesystem.
    fn keep(dest: PathBuf, kept: PathBuf) -> io::Result<Self> {
        // shortcut: a hard link needs a filesystem that has them, and the app-data
        // folder always does; copy the file instead if that ever stops being true.
        match fs::hard_link(&dest, &kept) {
            Ok(()) => Ok(Self {
                dest,
                kept: Some(kept),
            }),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self { dest, kept: None }),
            Err(e) => Err(e),
        }
    }

    /// Put the file back as it was: the kept one, or nothing.
    fn restore(self) -> io::Result<()> {
        match self.kept {
            Some(kept) => fs::rename(kept, self.dest),
            None => fs::remove_file(self.dest),
        }
    }
}

/// Move the staged `tools` into `tools_dir`, all or nothing: whatever was there is kept in
/// `staging` until every Tool is in place, and put back if one cannot be.
fn commit(staging: &Path, tools_dir: &Path, tools: &[Tool]) -> Result<(), ToolChangeError> {
    let previous = staging.join("previous");
    fs::create_dir(&previous)?;
    // Each Tool put in place so far.
    let mut placed: Vec<Previous> = Vec::new();
    for tool in tools {
        let name = tool.file_name();
        let put = || -> io::Result<Previous> {
            let kept = Previous::keep(tools_dir.join(&name), previous.join(&name))?;
            fs::rename(staging.join(&name), &kept.dest)?;
            Ok(kept)
        };
        match put() {
            Ok(kept) => placed.push(kept),
            Err(e) => {
                placed.into_iter().rev().for_each(|kept| {
                    let _ = kept.restore();
                });
                return Err(e.into());
            }
        }
    }
    Ok(())
}

/// Download one source into `staging` and leave its Tools there under their final names.
fn stage_source(
    index: usize,
    source: &ToolSource,
    staging: &Path,
    on_progress: &mut impl FnMut(InstallProgress),
) -> Result<(), ToolChangeError> {
    let download = staging.join(format!("download-{index}"));
    // The file and its checksum are two requests to a "latest" URL, so a release published in
    // between pairs the wrong two. One more try gets a matching pair; tampering fails both.
    // A pinned checksum never changes, so trying again would only download the file twice.
    match download_verified(source, &download, on_progress) {
        Err(ToolChangeError::ChecksumMismatch { .. })
            if !matches!(source.checksum, Checksum::Pinned(_)) =>
        {
            download_verified(source, &download, on_progress)?
        }
        other => other?,
    }
    match source.format {
        Format::Raw => {
            let [file] = source.tools.as_slice() else {
                return Err(ToolChangeError::Failed(
                    "A raw download yields exactly one Tool.".into(),
                ));
            };
            fs::rename(&download, staging.join(file.tool.file_name()))?;
        }
        Format::Zip => extract_zip(&download, staging, &source.tools)?,
        Format::TarXz => extract_tar_xz(&download, staging, &source.tools)?,
    }
    let _ = fs::remove_file(&download); // free the space before the next download
    #[cfg(unix)]
    for file in &source.tools {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(
            staging.join(file.tool.file_name()),
            fs::Permissions::from_mode(0o755),
        )?;
    }
    Ok(())
}

/// Does the archive entry `name` end with `inner` at a path boundary?
fn path_matches(name: &str, inner: &str) -> bool {
    name == inner || name.ends_with(&format!("/{inner}"))
}

fn inner_path(file: &ToolFile) -> Result<&str, ToolChangeError> {
    file.inner_path.as_deref().ok_or_else(|| {
        ToolChangeError::Failed("An archive needs the path of the Tool inside it.".into())
    })
}

fn not_in_archive(inner: &str) -> ToolChangeError {
    ToolChangeError::Failed(format!("{inner} is not in the downloaded archive."))
}

fn extract_zip(archive: &Path, staging: &Path, tools: &[ToolFile]) -> Result<(), ToolChangeError> {
    let mut zip = zip::ZipArchive::new(fs::File::open(archive)?)?;
    for file in tools {
        let inner = inner_path(file)?;
        let index = (0..zip.len())
            .find(|&i| {
                zip.name_for_index(i)
                    .and_then(Result::ok)
                    .is_some_and(|n| path_matches(&n, inner))
            })
            .ok_or_else(|| not_in_archive(inner))?;
        let mut out = fs::File::create(staging.join(file.tool.file_name()))?;
        std::io::copy(&mut zip.by_index(index)?, &mut out)?;
    }
    Ok(())
}

fn extract_tar_xz(
    archive: &Path,
    staging: &Path,
    tools: &[ToolFile],
) -> Result<(), ToolChangeError> {
    let mut pending: Vec<&ToolFile> = tools.iter().collect();
    // One pass: a tar can't be searched, and the xz stream is decoded as it is read.
    let mut tar = tar::Archive::new(liblzma::read::XzDecoder::new(fs::File::open(archive)?));
    for entry in tar.entries()? {
        let mut entry = entry?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry.path()?.to_string_lossy().into_owned();
        let found = pending
            .iter()
            .position(|f| inner_path(f).is_ok_and(|inner| path_matches(&path, inner)));
        if let Some(found) = found {
            let file = pending.remove(found);
            entry.unpack(staging.join(file.tool.file_name()))?;
        }
    }
    match pending.first() {
        Some(file) => Err(not_in_archive(inner_path(file)?)),
        None => Ok(()),
    }
}

fn client() -> Result<reqwest::blocking::Client, reqwest::Error> {
    // The blocking client has no per-read timeout, so without a limit a host that stops sending
    // holds the Tool Change, and with it every download, until the app quits.
    // shortcut: one hour for a whole Tool download, which a link slower than ~40 KB/s misses on
    // ffmpeg; read in chunks with a stall deadline instead if that ever matters.
    reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(60 * 60))
        .connect_timeout(Duration::from_secs(30))
        .user_agent(concat!("grapplehook/", env!("CARGO_PKG_VERSION")))
        // Never from https to http: the Tool and its checksum would both be open to tampering.
        .redirect(reqwest::redirect::Policy::custom(|attempt| {
            if leaves_https(attempt.previous(), attempt.url()) {
                attempt.error("a redirect left https")
            } else if attempt.previous().len() > 10 {
                attempt.error("too many redirects")
            } else {
                attempt.follow()
            }
        }))
        .build()
}

/// Would following a redirect to `next` drop from https, anywhere along the chain, to http?
fn leaves_https(previous: &[reqwest::Url], next: &reqwest::Url) -> bool {
    previous.iter().any(|u| u.scheme() == "https") && next.scheme() != "https"
}

/// GET that follows redirects and turns HTTP errors (404, ...) into `Err`.
fn get(url: &str) -> Result<reqwest::blocking::Response, reqwest::Error> {
    client()?.get(url).send()?.error_for_status()
}

/// `https://host/dir/name.zip?x=1` -> `name.zip`
fn asset_name(url: &str) -> &str {
    let path = url.split(['?', '#']).next().unwrap_or(url);
    path.rsplit('/').next().unwrap_or_default()
}

/// The hash listed for `name` in a `<sha256>  <name>` file.
fn find_hash(sums: &str, name: &str) -> Result<String, ToolChangeError> {
    sums.lines()
        .filter_map(|line| line.split_once(char::is_whitespace))
        .find(|(_, listed)| listed.trim().trim_start_matches('*') == name)
        .map(|(hash, _)| hash.to_ascii_lowercase())
        .ok_or_else(|| ToolChangeError::ChecksumMissing(name.to_string()))
}

/// Stream the source into `dest`, hashing as it goes, and check the hash.
fn download_verified(
    source: &ToolSource,
    dest: &Path,
    on_progress: &mut impl FnMut(InstallProgress),
) -> Result<(), ToolChangeError> {
    let mut response = get(&source.url)?;
    // After redirects: the stable "latest" URL has no checksum of its own for some hosts.
    let resolved = response.url().to_string();
    let expected = match &source.checksum {
        Checksum::Pinned(hash) => hash.to_ascii_lowercase(),
        Checksum::SumsFile { url } => find_hash(&get(url)?.text()?, asset_name(&source.url))?,
        Checksum::ResolvedSidecar => find_hash(
            &get(&format!("{resolved}.sha256"))?.text()?,
            asset_name(&resolved),
        )?,
    };
    let tool = source.tools.first().map(|f| f.tool).unwrap_or(Tool::YtDlp);
    let total = response.content_length();

    let mut out = fs::File::create(dest)?;
    let mut hasher = Sha256::new();
    let mut downloaded = 0;
    let mut buf = vec![0u8; 64 * 1024];
    // At most 4 events a second, plus the last one, as for downloads: each is a re-render.
    let mut last_event: Option<Instant> = None;
    loop {
        let n = response
            .read(&mut buf)
            .map_err(|e| ToolChangeError::Download(e.to_string()))?;
        hasher.update(&buf[..n]);
        out.write_all(&buf[..n])?;
        downloaded += n as u64;
        if n == 0 || last_event.is_none_or(|at| at.elapsed() >= PROGRESS_EVERY) {
            last_event = Some(Instant::now());
            on_progress(InstallProgress {
                tool,
                downloaded,
                total,
            });
        }
        if n == 0 {
            break;
        }
    }
    out.flush()?;

    let actual: String = hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect();
    if actual != expected {
        return Err(ToolChangeError::ChecksumMismatch { expected, actual });
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct YtDlpUpdate {
    pub before: String,
    pub after: String,
}

/// Its own version deadline, not the update's: a check cut short would read as a broken yt-dlp.
fn managed_yt_dlp(paths: &ToolPaths) -> Option<(PathBuf, String)> {
    match tool_state(Tool::YtDlp, paths, &CancelHandle::default()) {
        ToolState::Managed { path, version } => Some((path, version)),
        _ => None,
    }
}

/// Runs `yt-dlp -U` on the Managed yt-dlp. System Tools are never touched. A Tool Change, like
/// [`install_tools`]. Quitting stops `yt-dlp -U` and the result is [`ToolChangeError::Cancelled`].
///
/// yt-dlp's exit code is not reliable here (it exits 100 even when it only printed an
/// error), so the outcome is judged by the version afterwards plus any `ERROR:` line.
/// yt-dlp is kept aside first and put back if the update leaves it unable to run. The hard-link
/// backup is safe because the standalone yt-dlp builds update by renaming a `.new` file into
/// place; only its zip variant, which Grapplehook never installs, rewrites itself in place.
pub fn update_yt_dlp(paths: &ToolPaths, running: &Running) -> Result<YtDlpUpdate, ToolChangeError> {
    running.change(|cancel| update(paths, cancel))?
}

fn update(paths: &ToolPaths, cancel: &CancelHandle) -> Result<YtDlpUpdate, ToolChangeError> {
    let Some((path, before)) = managed_yt_dlp(paths) else {
        return Err(ToolChangeError::Failed(
            "Only a Managed yt-dlp can be updated. The app never changes Tools you installed yourself."
                .into(),
        ));
    };
    if cancel.is_cancelled() {
        return Err(ToolChangeError::Cancelled);
    }
    // yt-dlp -U puts a new file at its path, so a hard link keeps the old one intact.
    let scratch = path.with_file_name(".update");
    let _ = fs::remove_dir_all(&scratch);
    let kept = fs::create_dir(&scratch)
        .and_then(|()| Previous::keep(path.clone(), scratch.join(Tool::YtDlp.file_name())))
        .map_err(|e| {
            let _ = fs::remove_dir_all(&scratch);
            ToolChangeError::Failed(format!("Could not back up yt-dlp: {e}"))
        })?;

    // --ignore-config, as on every other yt-dlp run: a yt-dlp.conf (for one, in the working
    // directory) could otherwise point -U at another repository or an older release.
    let mut cmd = Command::new(&path);
    cmd.args(["--ignore-config", "-U"]);
    let deadline = cancel.with_deadline(UPDATE_WAIT);
    let output = run_captured(cmd, &deadline);
    let after = managed_yt_dlp(paths).map(|(_, version)| version);
    let downgraded = after
        .as_deref()
        .is_some_and(|a| compare_versions(a, &before).is_lt());

    // Put the old copy back if the update broke yt-dlp or went to an older version. If that
    // fails, the scratch folder holds the only good copy, so it stays.
    if after.is_none() || downgraded {
        if let Err(e) = kept.restore() {
            return Err(ToolChangeError::Failed(format!(
                "The update left yt-dlp unusable and the old copy could not be put back \
                 ({e}). It is kept at {}.",
                scratch.join(Tool::YtDlp.file_name()).display()
            )));
        }
    }
    let _ = fs::remove_dir_all(&scratch);

    let output =
        output.map_err(|e| ToolChangeError::Failed(format!("Could not run yt-dlp -U: {e}")))?;
    if deadline.timed_out() {
        return Err(ToolChangeError::Failed(UPDATE_TOO_SLOW.into()));
    }
    if cancel.is_cancelled() {
        return Err(ToolChangeError::Cancelled);
    }
    if downgraded {
        return Err(ToolChangeError::Failed(format!(
            "yt-dlp -U installed an older version ({}), so {before} was put back.",
            after.unwrap_or_default()
        )));
    }
    let Some(after) = after else {
        return Err(ToolChangeError::Failed(
            "yt-dlp stopped working after the update, so the old copy was restored.".into(),
        ));
    };
    if after == before {
        let text = format!("{}\n{}", output.stderr, output.stdout);
        if let Some(line) = text.lines().find(|l| l.trim_start().starts_with("ERROR")) {
            return Err(ToolChangeError::Failed(line.trim().to_string()));
        }
    }
    Ok(YtDlpUpdate { before, after })
}

// Unit tests, not in `tests/`: the local test servers speak plain http, so the
// public interface can never reach an https-to-http hop, and it cannot shorten the update's
// five-minute limit.
#[cfg(test)]
mod tests {
    use super::{leaves_https, update, CancelHandle, ToolPaths, UPDATE_TOO_SLOW};
    use reqwest::Url;
    use std::time::Duration;

    fn url(s: &str) -> Url {
        Url::parse(s).unwrap()
    }

    #[test]
    fn a_redirect_may_never_drop_from_https_to_http() {
        let https = url("https://github.com/a");
        let http = url("http://example.com/b");
        let cdn = url("https://cdn.example.com/c");
        assert!(leaves_https(std::slice::from_ref(&https), &http));
        // Once https, always https, even after a hop in between.
        assert!(leaves_https(&[https.clone(), cdn.clone()], &http));
        assert!(!leaves_https(&[https], &cdn));
        // A chain that never was https may stay on http (the local test servers do).
        assert!(!leaves_https(&[url("http://127.0.0.1/a")], &http));
    }

    #[cfg(unix)]
    #[test]
    fn an_update_past_its_deadline_is_stopped_and_yt_dlp_still_runs() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let yt_dlp = dir.path().join("yt-dlp");
        std::fs::write(
            &yt_dlp,
            "#!/bin/sh\ncase \" $* \" in\n*' --version '*) echo 2025.01.01;;\n*' -U '*) exec sleep 60;;\nesac\n",
        )
        .unwrap();
        std::fs::set_permissions(&yt_dlp, std::fs::Permissions::from_mode(0o755)).unwrap();
        let paths = ToolPaths {
            tools_dir: Some(dir.path().into()),
            search_path: Some("".into()),
            ..Default::default()
        };
        let cancel = CancelHandle::default().with_deadline(Duration::from_secs(1));

        let error = update(&paths, &cancel).unwrap_err();

        assert_eq!(error.to_string(), UPDATE_TOO_SLOW);
        assert!(yt_dlp.is_file(), "the old yt-dlp is kept");
    }
}
