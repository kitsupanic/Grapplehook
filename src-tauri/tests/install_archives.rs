//! Installing ffmpeg, ffprobe and aria2c: archives (zip, tar.xz), pinned checksums and
//! checksums found through a redirect, all against a local HTTP server and fixture archives.
#![cfg(unix)]

use std::fs;
use std::io::Write;
use std::path::Path;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, default_sources, install_size, install_tools, Checksum, Format, Running, Tool,
    ToolChangeError, ToolFile, ToolPaths, ToolSource, ToolState,
};

mod common;
use common::{dir_entries, fake_tool, missing, serve, serve_with_log, sha256_hex, Reply};

const FFMPEG: &str = "#!/bin/sh\n[ \"$1\" = -version ] || exit 1\necho 'ffmpeg version n9.0'\necho 'built with gcc'\n";
const FFPROBE: &str = "#!/bin/sh\n[ \"$1\" = -version ] || exit 1\necho 'ffprobe version n9.0'\n";
const FFPLAY: &str = "#!/bin/sh\necho ffplay\n";

fn zip_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut zip = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    for (name, body) in files {
        zip.start_file(*name, zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(body.as_bytes()).unwrap();
    }
    zip.finish().unwrap().into_inner()
}

fn tar_xz_of(files: &[(&str, &str)]) -> Vec<u8> {
    let mut tar = tar::Builder::new(Vec::new());
    for (name, body) in files {
        let mut header = tar::Header::new_gnu();
        header.set_size(body.len() as u64);
        header.set_mode(0o644); // the installer must set the executable bit itself
        header.set_cksum();
        tar.append_data(&mut header, name, body.as_bytes()).unwrap();
    }
    let mut xz = liblzma::write::XzEncoder::new(Vec::new(), 1);
    xz.write_all(&tar.into_inner().unwrap()).unwrap();
    xz.finish().unwrap()
}

/// The BtbN layout: `<top dir>/bin/<tool>`, with other files alongside.
const BTBN_FILES: [(&str, &str); 4] = [
    ("ffmpeg-n9.0/bin/ffmpeg", FFMPEG),
    ("ffmpeg-n9.0/bin/ffprobe", FFPROBE),
    ("ffmpeg-n9.0/bin/ffplay", FFPLAY),
    ("ffmpeg-n9.0/LICENSE.txt", "GPL"),
];

fn ffmpeg_and_ffprobe(url: String, format: Format, checksum: Checksum) -> ToolSource {
    ToolSource {
        url,
        format,
        checksum,
        tools: vec![
            ToolFile {
                tool: Tool::Ffmpeg,
                inner_path: Some("bin/ffmpeg".into()),
            },
            ToolFile {
                tool: Tool::Ffprobe,
                inner_path: Some("bin/ffprobe".into()),
            },
        ],
    }
}

fn sums(hash: &str, name: &str) -> Vec<u8> {
    format!("{}  other.zip\n{hash}  {name}\n", "0".repeat(64)).into_bytes()
}

fn tool_paths(tools_dir: &Path) -> ToolPaths {
    ToolPaths {
        tools_dir: Some(tools_dir.to_path_buf()),
        search_path: Some("".into()),
        ..Default::default()
    }
}

fn state_of(tool: Tool, tools_dir: &Path) -> ToolState {
    check_tools(&tool_paths(tools_dir), &CancelHandle::default())
        .tools
        .into_iter()
        .find(|s| s.tool == tool)
        .unwrap()
        .state
}

#[test]
fn one_zip_yields_ffmpeg_and_ffprobe_with_a_single_download() {
    let archive = zip_of(&BTBN_FILES);
    let (base, log) = serve_with_log(vec![
        ("/dl/ffmpeg.zip", Reply::Ok(archive.clone())),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&sha256_hex(&archive), "ffmpeg.zip")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.zip"),
        Format::Zip,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg", "ffprobe"]);
    let ToolState::Managed { version, .. } = state_of(Tool::Ffmpeg, tools_dir.path()) else {
        panic!("ffmpeg is not Managed");
    };
    assert_eq!(version, "n9.0");
    assert!(matches!(
        state_of(Tool::Ffprobe, tools_dir.path()),
        ToolState::Managed { .. }
    ));
    let downloads = log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| *r == "GET /dl/ffmpeg.zip")
        .count();
    assert_eq!(downloads, 1);
}

