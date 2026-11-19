//! `get_info` driven through core's public interface, with a fake yt-dlp that replays
//! recorded `-J` output (tests/fixtures/yt-dlp-info.json, a real run, trimmed).
#![cfg(unix)]

use std::fs;
use std::path::Path;
use std::thread;
use std::time::Duration;

use grapplehook_lib::core::{get_info, CancelHandle, ToolPaths, TOO_SLOW};

mod common;
use common::{assert_all_dead, fake_tool, hang_recording_pids, wait_for_pids};

const URL: &str = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";

/// A yt-dlp that answers `--version` and replays the fixture, but only for the exact
/// arguments core must pass.
fn replaying_yt_dlp(dir: &Path) -> ToolPaths {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yt-dlp-info.json");
    fake_tool(
        dir,
        "yt-dlp",
        &format!(
            r#"[ "$1" = "--version" ] && {{ echo 2025.01.01; exit 0; }}
[ "$*" = "--ignore-config --no-playlist --flat-playlist -J -- {URL}" ] || {{ echo "unexpected args: $*" >&2; exit 2; }}
cat '{}'"#,
            fixture.display()
        ),
    );
    ToolPaths {
        search_path: Some(dir.into()),
        ..Default::default()
    }
}

#[test]
fn parses_the_recorded_info() {
    let dir = tempfile::tempdir().unwrap();

    let info = get_info(&replaying_yt_dlp(dir.path()), URL, &CancelHandle::default()).unwrap();

    assert_eq!(info.id, "dQw4w9WgXcQ");
    assert_eq!(
        info.title,
        "Rick Astley - Never Gonna Give You Up (Official Video) (4K Remaster)"
    );
    assert_eq!(info.uploader.as_deref(), Some("Rick Astley"));
    assert_eq!(info.duration_seconds, Some(213.0));
    assert!(info.thumbnail.unwrap().starts_with("https://"));
}

#[test]
fn heights_are_distinct_descending_and_video_only() {
    let dir = tempfile::tempdir().unwrap();

    let info = get_info(&replaying_yt_dlp(dir.path()), URL, &CancelHandle::default()).unwrap();

    // The fixture has several codecs per height, audio-only formats (no height) and
    // storyboards (a height, but vcodec "none") that must not show up.
    assert_eq!(info.heights, [2160, 1440, 1080, 720, 480, 360, 240, 144]);
}

#[test]
fn uploader_falls_back_to_channel_and_missing_fields_become_defaults() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
echo '{"channel":"Some Channel","formats":[{"format_id":"1","width":640,"height":360,"vcodec":"avc1"}]}'"#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let info = get_info(&paths, URL, &CancelHandle::default()).unwrap();

    assert_eq!(info.id, "");
    assert_eq!(info.title, "");
    assert_eq!(info.uploader.as_deref(), Some("Some Channel"));
    assert_eq!(info.duration_seconds, None);
    assert_eq!(info.heights, [360]);
}

#[test]
fn a_height_whose_codec_yt_dlp_does_not_know_is_still_offered() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
echo '{"formats":[{"format_id":"hls-720","height":720},{"format_id":"sb0","height":90,"vcodec":"none"},{"format_id":"a","vcodec":"none"}]}'"#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let info = get_info(&paths, URL, &CancelHandle::default()).unwrap();

    assert_eq!(info.heights, [720]);
}

#[test]
fn a_playlist_or_channel_link_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
echo '{"_type":"playlist","title":"A playlist","entries":[{"url":"https://example.com/1"}]}'"#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let err = get_info(&paths, URL, &CancelHandle::default()).unwrap_err();

    assert_eq!(
        err,
        "This link is a playlist or a channel. Open one video and paste its link."
    );
}

#[test]
fn surrounding_whitespace_in_the_url_is_trimmed() {
    let dir = tempfile::tempdir().unwrap();

    let info = get_info(
        &replaying_yt_dlp(dir.path()),
        &format!("  {URL}\n"),
        &CancelHandle::default(),
    )
    .unwrap();

    assert_eq!(info.id, "dQw4w9WgXcQ");
}

#[test]
fn a_non_http_url_is_rejected_before_yt_dlp_runs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = replaying_yt_dlp(dir.path());

    for bad in ["", "not a url", "ftp://example.com/v", "file:///etc/passwd"] {
        assert_eq!(
            get_info(&paths, bad, &CancelHandle::default()).unwrap_err(),
            "Enter a valid http(s) URL."
        );
    }
}

#[test]
fn a_yt_dlp_failure_reports_yt_dlps_last_error_sentence() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
echo "ERROR: [youtube] abc: An earlier error" >&2
echo "ERROR: [youtube] abc: Video unavailable" >&2
echo "WARNING: something minor" >&2
exit 1"#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let err = get_info(&paths, URL, &CancelHandle::default()).unwrap_err();

    assert_eq!(err, "abc: Video unavailable");
}

#[test]
fn without_an_error_line_the_exit_code_and_400_characters_of_stderr_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
printf 'x%.0s' $(seq 1 1000) >&2
exit 1"#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let err = get_info(&paths, URL, &CancelHandle::default()).unwrap_err();

    assert_eq!(
        err,
        format!("yt-dlp exited with code 1: {}", "x".repeat(400))
    );
}

