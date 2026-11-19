//! `Running`, driven through core's public interface with fake Tools that hang: cancelling one
//! download by id, stopping every running Tool process when the app quits, and keeping Tool
//! Changes and Tool Runs out of each other's way.
#![cfg(unix)]

use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Sender};
use std::thread::{self, JoinHandle};
use std::time::Duration;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, delete_tools, get_info, install_tools, update_yt_dlp, Busy, Checksum,
    DownloadError, DownloadOptions, Format, Running, Tool, ToolChangeError, ToolFile, ToolPaths,
    ToolSource, ToolState, YtDlpUpdate,
};

mod common;
use common::{
    assert_all_dead, dir_entries, fake_tool, hang_recording_pids, is_alive, wait_for_pids,
};

type Ended = (u64, Result<PathBuf, DownloadError>);
type Update = JoinHandle<Result<YtDlpUpdate, ToolChangeError>>;

/// What a yt-dlp that is being updated does to itself: it puts a broken file at its own path.
const BREAK_YT_DLP: &str = r#"printf 'exit 1' > "$0.new"; chmod +x "$0.new"; mv "$0.new" "$0""#;

/// A yt-dlp in `dir` that answers `--version`, then starts a child, records its own pid and
/// the child's in `dir/pids.txt`, and hangs.
fn hanging_yt_dlp(dir: &Path) -> ToolPaths {
    fake_tool(
        dir,
        "yt-dlp",
        &format!(
            "[ \"$1\" = \"--version\" ] && {{ echo 2025.01.01; exit 0; }}\n{}",
            hang_recording_pids(&dir.join("pids.txt"))
        ),
    );
    ToolPaths {
        search_path: Some(dir.into()),
        ..Default::default()
    }
}

/// Try to start a download that hangs, and report how it ends on `ended`. Returns its id.
fn try_start_download(running: &Running, dir: &Path, ended: &Sender<Ended>) -> Result<u64, String> {
    let ended = ended.clone();
    running.download(
        &hanging_yt_dlp(dir),
        DownloadOptions {
            url: "https://example.com/video".into(),
            output_dir: dir.join("out"),
            ..Default::default()
        },
        |_, _| {},
        move |id, result| ended.send((id, result)).unwrap(),
    )
}

fn start_hanging_download(running: &Running, dir: &Path, ended: &Sender<Ended>) -> u64 {
    try_start_download(running, dir, ended).unwrap()
}

/// A Managed yt-dlp in `dir`, which is also the Managed Tools folder, that answers `--version`
/// and runs `on_update` for `-U`.
fn managed_yt_dlp(dir: &Path, on_update: &str) -> ToolPaths {
    fake_tool(
        dir,
        "yt-dlp",
        &format!(
            "case \" $* \" in *' --version '*) echo 2025.01.01;; *' -U '*) {on_update};; esac"
        ),
    );
    ToolPaths {
        tools_dir: Some(dir.into()),
        search_path: Some("".into()),
        ..Default::default()
    }
}

/// Start updating the Managed yt-dlp in `dir`, with an `-U` that runs `before_hanging` and then
/// hangs. Returns once it hangs, so the Tool Change is in progress.
fn start_hanging_update(
    running: &Running,
    dir: &Path,
    before_hanging: &str,
) -> (ToolPaths, Update, Vec<String>) {
    let pids_file = dir.join("pids.txt");
    let paths = managed_yt_dlp(
        dir,
        &format!("{before_hanging}; {}", hang_recording_pids(&pids_file)),
    );
    let update = {
        let (running, paths) = (running.clone(), paths.clone());
        thread::spawn(move || update_yt_dlp(&paths, &running))
    };
    let pids = wait_for_pids(&pids_file);
    (paths, update, pids)
}

/// Looks only in a Managed Tools folder that has nothing in it.
fn no_tools(dir: &Path) -> ToolPaths {
    ToolPaths {
        tools_dir: Some(dir.join("tools")),
        search_path: Some("".into()),
        ..Default::default()
    }
}

/// A source that nothing serves, so an install that is not refused fails on it.
fn unserved_source() -> ToolSource {
    ToolSource {
        url: "http://127.0.0.1:1/yt-dlp".into(),
        format: Format::Raw,
        checksum: Checksum::Pinned("0".repeat(64)),
        tools: vec![ToolFile {
            tool: Tool::YtDlp,
            inner_path: None,
        }],
    }
}

