//! `download` driven through core's public interface, with fake Tools. The fake yt-dlp replays
//! real output recorded once from `yt-dlp` (and `yt-dlp --downloader aria2c`) on a short public
//! video, trimmed (tests/fixtures/yt-dlp-download-*.txt), and records the arguments it was given.
#![cfg(unix)]

use std::fs;
use std::path::PathBuf;
use std::sync::{mpsc, Arc, Mutex};
use std::time::{Duration, Instant};

use grapplehook_lib::core::{
    download, DownloadError, DownloadOptions, FailureKind, Progress, Stage, ToolPaths,
};

mod common;
use common::{close, dir_entries, fake_tool, fixture, is_alive};

const URL: &str = "https://www.youtube.com/watch?v=jNQXAC9IVRw";

/// A scratch folder holding the output folder, the fake Tools and what they recorded.
struct Setup {
    root: tempfile::TempDir,
}

impl Setup {
    fn new() -> Self {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("out")).unwrap();
        fs::create_dir(root.path().join("bin")).unwrap();
        Setup { root }
    }

    fn out(&self) -> PathBuf {
        self.root.path().join("out")
    }

    fn args_file(&self) -> PathBuf {
        self.root.path().join("args.txt")
    }

    /// Tool paths with a fake yt-dlp. The search path is only `bin`, so no real Tool is ever found.
    fn paths(&self, yt_dlp_body: &str) -> ToolPaths {
        // `-P` and `-o` tell the fake where yt-dlp would write; `write_output` does that.
        fake_tool(
            &self.root.path().join("bin"),
            "yt-dlp",
            &format!(
                r#"[ "$1" = "--version" ] && {{ echo 2025.01.01; exit 0; }}
printf '%s\n' "$@" > '{args}'
while [ $# -gt 0 ]; do
  case "$1" in -P) P="$2";; -o) O="$2";; esac
  shift
done
write_output() {{
  name=$(printf '%s' "$O" | sed 's/%(title).200B/Me at the zoo/; s/%(ext)s/webm/; s/%%/%/g')
  printf 'video' > "$P/$name"
}}
{yt_dlp_body}"#,
                args = self.args_file().display()
            ),
        );
        ToolPaths {
            search_path: Some(self.root.path().join("bin").into()),
            ..Default::default()
        }
    }

    /// A fake yt-dlp that replays a recorded run and writes its output file.
    fn replaying(&self, fixture_name: &str) -> ToolPaths {
        self.paths(&format!(
            "cat '{}'\nwrite_output",
            fixture(fixture_name).display()
        ))
    }

    fn aria2c_path(&self) -> PathBuf {
        self.root.path().join("bin/aria2c")
    }

    /// Add a fake aria2c, which only has to answer `--version`, where Tools are searched for.
    fn with_aria2c(&self, mut paths: ToolPaths) -> ToolPaths {
        fake_tool(
            &self.root.path().join("bin"),
            "aria2c",
            "echo 'aria2 version 1.37.0'",
        );
        paths.search_path = Some(self.root.path().join("bin").into());
        paths
    }

    fn options(&self) -> DownloadOptions {
        DownloadOptions {
            url: URL.into(),
            output_dir: self.out(),
            ..Default::default()
        }
    }

    /// The arguments the fake yt-dlp was started with.
    fn yt_dlp_args(&self) -> Vec<String> {
        fs::read_to_string(self.args_file())
            .unwrap()
            .lines()
            .map(String::from)
            .collect()
    }
}

/// Run a download to the end, collecting its progress events.
fn run(
    paths: &ToolPaths,
    options: DownloadOptions,
) -> (Result<PathBuf, DownloadError>, Vec<Progress>) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let sink = events.clone();
    let task = download(paths, options, move |p| sink.lock().unwrap().push(p)).unwrap();
    let result = task.wait();
    let events = events.lock().unwrap().clone();
    (result, events)
}

