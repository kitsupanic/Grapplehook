//! `install_tools` and Managed Tool lookup, driven through core's public interface
//! against a local HTTP server that stands in for the GitHub release.
#![cfg(unix)]

use std::fs;
use std::path::{Path, PathBuf};

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, default_extra_dirs, delete_tools, install_size, install_tools, Checksum, Format,
    InstallProgress, Running, Tool, ToolChangeError, ToolFile, ToolPaths, ToolSource, ToolState,
};

mod common;
use common::{dir_entries, fake_tool, missing, serve, serve_with_log, Reply};

const FAKE_YT_DLP: &str = "#!/bin/sh\necho 2026.10.26\n";
// From `printf '#!/bin/sh\necho 2026.10.26\n' | shasum -a 256`.
const FAKE_YT_DLP_SHA256: &str = "90cfa38c35aad8d8dd8cfd98378d32fe9a91c4810542233a08eee196cd684301";

fn yt_dlp_source(base: &str) -> ToolSource {
    ToolSource {
        url: format!("{base}/download/yt-dlp_fake"),
        format: Format::Raw,
        checksum: Checksum::SumsFile {
            url: format!("{base}/download/SHA2-256SUMS"),
        },
        tools: vec![ToolFile {
            tool: Tool::YtDlp,
            inner_path: None,
        }],
    }
}

/// A sums file in the `<sha256>  <asset name>` format of the yt-dlp release.
fn sums_file(hash: &str) -> Vec<u8> {
    format!("{}  other_asset\n{hash}  yt-dlp_fake\n", "0".repeat(64)).into_bytes()
}

/// Look only in `tools_dir`, never on the real PATH.
fn tool_paths(tools_dir: &Path) -> ToolPaths {
    ToolPaths {
        tools_dir: Some(tools_dir.to_path_buf()),
        search_path: Some("".into()),
        ..Default::default()
    }
}

#[test]
fn good_download_is_installed_and_reported_as_managed_with_its_version() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
        ),
    ]);
    let app_data = tempfile::tempdir().unwrap();
    let tools_dir = app_data.path().join("tools");
    let mut progress = Vec::<InstallProgress>::new();

    install_tools(
        &tool_paths(&tools_dir),
        &Running::default(),
        &[yt_dlp_source(&base)],
        |p| progress.push(p),
    )
    .unwrap();

    let installed = tools_dir.join("yt-dlp");
    assert_eq!(fs::read_to_string(&installed).unwrap(), FAKE_YT_DLP);
    assert_eq!(dir_entries(&tools_dir), ["yt-dlp"]);
    let status = check_tools(&tool_paths(&tools_dir), &CancelHandle::default()).tools;
    assert_eq!(
        status[0].state,
        ToolState::Managed {
            path: installed,
            version: "2026.10.26".into()
        }
    );
    let last = progress.last().expect("progress reported");
    assert_eq!(last.tool, Tool::YtDlp);
    assert_eq!(last.downloaded, FAKE_YT_DLP.len() as u64);
    assert_eq!(last.total, Some(FAKE_YT_DLP.len() as u64));
}

#[test]
fn delete_tools_removes_managed_tools_and_keeps_system_tools() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
        ),
    ]);
    let app_data = tempfile::tempdir().unwrap();
    let tools_dir = app_data.path().join("tools");
    let system = tempfile::tempdir().unwrap();
    fake_tool(system.path(), "ffmpeg", "echo ffmpeg version 7.1");
    let paths = ToolPaths {
        search_path: Some(system.path().into()),
        ..tool_paths(&tools_dir)
    };
    install_tools(&paths, &Running::default(), &[yt_dlp_source(&base)], |_| {}).unwrap();

    delete_tools(&paths, &Running::default()).unwrap();

    assert!(!tools_dir.exists());
    let status = check_tools(&paths, &CancelHandle::default()).tools;
    assert_eq!(status[0].state, ToolState::Missing);
    assert!(matches!(status[1].state, ToolState::System { .. }));
    assert!(system.path().join("ffmpeg").exists());
    // Deleting again, with nothing left, is fine.
    delete_tools(&paths, &Running::default()).unwrap();
}

#[test]
fn download_behind_a_redirect_is_followed() {
    let base = serve(vec![
        ("/latest/yt-dlp_fake", Reply::Redirect("/cdn/yt-dlp_fake")),
        ("/cdn/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/latest/SHA2-256SUMS",
            Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
        ),
    ]);
    let mut source = yt_dlp_source(&base);
    source.url = format!("{base}/latest/yt-dlp_fake");
    source.checksum = Checksum::SumsFile {
        url: format!("{base}/latest/SHA2-256SUMS"),
    };
    let tools_dir = tempfile::tempdir().unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["yt-dlp"]);
}

