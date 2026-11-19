//! The transcode stage of `download`, driven through core's public interface with fake Tools.
//! The fake ffprobe answers with real output recorded once from ffprobe 9.0 on a VP9/Opus and
//! an H.264/AAC file (tests/fixtures/ffprobe-codecs-*.json); the fake ffmpeg replays the real
//! `-progress pipe:1 -nostats` output of a 20 s VP9/Opus to H.264/AAC run
//! (tests/fixtures/ffmpeg-progress.txt, including the `N/A` block ffmpeg prints mid-run).
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, Arc, Mutex};
use std::time::Duration;

use grapplehook_lib::core::{
    download, DownloadError, DownloadOptions, FailureKind, Progress, Stage, ToolPaths,
};

mod common;
use common::{close, dir_entries, fake_tool, fixture};

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
        let setup = Setup { root };
        // yt-dlp leaves `Me at the zoo.webm` wherever `-P` points.
        fake_tool(
            &setup.bin_dir(),
            "yt-dlp",
            r#"[ "$1" = "--version" ] && { echo 2025.01.01; exit 0; }
while [ $# -gt 0 ]; do
  case "$1" in -P) P="$2";; esac
  shift
done
printf 'video' > "$P/Me at the zoo.webm""#,
        );
        setup
    }

    fn bin_dir(&self) -> PathBuf {
        self.root.path().join("bin")
    }

    fn bin(&self, name: &str) -> PathBuf {
        self.bin_dir().join(name)
    }

    fn out(&self) -> PathBuf {
        self.root.path().join("out")
    }

    fn recorded(&self, name: &str) -> PathBuf {
        self.root.path().join(name)
    }

    /// Fake ffprobe: prints the codec answer in the file `codecs`, with `duration` added as
    /// ffprobe's `format.duration` (`None` leaves it out, as for a file of unknown length).
    fn ffprobe(&self, codecs: &str, duration: Option<&str>) {
        let mut answer = fs::read_to_string(codecs).unwrap();
        if let (Ok(mut json), Some(d)) =
            (serde_json::from_str::<serde_json::Value>(&answer), duration)
        {
            json["format"] = serde_json::json!({ "duration": d });
            answer = json.to_string();
        }
        let answer_file = self.recorded("ffprobe-answer.json");
        fs::write(&answer_file, answer).unwrap();
        fake_tool(
            &self.bin_dir(),
            "ffprobe",
            &format!(
                r#"[ "$1" = "-version" ] && {{ echo 'ffprobe version 9.0.1'; exit 0; }}
printf '%s\n' "$@" >> '{args}'
cat '{answer}'"#,
                args = self.recorded("ffprobe-args.txt").display(),
                answer = answer_file.display(),
            ),
        );
    }

    /// Fake ffmpeg: records its arguments, writes its output (the last argument), then runs `then`.
    fn ffmpeg(&self, then: &str) {
        fake_tool(
            &self.bin_dir(),
            "ffmpeg",
            &format!(
                r#"[ "$1" = "-version" ] && {{ echo 'ffmpeg version 9.0.1'; exit 0; }}
printf '%s\n' "$@" > '{args}'
for last; do :; done
printf 'mp4' > "$last"
{then}"#,
                args = self.recorded("ffmpeg-args.txt").display(),
            ),
        );
    }

    fn replay_ffmpeg(&self) {
        self.ffmpeg(&format!(
            "cat '{}'",
            fixture("ffmpeg-progress.txt").display()
        ));
    }

    fn paths(&self) -> ToolPaths {
        ToolPaths {
            search_path: Some(self.root.path().join("bin").into()),
            ..Default::default()
        }
    }

    fn options(&self) -> DownloadOptions {
        DownloadOptions {
            url: URL.into(),
            output_dir: self.out(),
            to_mp4: true,
            ..Default::default()
        }
    }

    fn ffmpeg_args(&self) -> Vec<String> {
        lines(&self.recorded("ffmpeg-args.txt"))
    }
}

fn lines(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .map(String::from)
        .collect()
}