#[test]
fn a_download_ends_up_as_one_file_in_the_output_folder() {
    let setup = Setup::new();

    let (result, _) = run(
        &setup.replaying("yt-dlp-download-native.txt"),
        setup.options(),
    );

    let path = result.unwrap();
    assert_eq!(path, setup.out().join("Me at the zoo.webm"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "video");
    // The temporary folder yt-dlp worked in is gone.
    assert_eq!(dir_entries(&setup.out()), ["Me at the zoo.webm"]);
}

fn downloading(events: &[Progress]) -> Vec<&Progress> {
    events
        .iter()
        .filter(|e| e.stage == Stage::Download)
        .collect()
}

#[test]
fn yt_dlp_progress_lines_become_download_events() {
    let setup = Setup::new();

    let (result, events) = run(
        &setup.replaying("yt-dlp-download-native.txt"),
        setup.options(),
    );

    result.unwrap();
    // Preparing first, done last, and one event per progress line (9 for the video stream,
    // 9 for the audio stream) and one for the merge in between. Other output is not progress.
    assert_eq!(events.first().unwrap().stage, Stage::Info);
    let done = events.last().unwrap();
    assert_eq!(done.stage, Stage::Done);
    assert_eq!(done.percent, Some(100.0));
    let downloads = downloading(&events);
    assert_eq!(downloads.len(), 19);

    // `GHPROGRESS 1024 223779 NA NA NA` in the first of two streams: half of 0.4576%.
    // Sizes are the stream's, not the download's, so they are left out.
    let first = downloads[0];
    assert!(close(first.percent, 0.2288));
    assert_eq!(first.downloaded_bytes, None);
    assert_eq!(first.total_bytes, None);
    assert_eq!(first.speed, None);
    assert_eq!(first.eta, None);

    // `GHPROGRESS 15360 223779 NA 14351639.438627757 0`: speed is a float.
    let fourth = downloads[3];
    assert!(close(fourth.percent, 3.43215));
    assert_eq!(fourth.speed, Some(14351639.438627757));

    // The video stream ends halfway, and the audio stream carries on from there.
    assert_eq!(downloads[8].percent, Some(50.0));
    assert!(close(downloads[9].percent, 50.203));
    assert_eq!(downloads[17].percent, Some(100.0));
    // `[Merger] Merging formats into ...`: busy, with no percent.
    assert_eq!(downloads[18].percent, None);
    // Never backwards.
    let percents: Vec<f64> = downloads.iter().filter_map(|d| d.percent).collect();
    assert!(percents.windows(2).all(|w| w[0] <= w[1]), "{percents:?}");
}

#[test]
fn a_single_stream_keeps_its_own_percent_and_sizes() {
    let setup = Setup::new();
    let paths = setup.paths(
        "echo '[info] abc: Downloading 1 format(s): 18'\necho '[download] Destination: a.mp4'\necho 'GHPROGRESS 250 1000 NA NA 4'\nwrite_output",
    );

    let (result, events) = run(&paths, setup.options());

    result.unwrap();
    let downloads = downloading(&events);
    assert_eq!(downloads.len(), 1);
    assert_eq!(downloads[0].percent, Some(25.0));
    assert_eq!(downloads[0].total_bytes, Some(1000.0));
    assert_eq!(downloads[0].eta, Some(4.0));
}

#[test]
fn an_unknown_total_falls_back_to_the_estimate_and_no_total_means_no_percent() {
    let setup = Setup::new();
    let paths = setup.paths(
        "echo 'GHPROGRESS 250 NA 1000 NA NA'\necho 'GHPROGRESS NA NA NA NA NA'\nwrite_output",
    );

    let (result, events) = run(&paths, setup.options());

    result.unwrap();
    let downloads = downloading(&events);
    assert_eq!(downloads.len(), 2);
    assert_eq!(downloads[0].percent, Some(25.0));
    assert_eq!(downloads[0].total_bytes, Some(1000.0));
    assert_eq!(downloads[1].percent, None);
    assert_eq!(downloads[1].downloaded_bytes, None);
}

/// The value after `flag` in the arguments yt-dlp was started with.
fn arg_after<'a>(args: &'a [String], flag: &str) -> Option<&'a str> {
    let i = args.iter().position(|a| a == flag)?;
    args.get(i + 1).map(String::as_str)
}

