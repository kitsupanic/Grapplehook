//! Grapplehook core (ADR-0001). This module's public interface is the test seam:
//! tests drive it through `check_tools`, `get_info`, `download` and friends, never through
//! private helpers.

mod download;
mod info;
mod managed;
mod process;
mod progress;
mod running;
mod tools;
mod update;

pub use download::{download, DownloadError, DownloadOptions, DownloadTask, FailureKind};
pub use info::{get_info, VideoInfo, TOO_SLOW};
pub use managed::{
    default_sources, delete_tools, install_size, install_tools, update_yt_dlp, Checksum, Format,
    InstallProgress, ToolChangeError, ToolFile, ToolSource, YtDlpUpdate,
};
pub use process::CancelHandle;
pub use progress::{Progress, Stage};
pub use running::{Busy, Running};
pub use tools::{
    check_tools, default_extra_dirs, Tool, ToolPaths, ToolReport, ToolState, ToolStatus,
};
pub use update::{check_for_update, UpdateInfo, RELEASES_API};

use tools::tool_path;

/// `url` trimmed, if it is an http(s) URL. Anything else must never reach a Tool.
fn http_url(url: &str) -> Result<&str, String> {
    let url = url.trim();
    let lower = url.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") {
        Ok(url)
    } else {
        Err("Enter a valid http(s) URL.".into())
    }
}

/// yt-dlp's `ERROR: [extractor] id: sentence` line as its kind and the part a person needs.
/// The phrases are yt-dlp's own wording: one it rewords is `Other`. `None` if the line says
/// nothing, so that the caller reports how yt-dlp ended instead of an empty message.
fn yt_dlp_error(line: &str) -> Option<(FailureKind, String)> {
    const GONE: [&str; 5] = [
        "video unavailable",
        "video is unavailable",
        "video is private",
        "private video",
        "removed by",
    ];
    let lower = line.to_lowercase();
    let kind = if lower.contains("requested format is not available") {
        FailureKind::FormatUnavailable
    } else if GONE.iter().any(|phrase| lower.contains(phrase)) {
        FailureKind::VideoUnavailable
    } else {
        FailureKind::Other
    };
    let sentence = line.strip_prefix("ERROR:").unwrap_or(line).trim_start();
    let sentence = match sentence.strip_prefix('[').and_then(|s| s.split_once(']')) {
        Some((_, rest)) => rest.trim_start(),
        None => sentence,
    };
    (!sentence.is_empty()).then(|| (kind, sentence.to_string()))
}