#[test]
fn output_that_is_not_json_is_an_error() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(
        dir.path(),
        "yt-dlp",
        r#"[ "$1" = "--version" ] && exit 0
echo "not json""#,
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let err = get_info(&paths, URL, &CancelHandle::default()).unwrap_err();

    assert!(err.starts_with("Could not read yt-dlp output"), "{err}");
}

#[test]
fn a_missing_yt_dlp_is_a_clear_error() {
    let empty = tempfile::tempdir().unwrap();
    let paths = ToolPaths {
        search_path: Some(empty.path().into()),
        ..Default::default()
    };

    let err = get_info(&paths, URL, &CancelHandle::default()).unwrap_err();

    assert!(err.contains("yt-dlp"), "{err}");
    assert!(err.contains("not found"), "{err}");
}

#[test]
fn cancelling_stops_a_hanging_yt_dlp_and_everything_it_started() {
    let dir = tempfile::tempdir().unwrap();
    let pids_file = dir.path().join("pids.txt");
    fake_tool(
        dir.path(),
        "yt-dlp",
        &format!(
            "[ \"$1\" = \"--version\" ] && {{ echo 2025.01.01; exit 0; }}\n{}",
            hang_recording_pids(&pids_file)
        ),
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };
    let cancel = CancelHandle::default();
    let request = {
        let cancel = cancel.clone();
        thread::spawn(move || get_info(&paths, URL, &cancel))
    };
    let pids = wait_for_pids(&pids_file);

    cancel.cancel();

    assert_eq!(request.join().unwrap(), Err("Cancelled".to_string()));
    assert_all_dead(&pids);
}

#[test]
fn a_lookup_past_its_deadline_stops_yt_dlp_and_says_it_took_too_long() {
    let dir = tempfile::tempdir().unwrap();
    let pids_file = dir.path().join("pids.txt");
    fake_tool(
        dir.path(),
        "yt-dlp",
        &format!(
            "[ \"$1\" = \"--version\" ] && {{ echo 2025.01.01; exit 0; }}\n{}",
            hang_recording_pids(&pids_file)
        ),
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };
    // A sooner deadline from the caller wins over the lookup's own. Not 1 second: under a full
    // parallel run macOS can take that long to scan and start the fake, which then never
    // writes its pids.
    let cancel = CancelHandle::default().with_deadline(Duration::from_secs(5));

    let result = get_info(&paths, URL, &cancel);

    assert_eq!(result, Err(TOO_SLOW.to_string()));
    assert_all_dead(&wait_for_pids(&pids_file));
}

#[test]
fn a_request_that_is_already_cancelled_returns_cancelled() {
    let dir = tempfile::tempdir().unwrap();
    let paths = replaying_yt_dlp(dir.path());
    let cancel = CancelHandle::default();
    cancel.cancel();

    assert_eq!(get_info(&paths, URL, &cancel), Err("Cancelled".to_string()));
}

#[test]
fn info_serializes_with_the_field_names_the_ui_reads() {
    let dir = tempfile::tempdir().unwrap();
    let info = get_info(&replaying_yt_dlp(dir.path()), URL, &CancelHandle::default()).unwrap();

    let json = serde_json::to_value(&info).unwrap();

    assert_eq!(json["durationSeconds"], 213.0);
    assert_eq!(json["heights"][0], 2160);
}

#[test]
fn a_managed_yt_dlp_is_run_without_asking_its_version_first() {
    let dir = tempfile::tempdir().unwrap();
    let calls = dir.path().join("calls.txt");
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yt-dlp-info.json");
    // A Managed yt-dlp (in the tools folder) that records each run's arguments.
    fake_tool(
        dir.path(),
        "yt-dlp",
        &format!(
            "echo \"$*\" >> '{}'\ncat '{}'",
            calls.display(),
            fixture.display()
        ),
    );
    let paths = ToolPaths {
        tools_dir: Some(dir.path().into()),
        search_path: Some("".into()),
        ..Default::default()
    };

    let info = get_info(&paths, URL, &CancelHandle::default()).unwrap();

    assert_eq!(info.id, "dQw4w9WgXcQ");
    assert_eq!(
        fs::read_to_string(&calls).unwrap(),
        format!("--ignore-config --no-playlist --flat-playlist -J -- {URL}\n")
    );
}

#[test]
fn output_that_arrives_after_yt_dlp_has_exited_is_still_read() {
    let dir = tempfile::tempdir().unwrap();
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/yt-dlp-info.json");
    // yt-dlp exits at once while a process it started still holds stdout and writes the info
    // a second later: the pipe closes long after the exit, as it can on a busy machine.
    fake_tool(
        dir.path(),
        "yt-dlp",
        &format!("(sleep 1; cat '{}') &", fixture.display()),
    );
    let paths = ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    };

    let info = get_info(&paths, URL, &CancelHandle::default()).unwrap();

    assert_eq!(info.id, "dQw4w9WgXcQ");
}