#[test]
fn yt_dlp_gets_the_default_arguments_in_order() {
    let setup = Setup::new();

    let (result, _) = run(
        &setup.replaying("yt-dlp-download-native.txt"),
        setup.options(),
    );

    result.unwrap();
    let mut args = setup.yt_dlp_args();
    // `-P` is the temporary folder next to the finished files; it is checked on its own.
    let temp = PathBuf::from(arg_after(&args, "-P").unwrap());
    assert_eq!(temp.parent().unwrap(), setup.out());
    let p = args.iter().position(|a| a == "-P").unwrap();
    args[p + 1] = "<temp>".into();
    assert_eq!(
        args,
        [
            "--ignore-config",
            "--no-playlist",
            "--newline",
            "--progress-delta",
            "0.25",
            "--progress-template",
            "download:GHPROGRESS %(progress.downloaded_bytes)s %(progress.total_bytes)s \
             %(progress.total_bytes_estimate)s %(progress.speed)s %(progress.eta)s",
            "-P",
            "<temp>",
            "-o",
            "%(title).200B.%(ext)s",
            "-f",
            "bv*+ba/b",
            "-N",
            "8",
            "--",
            URL,
        ]
    );
}

#[test]
fn the_quality_picks_the_format_selector() {
    // (quality, expected `-f`)
    let cases = [
        ("best", "bv*+ba/b"),
        ("1080", "bv*[height<=1080]+ba/b[height<=1080]"),
        ("720", "bv*[height<=720]+ba/b[height<=720]"),
        ("2160", "bv*[height<=2160]+ba/b[height<=2160]"),
        // Small heights are heights too, not "best".
        ("90", "bv*[height<=90]+ba/b[height<=90]"),
        // Something that is neither a height, `audio` nor `muxed` means best.
        ("foo", "bv*+ba/b"),
        // The quality picker's "Audio only" and "Muxed" choices.
        ("audio", "bestaudio/best"),
        ("Muxed", "b"),
    ];

    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");
    for (quality, expected) in cases {
        let options = DownloadOptions {
            quality: quality.into(),
            ..setup.options()
        };

        run(&paths, options).0.unwrap();

        assert_eq!(
            arg_after(&setup.yt_dlp_args(), "-f"),
            Some(expected),
            "{quality}"
        );
    }
}

#[test]
fn a_custom_filename_names_the_file_and_is_kept_out_of_yt_dlps_template_syntax() {
    let setup = Setup::new();
    let options = DownloadOptions {
        filename: Some("100% -/legit".into()),
        ..setup.options()
    };

    let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

    // Separators become `_` so the name cannot leave the output folder; `%` is doubled
    // for yt-dlp, which turns it back into one.
    assert_eq!(
        arg_after(&setup.yt_dlp_args(), "-o"),
        Some("./100%% -_legit.%(ext)s")
    );
    assert_eq!(result.unwrap(), setup.out().join("100% -_legit.webm"));
}

#[test]
fn a_filename_that_looks_like_a_yt_dlp_template_type_still_names_the_video() {
    // yt-dlp reads `chapter:...` (any case) in `-o` as a template for chapter files only.
    let setup = Setup::new();
    let options = DownloadOptions {
        filename: Some("Chapter: 2".into()),
        ..setup.options()
    };

    let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

    assert_eq!(
        arg_after(&setup.yt_dlp_args(), "-o"),
        Some("./Chapter: 2.%(ext)s")
    );
    assert_eq!(result.unwrap(), setup.out().join("Chapter: 2.webm"));
}