#[test]
fn an_archive_is_downloaded_when_either_of_its_tools_is_missing_and_not_otherwise() {
    let archive = zip_of(&BTBN_FILES);
    let (base, log) = serve_with_log(vec![
        ("/dl/ffmpeg.zip", Reply::Ok(archive.clone())),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&sha256_hex(&archive), "ffmpeg.zip")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.zip"),
        Format::Zip,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();
    let system = tempfile::tempdir().unwrap();
    // The user has ffmpeg but not ffprobe.
    fake_tool(system.path(), "ffmpeg", "echo 'ffmpeg version 8.0'");
    let paths = ToolPaths {
        search_path: Some(system.path().into()),
        ..tool_paths(tools_dir.path())
    };

    install_tools(
        &paths,
        &Running::default(),
        std::slice::from_ref(&source),
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg", "ffprobe"]);
    let requests = log.lock().unwrap().len();
    assert!(requests > 0, "the archive was downloaded");

    // Now neither is missing.
    install_tools(
        &paths,
        &Running::default(),
        std::slice::from_ref(&source),
        |_| {},
    )
    .unwrap();
    assert_eq!(
        install_size(&missing(&paths), std::slice::from_ref(&source)),
        Some(0)
    );
    assert_eq!(log.lock().unwrap().len(), requests);
}

#[test]
fn tar_xz_yields_ffmpeg_and_ffprobe_as_executables() {
    let archive = tar_xz_of(&BTBN_FILES);
    let base = serve(vec![
        ("/dl/ffmpeg.tar.xz", Reply::Ok(archive.clone())),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&sha256_hex(&archive), "ffmpeg.tar.xz")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.tar.xz"),
        Format::TarXz,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg", "ffprobe"]);
    assert!(matches!(
        state_of(Tool::Ffmpeg, tools_dir.path()),
        ToolState::Managed { .. }
    ));
    assert_eq!(
        fs::read_to_string(tools_dir.path().join("ffprobe")).unwrap(),
        FFPROBE
    );
}

#[test]
fn pinned_checksum_is_verified_without_fetching_any_checksum_file() {
    let archive = zip_of(&[(
        "aria2-1.37.0/aria2c",
        "#!/bin/sh\necho 'aria2 version 1.37.0'\n",
    )]);
    let (base, log) = serve_with_log(vec![("/dl/aria2.zip", Reply::Ok(archive.clone()))]);
    let source = |hash: String| ToolSource {
        url: format!("{base}/dl/aria2.zip"),
        format: Format::Zip,
        checksum: Checksum::Pinned(hash),
        tools: vec![ToolFile {
            tool: Tool::Aria2c,
            inner_path: Some("aria2c".into()),
        }],
    };
    let tools_dir = tempfile::tempdir().unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source("f".repeat(64))],
        |_| {},
    )
    .unwrap_err();
    assert!(
        matches!(err, ToolChangeError::ChecksumMismatch { .. }),
        "{err}"
    );
    assert!(dir_entries(tools_dir.path()).is_empty());
    // A pinned checksum cannot change between two requests, so there is no second try.
    assert_eq!(log.lock().unwrap().len(), 1);

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source(sha256_hex(&archive))],
        |_| {},
    )
    .unwrap();
    assert_eq!(dir_entries(tools_dir.path()), ["aria2c"]);
    assert!(matches!(
        state_of(Tool::Aria2c, tools_dir.path()),
        ToolState::Managed { .. }
    ));
    assert!(log.lock().unwrap().iter().all(|r| r.ends_with("aria2.zip")));
}