/// The ffmpeg arguments with the input and output paths, both in the temporary folder inside
/// the output folder, swapped for names.
fn normalized(args: &[String], setup: &Setup) -> Vec<String> {
    let in_temporary_folder = |a: &str| {
        assert_eq!(
            Path::new(a).parent().unwrap().parent().unwrap(),
            setup.out()
        );
    };
    args.iter()
        .map(|a| {
            if a.ends_with("Me at the zoo.webm") {
                in_temporary_folder(a);
                "<input>".into()
            } else if a.ends_with("converted.mp4") {
                in_temporary_folder(a);
                "<output>".into()
            } else {
                a.clone()
            }
        })
        .collect()
}

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

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().into_owned()
}

fn transcoding(events: &[Progress]) -> Vec<&Progress> {
    events
        .iter()
        .filter(|e| e.stage == Stage::Transcode)
        .collect()
}

fn failure(result: Result<PathBuf, DownloadError>) -> String {
    match result {
        Err(DownloadError::Failed { message, .. }) => message,
        other => panic!("expected a failure, got {other:?}"),
    }
}

fn failure_kind(result: &Result<PathBuf, DownloadError>) -> FailureKind {
    match result {
        Err(DownloadError::Failed { kind, .. }) => *kind,
        other => panic!("expected a failure, got {other:?}"),
    }
}

const VP9_OPUS: &str = "ffprobe-codecs-vp9-opus.json";
const H264_AAC: &str = "ffprobe-codecs-h264-aac.json";

#[test]
fn a_vp9_opus_download_is_re_encoded_into_an_mp4_with_the_default_arguments() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();

    let (result, _) = run(&setup.paths(), setup.options());

    // The mp4 lands in the output folder next to nothing else: the .webm and the temporary
    // folder are gone.
    let path = result.unwrap();
    assert_eq!(path, setup.out().join("Me at the zoo.mp4"));
    assert_eq!(fs::read_to_string(&path).unwrap(), "mp4");
    assert_eq!(dir_entries(&setup.out()), ["Me at the zoo.mp4"]);
    assert_eq!(
        normalized(&setup.ffmpeg_args(), &setup),
        [
            "-v",
            "error",
            "-y",
            "-i",
            "<input>",
            "-progress",
            "pipe:1",
            "-nostats",
            "-c:v",
            "libx264",
            "-preset",
            "medium",
            "-crf",
            "18",
            "-pix_fmt",
            "yuv420p",
            "-vf",
            "scale=trunc(iw/2)*2:trunc(ih/2)*2",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "<output>",
        ]
    );
}

#[test]
fn ffprobe_is_asked_once_for_the_codecs_and_the_duration() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();

    run(&setup.paths(), setup.options()).0.unwrap();

    let args = lines(&setup.recorded("ffprobe-args.txt"));
    assert_eq!(
        args[..args.len() - 1],
        [
            "-v",
            "error",
            "-show_entries",
            "stream=codec_type,codec_name:format=duration",
            "-of",
            "json"
        ]
    );
    assert!(args.last().unwrap().ends_with("Me at the zoo.webm"));
}

#[test]
fn a_download_named_converted_is_still_converted() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    // Like the real ffmpeg, refuses to write over its own input.
    setup.ffmpeg(r#"[ "$3" = "$last" ] && exit 1; true"#);
    let options = DownloadOptions {
        filename: Some("converted".into()),
        ..setup.options()
    };
    // The fake yt-dlp ignores the name, so make it leave `converted.mp4`.
    let script = fs::read_to_string(setup.bin("yt-dlp"))
        .unwrap()
        .replace("Me at the zoo.webm", "converted.mp4");
    fs::write(setup.bin("yt-dlp"), script).unwrap();

    let path = run(&setup.paths(), options).0.unwrap();

    assert_eq!(dir_entries(&setup.out()), ["converted.mp4"]);
    assert_eq!(fs::read_to_string(path).unwrap(), "mp4");
}