#[test]
fn checksum_mismatch_fails_and_leaves_the_tools_folder_unchanged() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(sums_file(&"f".repeat(64))),
        ),
    ]);
    let tools_dir = tempfile::tempdir().unwrap();
    fs::write(tools_dir.path().join("existing"), "keep me").unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[yt_dlp_source(&base)],
        |_| {},
    )
    .unwrap_err();

    assert!(
        matches!(err, ToolChangeError::ChecksumMismatch { .. }),
        "{err}"
    );
    assert_eq!(dir_entries(tools_dir.path()), ["existing"]);
}

#[test]
fn missing_asset_fails_and_leaves_the_tools_folder_unchanged() {
    let base = serve(vec![(
        "/download/SHA2-256SUMS",
        Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
    )]);
    let tools_dir = tempfile::tempdir().unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[yt_dlp_source(&base)],
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, ToolChangeError::Download(_)), "{err}");
    assert!(dir_entries(tools_dir.path()).is_empty());
}

#[test]
fn sums_file_without_the_asset_fails_and_leaves_the_tools_folder_unchanged() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(format!("{}  unrelated\n", "0".repeat(64)).into_bytes()),
        ),
    ]);
    let tools_dir = tempfile::tempdir().unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[yt_dlp_source(&base)],
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, ToolChangeError::ChecksumMissing(_)), "{err}");
    assert!(dir_entries(tools_dir.path()).is_empty());
}

#[test]
fn a_tool_the_user_already_has_is_not_downloaded_again() {
    let (base, log) = serve_with_log(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
        ),
    ]);
    let sources = [yt_dlp_source(&base)];
    let tools_dir = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    fake_tool(system.path(), "yt-dlp", "echo 1.0.0");
    let as_system_tool = ToolPaths {
        search_path: Some(system.path().into()),
        ..tool_paths(tools_dir.path())
    };

    install_tools(&as_system_tool, &Running::default(), &sources, |_| {}).unwrap();
    assert_eq!(install_size(&missing(&as_system_tool), &sources), Some(0));

    // The same for one the app installed itself earlier.
    fs::remove_dir_all(system.path()).unwrap();
    fake_tool(tools_dir.path(), "yt-dlp", "echo 2.0.0");
    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &sources,
        |_| {},
    )
    .unwrap();
    assert_eq!(
        install_size(&missing(&tool_paths(tools_dir.path())), &sources),
        Some(0)
    );

    assert_eq!(*log.lock().unwrap(), Vec::<String>::new());
    assert_eq!(dir_entries(tools_dir.path()), ["yt-dlp"]);
}

#[test]
fn a_system_tool_that_does_not_run_is_installed_like_a_missing_one() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(sums_file(FAKE_YT_DLP_SHA256)),
        ),
    ]);
    let tools_dir = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    fake_tool(system.path(), "yt-dlp", "exit 1");
    let paths = ToolPaths {
        search_path: Some(system.path().into()),
        ..tool_paths(tools_dir.path())
    };

    install_tools(&paths, &Running::default(), &[yt_dlp_source(&base)], |_| {}).unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["yt-dlp"]);
}

#[test]
fn only_the_sources_of_missing_tools_are_downloaded_and_sized() {
    let (base, log) = serve_with_log(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        ("/download/aria2c_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(
                format!(
                    "{h}  yt-dlp_fake\n{h}  aria2c_fake\n",
                    h = FAKE_YT_DLP_SHA256
                )
                .into_bytes(),
            ),
        ),
    ]);
    let aria2c = ToolSource {
        url: format!("{base}/download/aria2c_fake"),
        tools: vec![ToolFile {
            tool: Tool::Aria2c,
            inner_path: None,
        }],
        ..yt_dlp_source(&base)
    };
    let sources = [yt_dlp_source(&base), aria2c];
    let tools_dir = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    fake_tool(system.path(), "yt-dlp", "echo 1.0.0");
    let paths = ToolPaths {
        search_path: Some(system.path().into()),
        ..tool_paths(tools_dir.path())
    };

    assert_eq!(
        install_size(&missing(&paths), &sources),
        Some(FAKE_YT_DLP.len() as u64),
        "the size of aria2c's download alone"
    );
    install_tools(&paths, &Running::default(), &sources, |_| {}).unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["aria2c"]);
    assert!(log.lock().unwrap().iter().all(|r| !r.contains("yt-dlp")));
}