#[test]
fn a_media_extension_typed_into_the_filename_is_not_doubled() {
    // Only media extensions: a dot in the name itself stays.
    for (filename, template, file) in [
        ("clip.MP4", "./clip.%(ext)s", "clip.webm"),
        ("v1.2", "./v1.2.%(ext)s", "v1.2.webm"),
    ] {
        let setup = Setup::new();
        let options = DownloadOptions {
            filename: Some(filename.into()),
            ..setup.options()
        };

        let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

        assert_eq!(arg_after(&setup.yt_dlp_args(), "-o"), Some(template));
        assert_eq!(result.unwrap(), setup.out().join(file), "{filename}");
    }
}

#[test]
fn a_long_custom_filename_is_cut_to_200_bytes_like_a_title() {
    // 199 bytes of `a`, then `é` (2 bytes) would cross 200: it is dropped whole, never split.
    let setup = Setup::new();
    let options = DownloadOptions {
        filename: Some(format!("{}é and more", "a".repeat(199))),
        ..setup.options()
    };

    let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

    let name = "a".repeat(199);
    assert_eq!(
        arg_after(&setup.yt_dlp_args(), "-o"),
        Some(format!("./{name}.%(ext)s").as_str())
    );
    assert_eq!(result.unwrap(), setup.out().join(format!("{name}.webm")));
}

#[test]
fn leading_dots_are_dropped_from_a_custom_filename_so_the_file_is_not_hidden() {
    // A name of only dots falls back to the title.
    for (filename, template, file) in [
        (".notes", "./notes.%(ext)s", "notes.webm"),
        ("...", "%(title).200B.%(ext)s", "Me at the zoo.webm"),
    ] {
        let setup = Setup::new();
        let options = DownloadOptions {
            filename: Some(filename.into()),
            ..setup.options()
        };

        let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

        assert_eq!(arg_after(&setup.yt_dlp_args(), "-o"), Some(template));
        assert_eq!(result.unwrap(), setup.out().join(file), "{filename}");
    }
}

#[test]
fn the_url_comes_last_after_a_double_dash_and_is_trimmed() {
    let setup = Setup::new();
    let options = DownloadOptions {
        url: format!("  {URL}\n"),
        ..setup.options()
    };

    let (result, _) = run(&setup.replaying("yt-dlp-download-native.txt"), options);

    result.unwrap();
    let args = setup.yt_dlp_args();
    assert_eq!(args[args.len() - 2..], ["--", URL]);
}

#[test]
fn without_aria2c_yt_dlp_downloads_in_parallel_itself() {
    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");

    run(&paths, setup.options()).0.unwrap();

    let args = setup.yt_dlp_args();
    assert_eq!(arg_after(&args, "-N"), Some("8"));
    assert!(!args.contains(&"--downloader".to_string()));
}

#[test]
fn aria2c_is_used_when_present() {
    let setup = Setup::new();
    let paths = setup.with_aria2c(setup.replaying("yt-dlp-download-aria2c.txt"));

    run(&paths, setup.options()).0.unwrap();

    let args = setup.yt_dlp_args();
    assert_eq!(
        arg_after(&args, "--downloader"),
        Some(setup.aria2c_path().to_str().unwrap())
    );
    assert_eq!(
        arg_after(&args, "--downloader-args"),
        Some("aria2c:-x 8 -s 8 -k 1M --summary-interval=1")
    );
    assert!(!args.contains(&"-N".to_string()));
}

