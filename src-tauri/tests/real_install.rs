//! Hits the real Tool releases (GitHub, martin-riedl.de). Run by hand:
//! `GRAPPLEHOOK_TOOLS_DIR=/some/empty/dir cargo test --test real_install -- --ignored --nocapture`
//! Used for the Gatekeeper check in ADR-0002.

use std::path::PathBuf;

use grapplehook_lib::core::CancelHandle;
use grapplehook_lib::core::{
    check_tools, default_sources, install_size, install_tools, Running, Tool, ToolPaths, ToolState,
};

#[test]
#[ignore = "downloads 90-220 MB from GitHub and martin-riedl.de"]
fn real_tools_install_and_run() {
    let tools_dir = PathBuf::from(std::env::var_os("GRAPPLEHOOK_TOOLS_DIR").expect("set a dir"));
    // No System Tools, so everything is downloaded.
    let paths = ToolPaths {
        tools_dir: Some(tools_dir.clone()),
        search_path: Some("".into()),
        ..Default::default()
    };
    println!(
        "total size: {:?}",
        install_size(
            &check_tools(&paths, &CancelHandle::default())
                .tools
                .into_iter()
                .filter(|t| t.state == ToolState::Missing)
                .map(|t| t.tool)
                .collect::<Vec<_>>(),
            &default_sources()
        )
    );

    install_tools(&paths, &Running::default(), &default_sources(), |_| {}).unwrap();

    let status = check_tools(&paths, &CancelHandle::default()).tools;
    println!("installed in {}:", tools_dir.display());
    for s in status {
        println!("  {:?} required={} {:?}", s.tool, s.required, s.state);
    }
}

/// The source URLs float ("latest") or name an ffmpeg branch that FFmpeg-Builds stops building
/// one day; this fails once one of them no longer answers. Run weekly by `tool-links.yml`.
#[test]
#[ignore = "needs the network"]
fn every_tool_source_still_answers() {
    assert!(
        install_size(&Tool::ALL, &default_sources()).is_some(),
        "a Tool source URL did not answer with a size; see default_sources()"
    );
}