#[test]
fn a_tool_that_cannot_be_put_in_place_undoes_the_ones_already_installed() {
    let base = serve(vec![
        ("/download/yt-dlp_fake", Reply::Ok(FAKE_YT_DLP.into())),
        ("/download/ffmpeg_fake", Reply::Ok(FAKE_YT_DLP.into())),
        ("/download/aria2c_fake", Reply::Ok(FAKE_YT_DLP.into())),
        (
            "/download/SHA2-256SUMS",
            Reply::Ok(
                format!(
                    "{h}  yt-dlp_fake\n{h}  ffmpeg_fake\n{h}  aria2c_fake\n",
                    h = FAKE_YT_DLP_SHA256
                )
                .into_bytes(),
            ),
        ),
    ]);
    let source = |tool, asset: &str| ToolSource {
        url: format!("{base}/download/{asset}"),
        tools: vec![ToolFile {
            tool,
            inner_path: None,
        }],
        ..yt_dlp_source(&base)
    };
    let tools_dir = tempfile::tempdir().unwrap();
    // A broken yt-dlp to be replaced first, no ffmpeg yet, and a folder where aria2c has to
    // go, which no file can replace (the same as a locked aria2c.exe on Windows).
    let old = fake_tool(tools_dir.path(), "yt-dlp", "exit 1");
    let old_content = fs::read_to_string(&old).unwrap();
    fs::create_dir(tools_dir.path().join("aria2c")).unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[
            source(Tool::YtDlp, "yt-dlp_fake"),
            source(Tool::Ffmpeg, "ffmpeg_fake"),
            source(Tool::Aria2c, "aria2c_fake"),
        ],
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, ToolChangeError::Io(_)), "{err}");
    assert_eq!(fs::read_to_string(&old).unwrap(), old_content);
    assert_eq!(dir_entries(tools_dir.path()), ["aria2c", "yt-dlp"]);
}

#[test]
fn managed_tool_is_preferred_over_a_system_tool() {
    let tools_dir = tempfile::tempdir().unwrap();
    let managed = fake_tool(tools_dir.path(), "yt-dlp", "echo 2.0.0");
    let bin = tempfile::tempdir().unwrap();
    fake_tool(bin.path(), "yt-dlp", "echo 1.0.0");

    let status = check_tools(
        &ToolPaths {
            tools_dir: Some(tools_dir.path().into()),
            search_path: Some(bin.path().into()),
            ..Default::default()
        },
        &CancelHandle::default(),
    )
    .tools;

    assert_eq!(
        status[0].state,
        ToolState::Managed {
            path: managed,
            version: "2.0.0".into()
        }
    );
}

#[test]
/// A download runs the Managed file whenever it is there, so the Tools screen must not show
/// a working System Tool in its place: it shows Missing, and Install Tools replaces the file.
fn broken_managed_tool_is_missing_even_with_a_system_tool() {
    let tools_dir = tempfile::tempdir().unwrap();
    fake_tool(tools_dir.path(), "yt-dlp", "exit 1");
    let bin = tempfile::tempdir().unwrap();
    fake_tool(bin.path(), "yt-dlp", "echo 1.0.0");

    let status = check_tools(
        &ToolPaths {
            tools_dir: Some(tools_dir.path().into()),
            search_path: Some(bin.path().into()),
            ..Default::default()
        },
        &CancelHandle::default(),
    )
    .tools;

    assert_eq!(status[0].state, ToolState::Missing);
}

#[test]
fn system_tool_is_found_in_extra_dirs_outside_the_path() {
    let empty = tempfile::tempdir().unwrap();
    let extra = tempfile::tempdir().unwrap();
    let system = fake_tool(extra.path(), "yt-dlp", "echo 1.0.0");

    let status = check_tools(
        &ToolPaths {
            search_path: Some(empty.path().into()),
            extra_dirs: vec![extra.path().into()],
            ..Default::default()
        },
        &CancelHandle::default(),
    )
    .tools;

    assert_eq!(status[0].state, ToolState::System { path: system });
}

#[test]
fn default_extra_dirs_cover_homebrew_macports_and_the_users_local_bin() {
    let dirs = default_extra_dirs(Some(Path::new("/home/me")));

    for expected in [
        "/opt/homebrew/bin",
        "/usr/local/bin",
        "/opt/local/bin",
        "/home/me/.local/bin",
    ] {
        assert!(dirs.contains(&PathBuf::from(expected)), "{expected}");
    }
}
