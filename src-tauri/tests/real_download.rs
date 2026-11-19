//! Real yt-dlp and aria2c (from the PATH or the usual package manager folders) downloading a
//! short public video. Run by hand, it needs the network:
//! `cargo test --test real_download -- --ignored --nocapture`
//! The fixtures in tests/fixtures/yt-dlp-download-*.txt were recorded from runs like these.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc;
use std::time::Duration;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, default_extra_dirs, download, DownloadError, DownloadOptions, Progress, Stage,
    Tool, ToolPaths, ToolState,
};

const URL: &str = "https://www.youtube.com/watch?v=jNQXAC9IVRw";

fn tool_paths() -> ToolPaths {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    ToolPaths {
        extra_dirs: default_extra_dirs(home.as_deref()),
        ..Default::default()
    }
}

/// The same Tools minus aria2c, linked into `dir`, so yt-dlp uses its own downloader.
fn without_aria2c(dir: &Path) -> ToolPaths {
    for status in check_tools(&tool_paths(), &CancelHandle::default()).tools {
        if let ToolState::System { path } = status.state {
            if status.tool != Tool::Aria2c {
                std::os::unix::fs::symlink(&path, dir.join(path.file_name().unwrap())).unwrap();
            }
        }
    }
    ToolPaths {
        search_path: Some(dir.into()),
        ..Default::default()
    }
}

fn options(out: &Path) -> DownloadOptions {
    DownloadOptions {
        url: URL.into(),
        output_dir: out.into(),
        quality: "360".into(),
        ..Default::default()
    }
}

/// Command lines of the processes whose arguments mention `marker`.
fn processes_mentioning(marker: &str) -> Vec<String> {
    let output = Command::new("pgrep")
        .args(["-fl", marker])
        .output()
        .unwrap();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|line| line.chars().take(60).collect())
        .collect()
}

#[test]
#[ignore = "needs the network, yt-dlp and aria2c"]
fn real_downloads_finish_with_progress_with_and_without_aria2c() {
    let bin = tempfile::tempdir().unwrap();
    for (name, paths) in [
        ("aria2c", tool_paths()),
        ("yt-dlp's own downloader", without_aria2c(bin.path())),
    ] {
        let out = tempfile::tempdir().unwrap();
        let events = std::sync::Arc::new(std::sync::Mutex::new(Vec::<Progress>::new()));
        let sink = events.clone();

        let path = download(&paths, options(out.path()), move |p| {
            sink.lock().unwrap().push(p)
        })
        .unwrap()
        .wait()
        .unwrap();

        let events = events.lock().unwrap();
        let downloading: Vec<&Progress> = events
            .iter()
            .filter(|e| e.stage == Stage::Download)
            .collect();
        println!(
            "{name}: {} download events, {} with a speed, {} with an ETA; result {}",
            downloading.len(),
            downloading.iter().filter(|e| e.speed.is_some()).count(),
            downloading.iter().filter(|e| e.eta.is_some()).count(),
            path.display()
        );
        assert!(downloading.len() >= 3, "{name}: {downloading:?}");
        assert!(downloading.iter().any(|e| e.speed.is_some()), "{name}");
        assert_eq!(events.last().unwrap().stage, Stage::Done);
        assert!(fs::metadata(&path).unwrap().len() > 100_000, "{name}");
        assert_eq!(fs::read_dir(out.path()).unwrap().count(), 1, "{name}");
    }
}

#[test]
#[ignore = "needs the network, yt-dlp and aria2c"]
fn real_cancel_leaves_no_yt_dlp_or_aria2c_running() {
    let out = tempfile::tempdir().unwrap();
    // yt-dlp and the aria2c it starts both have the output folder in their arguments.
    let marker = out
        .path()
        .file_name()
        .unwrap()
        .to_string_lossy()
        .into_owned();
    let (tx, rx) = mpsc::channel();
    let task = download(&tool_paths(), options(out.path()), move |p| {
        let _ = tx.send(p);
    })
    .unwrap();

    // Wait for an aria2c readout (the percent is a whole number), so both are running.
    loop {
        let event = rx.recv_timeout(Duration::from_secs(60)).expect("progress");
        if event.stage == Stage::Download && event.eta.is_some() {
            break;
        }
    }
    let running = processes_mentioning(&marker);
    println!("running before cancel: {running:#?}");
    assert!(
        running.len() >= 2,
        "expected yt-dlp and aria2c: {running:?}"
    );

    task.cancel();
    assert_eq!(task.wait(), Err(DownloadError::Cancelled));

    std::thread::sleep(Duration::from_millis(500));
    let left = processes_mentioning(&marker);
    println!("running after cancel: {left:#?}");
    assert!(left.is_empty(), "{left:?}");
    assert_eq!(fs::read_dir(out.path()).unwrap().count(), 0);
}