#[test]
fn checksum_is_read_next_to_the_url_the_redirect_resolved_to() {
    // martin-riedl.de: the stable URL 307s to a dated one, and only that one has a .sha256.
    let ffmpeg = zip_of(&[("ffmpeg", FFMPEG)]);
    let ffprobe = zip_of(&[("ffprobe", FFPROBE)]);
    let base = serve(vec![
        (
            "/redirect/ffmpeg.zip",
            Reply::Redirect("/download/1_9.0/ffmpeg.zip"),
        ),
        ("/download/1_9.0/ffmpeg.zip", Reply::Ok(ffmpeg.clone())),
        (
            "/download/1_9.0/ffmpeg.zip.sha256",
            Reply::Ok(format!("{}  ffmpeg.zip\n", sha256_hex(&ffmpeg)).into_bytes()),
        ),
        (
            "/redirect/ffprobe.zip",
            Reply::Redirect("/download/1_9.0/ffprobe.zip"),
        ),
        ("/download/1_9.0/ffprobe.zip", Reply::Ok(ffprobe.clone())),
        (
            "/download/1_9.0/ffprobe.zip.sha256",
            Reply::Ok(format!("{}  ffprobe.zip\n", sha256_hex(&ffprobe)).into_bytes()),
        ),
    ]);
    let root_file = |name: &str, tool| ToolSource {
        url: format!("{base}/redirect/{name}.zip"),
        format: Format::Zip,
        checksum: Checksum::ResolvedSidecar,
        tools: vec![ToolFile {
            tool,
            inner_path: Some(name.into()),
        }],
    };
    let tools_dir = tempfile::tempdir().unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[
            root_file("ffmpeg", Tool::Ffmpeg),
            root_file("ffprobe", Tool::Ffprobe),
        ],
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg", "ffprobe"]);
}

