//! Cancelling a download stops its Tool and everything the Tool started, on every platform.
//! The other tests fake Tools with shell scripts and so skip Windows, whose kill path
//! (`taskkill /T /F`) differs; this one uses the Rust fake in examples/fake_tool.rs, which
//! plain `cargo test` builds.

use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

use grapplehook_lib::core::{download, DownloadError, DownloadOptions, ToolPaths};

/// target/<profile>/examples/fake_tool. The test binary sits in target/<profile>/deps on older
/// cargo and deeper under target/<profile>/build on newer cargo, so look up from it.
fn fake_tool() -> PathBuf {
    let exe = std::env::current_exe().unwrap();
    let name = format!("examples/fake_tool{}", std::env::consts::EXE_SUFFIX);
    exe.ancestors()
        .skip(1)
        .map(|dir| dir.join(&name))
        .find(|path| path.is_file())
        .unwrap_or_else(|| {
            panic!(
                "{name} is missing above {}: run plain `cargo test`, or `cargo build --examples` first",
                exe.display()
            )
        })
}

fn is_alive(pid: &str) -> bool {
    if cfg!(windows) {
        let out = Command::new("tasklist")
            .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).contains(&format!("\"{pid}\""))
    } else {
        Command::new("kill")
            .args(["-0", pid])
            .stderr(std::process::Stdio::null())
            .status()
            .unwrap()
            .success()
    }
}

/// Waits for `check` until `wait` runs out; false if it never held.
fn eventually(wait: Duration, mut check: impl FnMut() -> bool) -> bool {
    let deadline = Instant::now() + wait;
    while !check() {
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(Duration::from_millis(50));
    }
    true
}

#[test]
fn cancel_stops_the_tool_and_its_child_and_leaves_nothing_behind() {
    let root = tempfile::tempdir().unwrap();
    let (bin, out) = (root.path().join("bin"), root.path().join("out"));
    fs::create_dir_all(&bin).unwrap();
    let yt_dlp = bin.join(format!("yt-dlp{}", std::env::consts::EXE_SUFFIX));
    fs::copy(fake_tool(), &yt_dlp).unwrap();
    let paths = ToolPaths {
        search_path: Some(bin.clone().into()),
        ..Default::default()
    };
    let options = DownloadOptions {
        url: "https://example.com/video".into(),
        output_dir: out.clone(),
        ..Default::default()
    };

    let task = download(&paths, options, |_| {}).unwrap();
    let pids_file = bin.join("pids.txt");
    // Generous: Windows scans a new executable on its first start.
    let started = eventually(Duration::from_secs(60), || pids_file.is_file());
    task.cancel();
    assert!(started, "the fake yt-dlp never started its child");
    // In a thread: a cancel that kills nothing would make `wait` hang the test run for good.
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || tx.send(task.wait()));
    let result = rx.recv_timeout(Duration::from_secs(30));
    assert_eq!(
        result,
        Ok(Err(DownloadError::Cancelled)),
        "the download did not end"
    );

    let pids: Vec<String> = fs::read_to_string(&pids_file)
        .unwrap()
        .lines()
        .map(String::from)
        .collect();
    assert_eq!(pids.len(), 2, "{pids:?}");
    assert!(
        eventually(Duration::from_secs(15), || !pids
            .iter()
            .any(|p| is_alive(p))),
        "still running: {pids:?}"
    );
    let left: Vec<_> = fs::read_dir(&out).unwrap().collect();
    assert!(left.is_empty(), "left in the output folder: {left:?}");
}