#[test]
fn ffmpeg_progress_becomes_transcode_events_between_the_download_and_done() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();

    let (result, events) = run(&setup.paths(), setup.options());

    result.unwrap();
    assert_eq!(events.first().unwrap().stage, Stage::Info);
    assert_eq!(events.last().unwrap().stage, Stage::Done);
    let transcode = transcoding(&events);
    // The fixture has four blocks: two `continue`, one with `N/A` times, and the `end`.
    assert_eq!(transcode.len(), 4);
    assert!(events.iter().rev().nth(1).unwrap().stage == Stage::Transcode);

    // out_time_us=5440000 of 20.008 s at speed=10.8x
    assert!(close(transcode[0].percent, 27.189));
    assert!(close(transcode[0].eta, (20.008 - 5.44) / 10.8));
    assert_eq!(transcode[0].speed, None);
    // out_time_us=12266667, speed=12.2x
    assert!(close(transcode[1].percent, 61.305));
    // `out_time_us=N/A speed=N/A`: nothing new, so the last known position is reported
    // again and there is no speed to estimate an ETA from.
    assert!(close(transcode[2].percent, 61.305));
    assert_eq!(transcode[2].eta, None);
    // out_time_us=20000000 is a hair short of the 20.008 s duration.
    assert!(close(transcode[3].percent, 99.96));
}

#[test]
fn without_a_duration_there_is_no_percent_and_no_eta() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), None);
    setup.replay_ffmpeg();

    let (result, events) = run(&setup.paths(), setup.options());

    result.unwrap();
    let transcode = transcoding(&events);
    assert_eq!(transcode.len(), 4);
    assert!(transcode
        .iter()
        .all(|e| e.percent.is_none() && e.eta.is_none()));
}