#[test]
fn archive_checksum_mismatch_fails_and_leaves_the_tools_folder_unchanged() {
    let archive = zip_of(&BTBN_FILES);
    let base = serve(vec![
        ("/dl/ffmpeg.zip", Reply::Ok(archive)),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&"f".repeat(64), "ffmpeg.zip")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.zip"),
        Format::Zip,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();
    fs::write(tools_dir.path().join("existing"), "keep me").unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
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
fn a_checksum_that_changed_between_two_requests_is_fetched_again() {
    let archive = zip_of(&BTBN_FILES);
    let right = sha256_hex(&archive);
    let (base, log) = serve_with_log(vec![
        ("/dl/ffmpeg.zip", Reply::Ok(archive)),
        (
            "/dl/checksums.sha256",
            Reply::FirstThen(
                sums(&"f".repeat(64), "ffmpeg.zip"),
                sums(&right, "ffmpeg.zip"),
            ),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.zip"),
        Format::Zip,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg", "ffprobe"]);
    let downloads = log
        .lock()
        .unwrap()
        .iter()
        .filter(|r| *r == "GET /dl/ffmpeg.zip")
        .count();
    assert_eq!(downloads, 2);
}

#[test]
fn truncated_download_fails_and_leaves_the_tools_folder_unchanged() {
    let archive = tar_xz_of(&BTBN_FILES);
    let base = serve(vec![
        ("/dl/ffmpeg.tar.xz", Reply::Truncated(archive.clone())),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&sha256_hex(&archive), "ffmpeg.tar.xz")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.tar.xz"),
        Format::TarXz,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();
    fs::write(tools_dir.path().join("existing"), "keep me").unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, ToolChangeError::Download(_)), "{err}");
    assert_eq!(dir_entries(tools_dir.path()), ["existing"]);
}

#[test]
fn archive_without_the_expected_binary_fails_and_leaves_the_tools_folder_unchanged() {
    let archive = zip_of(&[("ffmpeg-n9.0/bin/ffmpeg", FFMPEG)]); // no ffprobe
    let base = serve(vec![
        ("/dl/ffmpeg.zip", Reply::Ok(archive.clone())),
        (
            "/dl/checksums.sha256",
            Reply::Ok(sums(&sha256_hex(&archive), "ffmpeg.zip")),
        ),
    ]);
    let source = ffmpeg_and_ffprobe(
        format!("{base}/dl/ffmpeg.zip"),
        Format::Zip,
        Checksum::SumsFile {
            url: format!("{base}/dl/checksums.sha256"),
        },
    );
    let tools_dir = tempfile::tempdir().unwrap();

    let err = install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source],
        |_| {},
    )
    .unwrap_err();

    assert!(matches!(err, ToolChangeError::Failed(_)), "{err}");
    assert!(dir_entries(tools_dir.path()).is_empty());
}

#[test]
fn a_failing_later_source_installs_nothing_from_the_earlier_ones() {
    let good = zip_of(&[("ffmpeg", FFMPEG)]);
    let base = serve(vec![("/dl/good.zip", Reply::Ok(good.clone()))]); // /dl/bad.zip is a 404
    let source = |name: &str, tool| ToolSource {
        url: format!("{base}/dl/{name}.zip"),
        format: Format::Zip,
        checksum: Checksum::Pinned(sha256_hex(&good)),
        tools: vec![ToolFile {
            tool,
            inner_path: Some("ffmpeg".into()),
        }],
    };
    let tools_dir = tempfile::tempdir().unwrap();
    // A broken ffmpeg counts as missing, so the good source is downloaded for it.
    let old = fake_tool(tools_dir.path(), "ffmpeg", "exit 1");
    let old_content = fs::read_to_string(&old).unwrap();

    install_tools(
        &tool_paths(tools_dir.path()),
        &Running::default(),
        &[source("good", Tool::Ffmpeg), source("bad", Tool::Ffprobe)],
        |_| {},
    )
    .unwrap_err();

    assert_eq!(dir_entries(tools_dir.path()), ["ffmpeg"]);
    assert_eq!(fs::read_to_string(&old).unwrap(), old_content);
}

#[test]
fn install_size_adds_up_content_lengths_and_falls_back_to_get_when_head_is_refused() {
    let base = serve(vec![
        ("/a", Reply::Ok(vec![0; 1000])),
        ("/b", Reply::NoHead(vec![0; 234])),
    ]);
    let source = |path: &str| ToolSource {
        url: format!("{base}{path}"),
        format: Format::Raw,
        checksum: Checksum::Pinned(String::new()),
        tools: vec![ToolFile {
            tool: Tool::YtDlp,
            inner_path: None,
        }],
    };

    assert_eq!(
        install_size(&[Tool::YtDlp], &[source("/a"), source("/b")]),
        Some(1234)
    );
    assert_eq!(
        install_size(&[Tool::YtDlp], &[source("/a"), source("/missing")]),
        None
    );
}

#[test]
fn check_tools_reports_every_tool_as_required_or_optional() {
    let tools_dir = tempfile::tempdir().unwrap();
    fake_tool(tools_dir.path(), "aria2c", "echo 'aria2 version 1.37.0'");

    let status = check_tools(&tool_paths(tools_dir.path()), &CancelHandle::default()).tools;

    let summary: Vec<_> = status
        .iter()
        .map(|s| (s.tool, s.required, matches!(s.state, ToolState::Missing)))
        .collect();
    assert_eq!(
        summary,
        [
            (Tool::YtDlp, true, true),
            (Tool::Ffmpeg, true, true),
            (Tool::Ffprobe, true, true),
            (Tool::Aria2c, false, false),
        ]
    );
}

#[test]
fn default_sources_cover_every_required_tool_on_this_platform() {
    let provided: Vec<Tool> = default_sources()
        .iter()
        .flat_map(|s| s.tools.iter().map(|f| f.tool))
        .collect();

    for tool in [Tool::YtDlp, Tool::Ffmpeg, Tool::Ffprobe] {
        assert!(provided.contains(&tool), "{tool:?}");
    }

    // Each Tool must be built for this machine's CPU: an arm64 ffmpeg never runs on an Intel Mac.
    let urls: Vec<String> = default_sources().into_iter().map(|s| s.url).collect();
    let (wrong, right) = match (std::env::consts::OS, std::env::consts::ARCH) {
        ("macos", "aarch64") => ("/amd64/", "/arm64/"),
        ("macos", _) => ("/arm64/", "/amd64/"),
        ("linux", "aarch64") => ("linux64", "linuxarm64"),
        ("linux", _) => ("arm64", "linux64"),
        _ => return,
    };
    assert!(urls.iter().all(|u| !u.contains(wrong)), "{urls:?}");
    assert!(urls.iter().any(|u| u.contains(right)), "{urls:?}");
}