/// Quitting calls `cancel_all_and_wait`: afterwards no Tool process of any running download
/// may be left, and every download must have ended as cancelled.
#[test]
fn cancel_all_and_wait_stops_every_running_download_and_its_tools() {
    let running = Running::default();
    let (ended_tx, ended_rx) = mpsc::channel();
    let dirs: Vec<_> = (0..2).map(|_| tempfile::tempdir().unwrap()).collect();
    let ids: Vec<u64> = dirs
        .iter()
        .map(|dir| start_hanging_download(&running, dir.path(), &ended_tx))
        .collect();
    let pids: Vec<String> = dirs
        .iter()
        .flat_map(|dir| wait_for_pids(&dir.path().join("pids.txt")))
        .collect();
    assert_eq!(pids.len(), 4);
    assert!(pids.iter().all(|pid| is_alive(pid)));

    running.cancel_all_and_wait();

    let mut ended: Vec<Ended> = (0..2)
        .map(|_| ended_rx.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    ended.sort_by_key(|(id, _)| *id);
    assert_eq!(
        ended,
        [
            (ids[0], Err(DownloadError::Cancelled)),
            (ids[1], Err(DownloadError::Cancelled))
        ]
    );
    assert_all_dead(&pids);
}

#[test]
fn quitting_loses_work_while_a_download_runs_but_not_a_lookup() {
    let running = Running::default();
    let (ended_tx, ended_rx) = mpsc::channel();
    let dir = tempfile::tempdir().unwrap();
    // A lookup is a Tool Run too, but quitting during one loses nothing.
    let lookup = running.run(|_| running.quitting_loses_work()).unwrap();
    assert!(!lookup);

    let id = start_hanging_download(&running, dir.path(), &ended_tx);
    wait_for_pids(&dir.path().join("pids.txt"));
    assert!(running.quitting_loses_work());

    running.cancel(id);
    let (_, result) = ended_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    assert_eq!(result, Err(DownloadError::Cancelled));
    assert!(!running.quitting_loses_work());
}

#[test]
fn quitting_loses_work_during_a_tool_change() {
    let running = Running::default();
    // An install can be minutes of downloading that quitting throws away.
    let during = running.change(|_| running.quitting_loses_work()).unwrap();
    assert!(during);
    assert!(!running.quitting_loses_work());
}

#[test]
fn cancelling_one_download_by_id_leaves_the_others_running() {
    let running = Running::default();
    let (ended_tx, ended_rx) = mpsc::channel();
    let (first, second) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let cancelled = start_hanging_download(&running, first.path(), &ended_tx);
    start_hanging_download(&running, second.path(), &ended_tx);
    let first_pids = wait_for_pids(&first.path().join("pids.txt"));
    let second_pids = wait_for_pids(&second.path().join("pids.txt"));

    running.cancel(cancelled);

    let (id, result) = ended_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!((id, result), (cancelled, Err(DownloadError::Cancelled)));
    assert_all_dead(&first_pids);
    assert!(second_pids.iter().all(|pid| is_alive(pid)));
    assert!(ended_rx.try_recv().is_err(), "the other download ended too");

    running.cancel_all_and_wait();
    assert_all_dead(&second_pids);
}

/// Looking up a video (`yt-dlp -J`) is a Tool Run, so quitting stops it too.
#[test]
fn cancel_all_and_wait_stops_a_running_info_request() {
    let running = Running::default();
    let dir = tempfile::tempdir().unwrap();
    let paths = hanging_yt_dlp(dir.path());
    let info = {
        let running = running.clone();
        thread::spawn(move || {
            running.run(|cancel| get_info(&paths, "https://example.com/v", cancel))
        })
    };
    let pids = wait_for_pids(&dir.path().join("pids.txt"));

    running.cancel_all_and_wait();

    assert!(info.join().unwrap().unwrap().is_err());
    assert_all_dead(&pids);
}

/// Updating yt-dlp is a Tool Change, so quitting stops it too, and puts back the yt-dlp that
/// the update had already broken.
#[test]
fn quitting_during_an_update_stops_it_and_restores_yt_dlp() {
    let running = Running::default();
    let dir = tempfile::tempdir().unwrap();
    let (paths, update, pids) = start_hanging_update(&running, dir.path(), BREAK_YT_DLP);

    running.cancel_all_and_wait();

    assert!(matches!(
        update.join().unwrap(),
        Err(ToolChangeError::Cancelled)
    ));
    assert_all_dead(&pids);
    match &check_tools(&paths, &CancelHandle::default()).tools[0].state {
        ToolState::Managed { version, .. } => assert_eq!(version, "2025.01.01"),
        other => panic!("expected a working Managed Tool, got {other:?}"),
    }
    // Nothing is left beside it.
    assert_eq!(dir_entries(dir.path()), ["pids.txt", "yt-dlp"]);
}

#[test]
fn a_tool_change_is_refused_while_a_download_runs() {
    let running = Running::default();
    let (ended_tx, ended_rx) = mpsc::channel();
    let (download_dir, tools_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    start_hanging_download(&running, download_dir.path(), &ended_tx);
    wait_for_pids(&download_dir.path().join("pids.txt"));
    let managed = managed_yt_dlp(tools_dir.path(), "true");
    let empty = no_tools(tools_dir.path());

    let deleted = delete_tools(&managed, &running).unwrap_err();
    let installed = install_tools(&empty, &running, &[unserved_source()], |_| {});
    let updated = update_yt_dlp(&managed, &running);

    assert!(matches!(deleted, ToolChangeError::Busy(Busy::Runs)));
    assert_eq!(
        deleted.to_string(),
        "Wait for the downloads and lookups to finish."
    );
    assert!(matches!(installed, Err(ToolChangeError::Busy(Busy::Runs))));
    assert!(matches!(updated, Err(ToolChangeError::Busy(Busy::Runs))));
    // Nothing was touched.
    assert_eq!(dir_entries(tools_dir.path()), ["yt-dlp"]);

    // Once the download is over, the same Tool Change goes through.
    running.cancel_all_and_wait();
    let (_, result) = ended_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(result, Err(DownloadError::Cancelled));
    delete_tools(&managed, &running).unwrap();
}

#[test]
fn a_tool_change_is_refused_while_a_lookup_runs() {
    let running = Running::default();
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let lookup = {
        let running = running.clone();
        thread::spawn(move || {
            running.run(|_| {
                started_tx.send(()).unwrap();
                release_rx.recv().unwrap();
            })
        })
    };
    started_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    let tools_dir = tempfile::tempdir().unwrap();
    let managed = managed_yt_dlp(tools_dir.path(), "true");

    let err = delete_tools(&managed, &running).unwrap_err();

    assert!(matches!(err, ToolChangeError::Busy(Busy::Runs)));
    assert!(tools_dir.path().join("yt-dlp").exists());
    release_tx.send(()).unwrap();
    lookup.join().unwrap().unwrap();
    delete_tools(&managed, &running).unwrap();
}

#[test]
fn a_download_and_a_lookup_are_refused_during_a_tool_change() {
    let running = Running::default();
    let (update_dir, other_dir) = (tempfile::tempdir().unwrap(), tempfile::tempdir().unwrap());
    let (_, update, pids) = start_hanging_update(&running, update_dir.path(), "true");
    let (ended_tx, ended_rx) = mpsc::channel();

    let download = try_start_download(&running, other_dir.path(), &ended_tx);
    let lookup = running.run(|cancel| {
        get_info(
            &hanging_yt_dlp(other_dir.path()),
            "https://example.com/v",
            cancel,
        )
    });

    assert_eq!(download, Err(Busy::Change.to_string()));
    assert!(matches!(lookup, Err(Busy::Change)));
    // Neither got as far as starting its Tool.
    assert!(!other_dir.path().join("pids.txt").exists());
    assert!(ended_rx.try_recv().is_err());

    // Once the Tool Change is over, Tool Runs start again.
    running.cancel_all_and_wait();
    assert!(update.join().unwrap().is_err());
    assert_all_dead(&pids);
    running.run(|_| ()).unwrap();
}

#[test]
fn two_tool_changes_exclude_each_other() {
    let running = Running::default();
    let dir = tempfile::tempdir().unwrap();
    let (paths, update, pids) = start_hanging_update(&running, dir.path(), "true");

    let updated = update_yt_dlp(&paths, &running);
    let deleted = delete_tools(&paths, &running);
    let installed = install_tools(
        &no_tools(dir.path()),
        &running,
        &[unserved_source()],
        |_| {},
    );
    let changed = running.change(|_| ());

    for result in [updated.map(drop), deleted, installed] {
        assert!(matches!(result, Err(ToolChangeError::Busy(Busy::Change))));
    }
    assert_eq!(changed, Err(Busy::Change));
    assert_eq!(
        Busy::Change.to_string(),
        "Wait until the Tools finish changing."
    );
    assert!(dir.path().join("yt-dlp").exists());

    running.cancel_all_and_wait();
    assert!(update.join().unwrap().is_err());
    assert_all_dead(&pids);
    running.change(|_| ()).unwrap();
}