/// A codec query answer for the given `(codec_type, codec_name)` streams, shaped like ffprobe's.
fn streams_json(streams: &[(&str, &str)]) -> String {
    let streams: Vec<String> = streams
        .iter()
        .map(|(t, n)| format!(r#"{{"codec_name":"{n}","codec_type":"{t}"}}"#))
        .collect();
    format!(r#"{{"programs":[],"streams":[{}]}}"#, streams.join(","))
}

#[test]
fn h264_and_aac_are_copied_and_everything_else_is_re_encoded() {
    // (streams, expected codec arguments between `-nostats` and `-movflags`)
    let reencode_v = [
        "-c:v",
        "libx264",
        "-preset",
        "medium",
        "-crf",
        "18",
        "-pix_fmt",
        "yuv420p",
        "-vf",
        "scale=trunc(iw/2)*2:trunc(ih/2)*2",
    ];
    type Case<'a> = (Vec<(&'a str, &'a str)>, Vec<&'a str>);
    let cases: Vec<Case> = vec![
        (
            vec![("video", "h264"), ("audio", "aac")],
            vec!["-c:v", "copy", "-c:a", "copy"],
        ),
        (
            vec![("video", "h264"), ("audio", "opus")],
            [&["-c:v", "copy"][..], &["-c:a", "aac", "-b:a", "192k"]].concat(),
        ),
        (
            vec![("video", "vp9"), ("audio", "aac")],
            [&reencode_v[..], &["-c:a", "copy"]].concat(),
        ),
        (
            vec![("video", "av1"), ("audio", "opus")],
            [&reencode_v[..], &["-c:a", "aac", "-b:a", "192k"]].concat(),
        ),
        // No audio stream: no audio arguments at all.
        (vec![("video", "h264")], vec!["-c:v", "copy"]),
        (vec![("video", "vp9")], reencode_v.to_vec()),
        // Neither stream is found: re-encode the video, and leave the audio to ffmpeg.
        (vec![], reencode_v.to_vec()),
    ];

    for (streams, expected) in cases {
        let setup = Setup::new();
        let json = setup.root.path().join("codecs.json");
        fs::write(&json, streams_json(&streams)).unwrap();
        setup.ffprobe(json.to_str().unwrap(), Some("20.008000"));
        setup.replay_ffmpeg();

        run(&setup.paths(), setup.options()).0.unwrap();

        let args = setup.ffmpeg_args();
        let codec_args = &args[8..args.len() - 3]; // after `-nostats`, before `-movflags +faststart <output>`
        assert_eq!(codec_args, expected, "{streams:?}");
        assert_eq!(
            &args[args.len() - 3..args.len() - 1],
            ["-movflags", "+faststart"]
        );
    }
}

#[test]
fn unreadable_codec_output_means_re_encode() {
    let setup = Setup::new();
    let garbage = setup.root.path().join("codecs.json");
    fs::write(&garbage, "this is not json").unwrap();
    setup.ffprobe(garbage.to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();

    run(&setup.paths(), setup.options()).0.unwrap();

    assert_eq!(setup.ffmpeg_args()[8..11], ["-c:v", "libx264", "-preset"]);
}

#[test]
fn a_warning_from_ffprobe_does_not_turn_a_copy_into_a_re_encode() {
    let setup = Setup::new();
    setup.ffprobe(fixture(H264_AAC).to_str().unwrap(), Some("20.008000"));
    // The same fake, plus a warning on stderr like the one `-v error` prints for a damaged frame.
    let script = fs::read_to_string(setup.bin("ffprobe")).unwrap().replace(
        "\ncat ",
        "\necho '[h264 @ 0x1] Invalid NAL unit size' >&2\ncat ",
    );
    fs::write(setup.bin("ffprobe"), script).unwrap();
    setup.replay_ffmpeg();

    run(&setup.paths(), setup.options()).0.unwrap();

    assert_eq!(setup.ffmpeg_args()[8..12], ["-c:v", "copy", "-c:a", "copy"]);
}

#[test]
fn crf_and_preset_are_passed_to_ffmpeg_and_bad_ones_are_rejected() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();
    let options = DownloadOptions {
        crf: 23,
        preset: "slow".into(),
        ..setup.options()
    };

    run(&setup.paths(), options).0.unwrap();

    let args = setup.ffmpeg_args();
    let at = |flag: &str| args[args.iter().position(|a| a == flag).unwrap() + 1].clone();
    assert_eq!(at("-crf"), "23");
    assert_eq!(at("-preset"), "slow");

    for (crf, preset) in [(52, "medium"), (18, "fastest"), (18, "")] {
        let options = DownloadOptions {
            crf,
            preset: preset.into(),
            ..setup.options()
        };
        assert!(
            download(&setup.paths(), options, |_| {}).is_err(),
            "crf={crf} preset={preset}"
        );
    }
}

#[test]
fn without_to_mp4_or_with_audio_only_ffmpeg_is_not_run() {
    // The second case is the quality picker's "Audio only" choice, whose crf box is hidden, so
    // a crf left out of range there must not stop the download.
    for (to_mp4, quality) in [(false, "best"), (true, "audio")] {
        let setup = Setup::new();
        setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
        setup.replay_ffmpeg();
        let options = DownloadOptions {
            to_mp4,
            quality: quality.into(),
            crf: 60,
            ..setup.options()
        };

        let (result, events) = run(&setup.paths(), options);

        assert_eq!(result.unwrap(), setup.out().join("Me at the zoo.webm"));
        assert!(!setup.recorded("ffmpeg-args.txt").exists());
        assert!(transcoding(&events).is_empty());
    }
}

#[test]
fn an_existing_mp4_is_never_overwritten() {
    let setup = Setup::new();
    setup.ffprobe(fixture(H264_AAC).to_str().unwrap(), Some("20.008000"));
    setup.replay_ffmpeg();
    fs::write(setup.out().join("Me at the zoo.mp4"), "mine").unwrap();

    let (result, _) = run(&setup.paths(), setup.options());

    assert_eq!(result.unwrap(), setup.out().join("Me at the zoo (1).mp4"));
    assert_eq!(
        fs::read_to_string(setup.out().join("Me at the zoo.mp4")).unwrap(),
        "mine"
    );
}

/// Makes `setup` convert like a real run, except that its ffmpeg only writes once another
/// ffmpeg has started too (each leaves a file in `rendezvous`), so two conversions are
/// certain to be running at the same moment. Its output says which process wrote it. Then it
/// runs `after`.
fn racing_ffmpeg(setup: &Setup, rendezvous: &Path, after: &str) {
    fs::create_dir_all(rendezvous).unwrap();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    fake_tool(
        &setup.bin_dir(),
        "ffmpeg",
        &format!(
            r#"[ "$1" = "-version" ] && {{ echo 'ffmpeg version 9.0.1'; exit 0; }}
for last; do :; done
touch '{dir}/started.'$$
while [ "$(ls '{dir}' | wc -l)" -lt 2 ]; do sleep 0.05; done
printf 'mp4 from %s' $$ > "$last"
{after}"#,
            dir = rendezvous.display(),
        ),
    );
}

#[test]
fn two_conversions_of_the_same_video_never_write_to_the_same_file() {
    let setup = Setup::new();
    racing_ffmpeg(&setup, &setup.recorded("rendezvous"), "true");
    let paths = setup.paths();

    let first = download(&paths, setup.options(), |_| {}).unwrap();
    let second = download(&paths, setup.options(), |_| {}).unwrap();
    let (first, second) = (first.wait().unwrap(), second.wait().unwrap());

    assert_ne!(first, second);
    assert_eq!(
        dir_entries(&setup.out()),
        ["Me at the zoo (1).mp4", "Me at the zoo.mp4"]
    );
    assert_ne!(
        fs::read_to_string(first).unwrap(),
        fs::read_to_string(second).unwrap(),
        "each file is what its own ffmpeg wrote"
    );
}

#[test]
fn cancelling_one_conversion_leaves_the_finished_file_of_another() {
    let (cancelled, finished) = (Setup::new(), Setup::new());
    let rendezvous = cancelled.recorded("rendezvous");
    racing_ffmpeg(&cancelled, &rendezvous, "sleep 60");
    racing_ffmpeg(&finished, &rendezvous, "true");
    let into_the_same_folder = DownloadOptions {
        output_dir: cancelled.out(),
        ..finished.options()
    };
    let hanging = download(&cancelled.paths(), cancelled.options(), |_| {}).unwrap();
    let quick = download(&finished.paths(), into_the_same_folder, |_| {}).unwrap();

    let kept = quick.wait().unwrap();
    hanging.cancel();

    assert_eq!(hanging.wait(), Err(DownloadError::Cancelled));
    assert!(fs::read_to_string(&kept).unwrap().starts_with("mp4 from "));
    assert_eq!(dir_entries(&cancelled.out()), [file_name(&kept)]);
}

#[test]
fn a_failing_ffmpeg_keeps_the_original_and_leaves_no_partial_mp4() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.ffmpeg("exit 1"); // after writing a partial output

    let (result, _) = run(&setup.paths(), setup.options());

    assert_eq!(failure_kind(&result), FailureKind::ConversionFailed);
    let original = setup.out().join("Me at the zoo.webm");
    assert!(
        matches!(&result, Err(DownloadError::Failed { saved: Some(p), .. }) if *p == original),
        "the failure names the saved original so the UI can show it"
    );
    assert_eq!(
        failure(result),
        format!(
            "Converting to .mp4 failed: ffmpeg exited with code 1. The original was saved as {}.",
            original.display()
        )
    );
    assert_eq!(fs::read_to_string(&original).unwrap(), "video");
    assert_eq!(dir_entries(&setup.out()), ["Me at the zoo.webm"]);
}

#[test]
fn a_failing_ffmpeg_reports_its_first_error_line() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    setup.ffmpeg(
        "echo 'progress=continue'\n\
         echo '[libx264 @ 0x7f8e1c] width not divisible by 2 (607x1080)' >&2\n\
         echo 'Conversion failed!' >&2\nexit 1",
    );

    let (result, _) = run(&setup.paths(), setup.options());

    assert!(failure(result).starts_with(
        "Converting to .mp4 failed: ffmpeg exited with code 1: width not divisible by 2 (607x1080)."
    ));
}

