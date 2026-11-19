//! `update_yt_dlp` driven through core's public interface, with a fake yt-dlp whose
//! `-U` behaviour each test chooses.
#![cfg(unix)]

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, update_yt_dlp, Running, ToolPaths, ToolState, YtDlpUpdate,
};

mod common;
use common::dir_entries;

/// A Managed yt-dlp in `dir` that reports the version in its `version` file and runs
/// `on_update` for `-U`.
fn managed(dir: &Path, version: &str, on_update: &str) -> ToolPaths {
    fs::write(dir.join("version"), version).unwrap();
    let path = dir.join("yt-dlp");
    fs::write(
        &path,
        format!(
            "#!/bin/sh\ncase \" $* \" in\n*' --version '*) cat '{}/version';;\n*' -U '*) {on_update};;\nesac\n",
            dir.display()
        ),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    ToolPaths {
        tools_dir: Some(dir.into()),
        search_path: Some("".into()),
        ..Default::default()
    }
}

#[test]
fn reports_the_version_before_and_after_an_update() {
    let dir = tempfile::tempdir().unwrap();
    let paths = managed(
        dir.path(),
        "2025.01.01",
        &format!("echo 2025.02.02 > '{}/version'", dir.path().display()),
    );

    let result = update_yt_dlp(&paths, &Running::default()).unwrap();

    assert_eq!(
        result,
        YtDlpUpdate {
            before: "2025.01.01".into(),
            after: "2025.02.02".into()
        }
    );
}

#[test]
fn the_update_ignores_yt_dlp_config_files() {
    let dir = tempfile::tempdir().unwrap();
    let args = dir.path().join("args");
    let paths = managed(
        dir.path(),
        "2025.01.01",
        &format!("echo \"$@\" > '{}'", args.display()),
    );

    update_yt_dlp(&paths, &Running::default()).unwrap();

    let args = fs::read_to_string(args).unwrap();
    assert!(
        args.split_whitespace().any(|a| a == "--ignore-config"),
        "{args}"
    );
}

#[test]
fn a_downgrade_is_an_error_and_the_old_version_is_put_back() {
    let dir = tempfile::tempdir().unwrap();
    let paths = managed(
        dir.path(),
        "2025.02.02",
        "printf '#!/bin/sh\\necho 2024.01.01' > \"$0.new\"; chmod +x \"$0.new\"; mv \"$0.new\" \"$0\"",
    );

    let err = update_yt_dlp(&paths, &Running::default()).unwrap_err();

    assert!(err.to_string().contains("older version"), "{err}");
    assert_managed_runs(&paths, "2025.02.02");
    assert_eq!(dir_entries(dir.path()), ["version", "yt-dlp"]);
}

#[test]
fn reports_the_same_version_when_already_up_to_date() {
    let dir = tempfile::tempdir().unwrap();
    let paths = managed(dir.path(), "2025.01.01", "echo 'yt-dlp is up to date'");

    let result = update_yt_dlp(&paths, &Running::default()).unwrap();

    assert_eq!(result.before, "2025.01.01");
    assert_eq!(result.after, "2025.01.01");
}

#[test]
fn a_failed_update_is_an_error_and_the_binary_still_runs() {
    let dir = tempfile::tempdir().unwrap();
    let paths = managed(
        dir.path(),
        "2025.01.01",
        "echo 'ERROR: unable to download' >&2; exit 100",
    );

    let err = update_yt_dlp(&paths, &Running::default()).unwrap_err();

    assert!(err.to_string().contains("unable to download"), "{err}");
    assert_managed_runs(&paths, "2025.01.01");
}

#[test]
fn an_update_that_breaks_the_binary_is_rolled_back() {
    let dir = tempfile::tempdir().unwrap();
    let paths = managed(
        dir.path(),
        "2025.01.01",
        "printf 'exit 1' > \"$0.new\"; chmod +x \"$0.new\"; mv \"$0.new\" \"$0\"; echo 'ERROR: replace failed' >&2; exit 1",
    );

    assert!(update_yt_dlp(&paths, &Running::default()).is_err());

    assert_managed_runs(&paths, "2025.01.01");
    // The copy kept aside for the rollback is gone again.
    assert_eq!(dir_entries(dir.path()), ["version", "yt-dlp"]);
}

#[test]
fn a_system_tool_is_refused_and_never_touched() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("updated");
    let mut paths = managed(
        dir.path(),
        "2025.01.01",
        &format!("touch '{}'", marker.display()),
    );
    // Not the Managed Tools folder, so the same file is a System Tool.
    paths.search_path = paths.tools_dir.take().map(Into::into);

    let err = update_yt_dlp(&paths, &Running::default()).unwrap_err();

    assert!(err.to_string().contains("installed yourself"), "{err}");
    assert!(!marker.exists());
}

fn assert_managed_runs(paths: &ToolPaths, version: &str) {
    match &check_tools(paths, &CancelHandle::default()).tools[0].state {
        ToolState::Managed { version: v, .. } => assert_eq!(v, version),
        other => panic!("expected a working Managed Tool, got {other:?}"),
    }
}