#[test]
fn ffmpeg_is_handed_to_yt_dlp_when_found_so_it_can_merge_streams() {
    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");

    // Not found: yt-dlp is left to look for it itself.
    run(&paths, setup.options()).0.unwrap();
    assert_eq!(arg_after(&setup.yt_dlp_args(), "--ffmpeg-location"), None);

    // Found (a Managed Tool is in no `PATH`, so yt-dlp could not find it alone).
    let ffmpeg = fake_tool(
        &setup.root.path().join("bin"),
        "ffmpeg",
        "echo 'ffmpeg version 7.1.1 Copyright'",
    );
    run(&paths, setup.options()).0.unwrap();
    assert_eq!(
        arg_after(&setup.yt_dlp_args(), "--ffmpeg-location"),
        Some(ffmpeg.to_str().unwrap())
    );
}

fn failure(result: Result<PathBuf, DownloadError>) -> String {
    match result {
        Err(DownloadError::Failed { message, .. }) => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn a_non_zero_exit_is_an_error_carrying_yt_dlps_message_and_leaves_nothing_behind() {
    let setup = Setup::new();
    let paths = setup.paths(
        r#"echo 'GHPROGRESS 100 1000 NA 50 18'
echo "WARNING: something minor" >&2
echo "ERROR: [youtube] abc: Video unavailable" >&2
printf 'partial' > "$P/video.webm.part"
exit 1"#,
    );

    let (result, events) = run(&paths, setup.options());

    assert_eq!(failure(result), "abc: Video unavailable");
    assert!(events.iter().all(|e| e.stage != Stage::Done));
    assert_eq!(dir_entries(&setup.out()), Vec::<String>::new());
}

/// The kind of failure a yt-dlp that prints `stderr_line` and exits with 1 ends in.
fn kind_of(stderr_line: &str) -> FailureKind {
    let setup = Setup::new();
    let paths = setup.paths(&format!("echo '{stderr_line}' >&2\nexit 1"));
    match run(&paths, setup.options()).0 {
        Err(DownloadError::Failed { kind, .. }) => kind,
        other => panic!("expected a failure, got {other:?}"),
    }
}

#[test]
fn a_format_yt_dlp_cannot_get_is_a_failure_of_its_own_kind() {
    let line = "ERROR: [youtube] abc: Requested format is not available. Use --list-formats";
    assert_eq!(kind_of(line), FailureKind::FormatUnavailable);
}

#[test]
fn a_video_that_is_gone_or_private_is_a_failure_of_its_own_kind() {
    for line in [
        "ERROR: [youtube] abc: Video unavailable",
        "ERROR: [youtube] abc: This video is unavailable",
        "ERROR: [youtube] abc: This video is private",
        "ERROR: [youtube] abc: Private video. Sign in to watch",
        "ERROR: [youtube] abc: This video has been removed by the uploader",
    ] {
        assert_eq!(kind_of(line), FailureKind::VideoUnavailable, "{line}");
    }
}

#[test]
fn any_other_failure_is_of_the_other_kind() {
    for line in [
        "ERROR: unable to download video data: HTTP Error 403: Forbidden",
        "no error line at all",
    ] {
        assert_eq!(kind_of(line), FailureKind::Other, "{line}");
    }
}

#[test]
fn an_error_line_that_says_nothing_falls_back_to_how_yt_dlp_ended() {
    let setup = Setup::new();
    let paths = setup.paths("echo 'ERROR: [youtube] ' >&2\nexit 1");

    let (result, _) = run(&paths, setup.options());

    assert_eq!(failure(result), "yt-dlp exited with code 1");
}

#[test]
fn an_option_an_older_yt_dlp_does_not_know_is_named_in_the_error() {
    let setup = Setup::new();
    let paths = setup.paths(
        r#"echo "Usage: yt-dlp [OPTIONS] URL [URL...]" >&2
echo "" >&2
echo "yt-dlp: error: no such option: --progress-delta" >&2
exit 2"#,
    );

    let (result, _) = run(&paths, setup.options());

    assert_eq!(
        failure(result),
        "yt-dlp: error: no such option: --progress-delta"
    );
}

#[test]
fn a_tool_killed_by_a_signal_is_an_error() {
    let setup = Setup::new();
    let paths = setup.paths("kill -9 $$");

    let (result, _) = run(&paths, setup.options());

    assert_eq!(failure(result), "yt-dlp terminated by signal 9");
}

#[test]
fn a_run_that_writes_no_file_is_an_error() {
    let setup = Setup::new();
    let paths = setup.paths("echo nothing to see");

    let (result, _) = run(&paths, setup.options());

    assert_eq!(failure(result), "yt-dlp produced no output file.");
}

#[test]
fn a_missing_yt_dlp_is_an_error() {
    let setup = Setup::new();
    let paths = ToolPaths {
        search_path: Some(setup.root.path().join("bin").into()),
        ..Default::default()
    };

    let (result, _) = run(&paths, setup.options());

    assert!(failure(result).contains("yt-dlp was not found"));
}

#[test]
fn bad_options_are_rejected_before_anything_runs() {
    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");

    for url in ["", "not a url", "ftp://example.com/v", "--exec=touch pwned"] {
        let options = DownloadOptions {
            url: url.into(),
            ..setup.options()
        };
        let err = download(&paths, options, |_| {}).err().unwrap();
        assert_eq!(err, "Enter a valid http(s) URL.", "{url}");
    }

    let no_folder = DownloadOptions {
        output_dir: PathBuf::new(),
        ..setup.options()
    };
    assert!(download(&paths, no_folder, |_| {}).is_err());
    assert!(!setup.args_file().exists());
}

#[test]
fn an_existing_file_is_never_overwritten() {
    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");
    fs::write(setup.out().join("Me at the zoo.webm"), "mine").unwrap();
    fs::write(setup.out().join("Me at the zoo (1).webm"), "mine too").unwrap();

    let path = run(&paths, setup.options()).0.unwrap();

    assert_eq!(path, setup.out().join("Me at the zoo (2).webm"));
    assert_eq!(
        fs::read_to_string(setup.out().join("Me at the zoo.webm")).unwrap(),
        "mine"
    );
    assert_eq!(
        fs::read_to_string(setup.out().join("Me at the zoo (1).webm")).unwrap(),
        "mine too"
    );
}

#[test]
fn downloads_of_the_same_title_at_the_same_time_all_keep_their_own_file() {
    let setup = Setup::new();
    let paths = setup.paths(r#"printf 'video from %s' $$ > "$P/Me at the zoo.webm""#);

    let tasks: Vec<_> = (0..8)
        .map(|_| download(&paths, setup.options(), |_| {}).unwrap())
        .collect();
    let mut contents: Vec<String> = tasks
        .into_iter()
        .map(|task| fs::read_to_string(task.wait().unwrap()).unwrap())
        .collect();

    assert_eq!(dir_entries(&setup.out()).len(), 8);
    contents.sort();
    contents.dedup();
    assert_eq!(contents.len(), 8, "no download overwrote another");
}

#[test]
fn the_media_file_is_picked_when_yt_dlp_leaves_other_files_beside_it() {
    let setup = Setup::new();
    let paths = setup.paths(r#"printf 'x' > "$P/a notes.txt"; printf 'video' > "$P/b clip.mkv""#);

    let path = run(&paths, setup.options()).0.unwrap();

    assert_eq!(path, setup.out().join("b clip.mkv"));
}

#[test]
fn the_output_folder_is_created_when_it_does_not_exist() {
    let setup = Setup::new();
    let paths = setup.replaying("yt-dlp-download-native.txt");
    let options = DownloadOptions {
        output_dir: setup.out().join("a/b"),
        ..setup.options()
    };

    let path = run(&paths, options).0.unwrap();

    assert_eq!(path, setup.out().join("a/b/Me at the zoo.webm"));
}

/// The pids a hanging fake recorded (itself and a child it started), once it has run.
fn recorded_pids(setup: &Setup) -> Vec<String> {
    let text = fs::read_to_string(setup.root.path().join("pids.txt")).unwrap();
    text.split_whitespace().map(String::from).collect()
}

fn assert_all_dead(pids: &[String]) {
    assert_eq!(pids.len(), 2);
    common::assert_all_dead(pids);
}

/// A fake yt-dlp that starts a child, reports some progress and then hangs.
fn hanging(setup: &Setup, prologue: &str) -> ToolPaths {
    setup.paths(&format!(
        r#"{prologue}
sleep 300 &
echo "$!" > '{pids}'
echo "$$" >> '{pids}'
printf 'partial' > "$P/video.webm.part"
echo 'GHPROGRESS 100 1000 NA 50 18'
while :; do wait; done"#,
        pids = setup.root.path().join("pids.txt").display()
    ))
}

/// Start a download and block until its first progress line has been seen, so the
/// fake is known to be running.
fn start_and_wait_for_progress(
    paths: &ToolPaths,
    options: DownloadOptions,
) -> grapplehook_lib::core::DownloadTask {
    let (tx, rx) = mpsc::channel();
    let task = download(paths, options, move |p| {
        let _ = tx.send(p);
    })
    .unwrap();
    loop {
        let event = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("a progress event");
        if event.stage == Stage::Download {
            return task;
        }
    }
}

#[test]
fn cancel_stops_a_tool_that_hangs_and_everything_it_started() {
    let setup = Setup::new();
    let paths = hanging(&setup, "");
    let task = start_and_wait_for_progress(&paths, setup.options());
    let pids = recorded_pids(&setup);
    assert!(pids.iter().all(|pid| is_alive(pid)));

    let started = Instant::now();
    task.cancel();
    let result = task.wait();

    assert_eq!(result, Err(DownloadError::Cancelled));
    // A polite SIGTERM is enough for a Tool that does not ignore it.
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert_all_dead(&pids);
    // The partial download went with the temporary folder.
    assert_eq!(dir_entries(&setup.out()), Vec::<String>::new());
}

#[test]
fn cancelling_a_task_right_after_it_starts_leaves_nothing_running() {
    let setup = Setup::new();
    // This one never finishes, so the only possible outcome is a cancel, whether it lands
    // before the Tool has started or after.
    let paths = hanging(&setup, "");

    let task = download(&paths, setup.options(), |_| {}).unwrap();
    task.cancel();
    let result = task.wait();

    assert_eq!(result, Err(DownloadError::Cancelled));
    if let Ok(text) = fs::read_to_string(setup.root.path().join("pids.txt")) {
        let pids: Vec<String> = text.split_whitespace().map(String::from).collect();
        assert_all_dead(&pids);
    }
    assert_eq!(dir_entries(&setup.out()), Vec::<String>::new());
}

#[test]
fn a_tool_that_ignores_sigterm_is_killed_after_a_grace_period() {
    let setup = Setup::new();
    // The ignore is inherited by the child it starts, so both need the SIGKILL.
    let paths = hanging(&setup, "trap '' TERM");
    let task = start_and_wait_for_progress(&paths, setup.options());
    let pids = recorded_pids(&setup);

    let started = Instant::now();
    task.cancel();
    let result = task.wait();

    assert_eq!(result, Err(DownloadError::Cancelled));
    assert!(
        started.elapsed() > Duration::from_secs(4),
        "{:?}",
        started.elapsed()
    );
    assert_all_dead(&pids);
}

#[test]
fn aria2c_readouts_become_download_events_too() {
    let setup = Setup::new();
    let paths = setup.with_aria2c(setup.replaying("yt-dlp-download-aria2c.txt"));

    let (result, events) = run(&paths, setup.options());

    result.unwrap();
    // 6 readouts for each of the two streams (each printed twice), plus yt-dlp's own
    // line when a stream is finished, plus the merge. The summary banner and `FILE:` lines
    // are not progress.
    let downloads = downloading(&events);
    assert_eq!(downloads.len(), 15);

    // `[#4c809f 16KiB/218KiB(7%) CN:1 DL:16KiB ETA:11s]` in the first of two streams.
    let first = downloads[0];
    assert!(close(first.percent, 3.5));
    assert_eq!(first.speed, Some(16.0 * 1024.0));

    // `[#8cf12d 224KiB/246KiB(90%) CN:1 DL:25KiB]` in the second stream.
    assert!(close(downloads[12].percent, 95.0));

    // `GHPROGRESS 252182 252182 NA 27162.08180508613 NA`: yt-dlp reports the finished stream.
    assert_eq!(downloads[13].percent, Some(100.0));
    assert_eq!(downloads[14].percent, None);
}

#[test]
fn aria2c_sizes_and_times_are_binary_units_and_h_m_s() {
    let setup = Setup::new();
    // aria2c separates its readouts with `\r` when it writes to a terminal.
    let paths = setup.paths(
        r"printf '[#a1b2c3 1GiB/9GiB(11%%) CN:16 DL:1.0MiB ETA:1h2m3s]\r[#a1b2c3 12MiB/345MiB(3%%) CN:8 DL:5.2MiB ETA:1m2s]\r[NOTICE] Download complete\n'
write_output",
    );

    let (result, events) = run(&paths, setup.options());

    result.unwrap();
    let downloads = downloading(&events);
    assert_eq!(downloads.len(), 2);
    assert_eq!(downloads[0].eta, Some(3723.0));
    assert_eq!(
        downloads[0].total_bytes,
        Some(9.0 * 1024.0 * 1024.0 * 1024.0)
    );
    assert_eq!(downloads[0].speed, Some(1024.0 * 1024.0));
    assert_eq!(downloads[1].percent, Some(3.0));
    assert_eq!(
        downloads[1].speed,
        Some((5.2_f64 * 1024.0 * 1024.0).round())
    );
    assert_eq!(downloads[1].eta, Some(62.0));
}

#[test]
fn a_title_with_a_percent_in_brackets_is_not_progress() {
    let setup = Setup::new();
    let paths = setup.paths(
        r#"printf '[download] Destination: Sale (50%%) off.webm\n[Merger] Merging formats into "Sale (50%%) off.webm"\n'
write_output"#,
    );

    let (result, events) = run(&paths, setup.options());

    result.unwrap();
    // Only the merge, which has no percent.
    let downloads = downloading(&events);
    assert!(downloads.iter().all(|d| d.percent.is_none()), "{events:?}");
}

#[test]
fn temporary_folders_of_downloads_that_never_ended_are_removed() {
    let setup = Setup::new();
    let make = |name: &str, lock: bool| {
        let dir = setup.out().join(name);
        fs::create_dir(&dir).unwrap();
        fs::write(dir.join("video.f137.mp4.part"), "partial").unwrap();
        lock.then(|| fs::File::create(dir.join(".lock")).unwrap())
    };
    // Left by a crash: its lock file is there but nobody holds the lock.
    drop(make(".grapplehook-1-0", true));
    // A download still running elsewhere holds its lock.
    let held = make(".grapplehook-2-0", true).unwrap();
    held.lock().unwrap();
    // Another instance is still setting this one up: no lock file yet.
    make(".grapplehook-3-0", false);
    // This process's own, with a free lock: on NFS a process can always take its own lock, so a
    // running download there looks like this one.
    let own = format!(".grapplehook-{}-{}", std::process::id(), u32::MAX);
    drop(make(&own, true));

    run(
        &setup.replaying("yt-dlp-download-native.txt"),
        setup.options(),
    )
    .0
    .unwrap();

    let mut expected = vec![
        ".grapplehook-2-0".to_string(),
        ".grapplehook-3-0".into(),
        own,
        "Me at the zoo.webm".into(),
    ];
    expected.sort();
    assert_eq!(dir_entries(&setup.out()), expected);
    drop(held);
}
