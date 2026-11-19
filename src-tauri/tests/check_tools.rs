//! `check_tools` driven through core's public interface, with fake Tool scripts.
#![cfg(unix)]

use std::fs;
use std::path::Path;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{check_tools, Tool, ToolPaths, ToolState};

mod common;
use common::fake_tool;

fn yt_dlp_state(paths: &ToolPaths) -> ToolState {
    let status = check_tools(paths, &CancelHandle::default()).tools;
    let yt = status.iter().find(|s| s.tool == Tool::YtDlp).unwrap();
    assert!(yt.required);
    yt.state.clone()
}

/// On Linux, running a file that some process still holds open for writing fails with
/// "Text file busy". That is what happens when another thread forks right after a Tool
/// was written (an install, or a parallel test's fake), so core must wait it out.
#[test]
fn tool_still_open_for_writing_is_found_once_released() {
    let dir = tempfile::tempdir().unwrap();
    let fake = fake_tool(dir.path(), "yt-dlp", "echo 2025.01.01");
    let writer = fs::OpenOptions::new().append(true).open(&fake).unwrap();
    let release = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(100));
        drop(writer);
    });

    let state = yt_dlp_state(&ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    });

    release.join().unwrap();
    assert_eq!(state, ToolState::System { path: fake });
}

#[test]
fn a_tool_that_fails_to_run_is_missing() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(dir.path(), "yt-dlp", "exit 1");

    let state = yt_dlp_state(&ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    });

    assert_eq!(state, ToolState::Missing);
}

/// A Tool stuck on its version command must not hold up the check (and every Tool Change).
#[test]
fn a_tool_that_hangs_on_its_version_command_is_missing_after_a_deadline() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(dir.path(), "yt-dlp", "sleep 600");

    let started = std::time::Instant::now();
    let state = yt_dlp_state(&ToolPaths {
        search_path: Some(dir.path().into()),
        ..Default::default()
    });

    assert_eq!(state, ToolState::Missing);
    assert!(started.elapsed() < std::time::Duration::from_secs(30));
}

#[test]
fn quitting_stops_a_tool_that_hangs_on_its_version_command() {
    let dir = tempfile::tempdir().unwrap();
    fake_tool(dir.path(), "yt-dlp", "sleep 600");
    let cancel = CancelHandle::default();
    let quit = cancel.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(500));
        quit.cancel();
    });

    let started = std::time::Instant::now();
    let report = check_tools(
        &ToolPaths {
            search_path: Some(dir.path().into()),
            ..Default::default()
        },
        &cancel,
    );

    assert_eq!(report.tools[0].state, ToolState::Missing);
    // Well before the 15-second version deadline.
    assert!(started.elapsed() < std::time::Duration::from_secs(10));
}

#[test]
fn yt_dlp_on_the_search_path_is_reported_as_system_tool_with_its_path() {
    let empty = tempfile::tempdir().unwrap();
    let bin = tempfile::tempdir().unwrap();
    let fake = fake_tool(bin.path(), "yt-dlp", "echo 2025.01.01");
    let search_path = std::env::join_paths([empty.path(), bin.path()]).unwrap();

    let state = yt_dlp_state(&ToolPaths {
        search_path: Some(search_path),
        ..Default::default()
    });

    assert_eq!(state, ToolState::System { path: fake });
}

#[test]
fn yt_dlp_absent_from_the_search_path_is_missing() {
    let empty = tempfile::tempdir().unwrap();

    let state = yt_dlp_state(&ToolPaths {
        search_path: Some(empty.path().into()),
        ..Default::default()
    });

    assert_eq!(state, ToolState::Missing);
}

#[test]
fn the_report_says_whether_every_required_tool_is_present() {
    let system = tempfile::tempdir().unwrap();
    let managed = tempfile::tempdir().unwrap();
    let report = || {
        check_tools(
            &ToolPaths {
                search_path: Some(system.path().into()),
                tools_dir: Some(managed.path().into()),
                ..Default::default()
            },
            &CancelHandle::default(),
        )
    };
    assert!(!report().required_present, "nothing is installed");

    fake_tool(system.path(), "ffmpeg", "echo 'ffmpeg version 9.0'");
    fake_tool(system.path(), "ffprobe", "echo 'ffprobe version 9.0'");
    assert!(!report().required_present, "yt-dlp is still missing");

    // A Managed Tool counts as much as a System Tool. aria2c is optional.
    fake_tool(managed.path(), "yt-dlp", "echo 2025.01.01");
    let report = report();
    assert!(report.required_present);
    assert_eq!(report.tools.len(), 4);

    fs::remove_file(system.path().join("ffprobe")).unwrap();
    assert!(
        !check_tools(
            &ToolPaths {
                search_path: Some(system.path().into()),
                tools_dir: Some(managed.path().into()),
                ..Default::default()
            },
            &CancelHandle::default(),
        )
        .required_present
    );
}

#[test]
fn the_report_serializes_with_the_field_names_the_ui_reads() {
    let empty = tempfile::tempdir().unwrap();

    let report = check_tools(
        &ToolPaths {
            search_path: Some(empty.path().into()),
            ..Default::default()
        },
        &CancelHandle::default(),
    );

    let json = serde_json::to_value(&report).unwrap();
    assert_eq!(json["requiredPresent"], false);
    assert_eq!(json["tools"][0]["tool"], "ytDlp");
    assert_eq!(json["tools"][0]["state"]["kind"], "missing");
}

#[test]
fn aria2c_is_an_optional_tool_found_like_yt_dlp() {
    let bin = tempfile::tempdir().unwrap();
    let empty = tempfile::tempdir().unwrap();
    let fake = fake_tool(bin.path(), "aria2c", "echo 'aria2 version 1.37.0'");
    let aria2c = |search_path: &Path| {
        check_tools(
            &ToolPaths {
                search_path: Some(search_path.into()),
                ..Default::default()
            },
            &CancelHandle::default(),
        )
        .tools
        .into_iter()
        .find(|s| s.tool == Tool::Aria2c)
        .unwrap()
    };

    let on_path = aria2c(bin.path());
    let missing = aria2c(empty.path());

    assert!(!on_path.required);
    assert_eq!(on_path.state, ToolState::System { path: fake });
    assert_eq!(missing.state, ToolState::Missing);
}