#[test]
fn a_failing_ffprobe_or_a_missing_one_is_an_error_before_ffmpeg_runs() {
    let setup = Setup::new();
    setup.ffmpeg("true");
    fake_tool(
        &setup.bin_dir(),
        "ffprobe",
        "[ \"$1\" = \"-version\" ] && { echo 'ffprobe version 9.0.1'; exit 0; }\nexit 2",
    );

    let (result, _) = run(&setup.paths(), setup.options());

    assert!(failure(result).contains("ffprobe exited with code 2"));
    assert!(!setup.recorded("ffmpeg-args.txt").exists());
    assert_eq!(dir_entries(&setup.out()), ["Me at the zoo.webm"]);

    fs::remove_file(setup.bin("ffprobe")).unwrap();
    let (result, _) = run(&setup.paths(), setup.options());
    assert!(failure(result).contains("ffprobe was not found"));
    // Never over the first one.
    assert_eq!(
        dir_entries(&setup.out()),
        ["Me at the zoo (1).webm", "Me at the zoo.webm"]
    );
}

#[test]
fn cancelling_during_the_transcode_stops_ffmpeg_and_removes_the_partial_mp4() {
    let setup = Setup::new();
    setup.ffprobe(fixture(VP9_OPUS).to_str().unwrap(), Some("20.008000"));
    // Starts a child, reports one progress block, then hangs, like a real ffmpeg that is
    // busy encoding. The pids are written before the progress: the test cancels as soon as
    // it sees progress, so writing them after raced the kill.
    let pids = setup.recorded("pids.txt");
    setup.ffmpeg(&format!(
        "sleep 60 &\necho $$ $! > '{}'\nhead -12 '{}'\nwait",
        pids.display(),
        fixture("ffmpeg-progress.txt").display()
    ));
    let (tx, rx) = mpsc::channel();
    let task = download(&setup.paths(), setup.options(), move |p| {
        if p.stage == Stage::Transcode {
            let _ = tx.send(());
        }
    })
    .unwrap();
    // Generous: on macOS each freshly written fake Tool is scanned before it first runs, and
    // the tests all start theirs at once.
    rx.recv_timeout(Duration::from_secs(60))
        .expect("transcode progress");
    // ffmpeg is writing, but the output folder is not shown a partial mp4.
    assert!(!dir_entries(&setup.out()).contains(&"Me at the zoo.mp4".to_string()));

    task.cancel();
    let result = task.wait();

    assert_eq!(result, Err(DownloadError::Cancelled));
    // Nothing is left: no partial mp4, no downloaded file, no temporary folder.
    assert_eq!(dir_entries(&setup.out()), Vec::<String>::new());
    for pid in fs::read_to_string(&pids).unwrap().split_whitespace() {
        let alive = std::process::Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success();
        assert!(!alive, "process {pid} is still running");
    }
}

/// With the real ffmpeg and ffprobe (on the `PATH` or in Homebrew's folder): a generated
/// VP9/Opus clip of odd width and height comes out as a playable H.264/AAC mp4. Run by hand:
/// `cargo test --test transcode -- --ignored`
#[test]
#[ignore = "needs a real GPL ffmpeg and ffprobe"]
fn real_ffmpeg_turns_vp9_opus_into_h264_aac() {
    let setup = Setup::new();
    let clip = setup.root.path().join("clip.webm");
    let made = std::process::Command::new("ffmpeg")
        .args(["-v", "error", "-f", "lavfi"])
        .args(["-i", "testsrc=duration=2:size=321x241:rate=25"])
        .args(["-f", "lavfi", "-i", "sine=duration=2"])
        .args(["-c:v", "libvpx-vp9", "-c:a", "libopus"])
        .arg(&clip)
        .status()
        .unwrap();
    assert!(made.success());
    fake_tool(
        &setup.bin_dir(),
        "yt-dlp",
        &format!(
            r#"[ "$1" = "--version" ] && {{ echo 1; exit 0; }}
while [ $# -gt 0 ]; do case "$1" in -P) P="$2";; esac; shift; done
cp '{}' "$P/Me at the zoo.webm""#,
            clip.display()
        ),
    );
    let mut paths = setup.paths();
    paths.search_path = None;
    paths.extra_dirs = grapplehook_lib::core::default_extra_dirs(None);

    let (result, events) = run(&paths, setup.options());

    let path = result.unwrap();
    assert_eq!(dir_entries(&setup.out()), ["Me at the zoo.mp4"]);
    let codecs = std::process::Command::new("ffprobe")
        .args(["-v", "error", "-show_entries", "stream=codec_name"])
        .args(["-of", "csv=p=0"])
        .arg(&path)
        .output()
        .unwrap();
    assert_eq!(String::from_utf8_lossy(&codecs.stdout).trim(), "h264\naac");
    let last = transcoding(&events).last().unwrap().percent;
    assert_eq!(last.map(f64::round), Some(100.0));
}
