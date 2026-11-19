use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::process::{describe_failure, run_captured, CancelHandle, CANCELLED};
use super::{http_url, tool_path, yt_dlp_error, Tool, ToolPaths};

/// How long a lookup may take before yt-dlp is stopped. yt-dlp gives up on a silent host by
/// itself after 20 seconds; this catches one that keeps sending a little.
const LOOKUP_WAIT: Duration = Duration::from_secs(90);

/// The error of a lookup stopped at [`LOOKUP_WAIT`].
pub const TOO_SLOW: &str = "Looking up the video took too long. Check the link and try again.";

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct VideoInfo {
    pub id: String,
    pub title: String,
    pub uploader: Option<String>,
    pub duration_seconds: Option<f64>,
    pub thumbnail: Option<String>,
    /// Distinct heights of the formats that have video, highest first.
    pub heights: Vec<u32>,
}

// What yt-dlp's `-J` prints; every field is optional, as in the old Node core.
#[derive(Deserialize)]
struct RawInfo {
    /// `playlist` for a playlist or channel link.
    #[serde(rename = "_type")]
    kind: Option<String>,
    id: Option<String>,
    title: Option<String>,
    uploader: Option<String>,
    channel: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
    #[serde(default)]
    formats: Vec<RawFormat>,
}

#[derive(Deserialize)]
struct RawFormat {
    height: Option<u32>,
    vcodec: Option<String>,
}

/// Ask yt-dlp about `url`. Once `cancel` is set, yt-dlp is stopped and the result is
/// `Err("Cancelled")`. A playlist or channel link is refused: `--no-playlist` does not stop
/// yt-dlp from downloading every video of one, and `--flat-playlist` keeps that refusal fast.
pub fn get_info(paths: &ToolPaths, url: &str, cancel: &CancelHandle) -> Result<VideoInfo, String> {
    let url = http_url(url)?;
    // A slow host would otherwise keep the lookup, and with it every Tool Change, going until
    // the app quits: nothing else can stop a lookup.
    let cancel = &cancel.with_deadline(LOOKUP_WAIT);
    let yt_dlp =
        tool_path(paths, Tool::YtDlp).ok_or("yt-dlp was not found. Install it and try again.")?;
    if cancel.is_cancelled() {
        return Err(CANCELLED.into());
    }

    let mut cmd = Command::new(&yt_dlp);
    cmd.args([
        "--ignore-config",
        "--no-playlist",
        "--flat-playlist",
        "-J",
        "--",
        url,
    ]);
    let output = run_captured(cmd, cancel).map_err(|e| format!("Could not run yt-dlp: {e}"))?;
    if cancel.timed_out() {
        return Err(TOO_SLOW.into());
    }
    if cancel.is_cancelled() {
        return Err(CANCELLED.into());
    }

    if !output.status.success() {
        // yt-dlp's own sentence if it gave one, without the warnings around it.
        let last_error = output
            .stderr
            .lines()
            .rev()
            .find(|l| l.starts_with("ERROR:"));
        if let Some((_, sentence)) = last_error.and_then(yt_dlp_error) {
            return Err(sentence);
        }
        let detail: String = output.stderr.trim().chars().take(400).collect();
        return Err(format!(
            "{}: {detail}",
            describe_failure("yt-dlp", output.status)
        ));
    }

    let raw: RawInfo = serde_json::from_str(&output.stdout)
        .map_err(|e| format!("Could not read yt-dlp output: {e}"))?;
    if raw.kind.as_deref() == Some("playlist") {
        return Err(
            "This link is a playlist or a channel. Open one video and paste its link.".into(),
        );
    }
    Ok(raw.into())
}

impl From<RawInfo> for VideoInfo {
    fn from(raw: RawInfo) -> Self {
        let mut heights: Vec<u32> = raw
            .formats
            .into_iter()
            // yt-dlp leaves vcodec out when it does not know it (HLS without CODECS, Vimeo's
            // plain MP4s); only "none" means no video (audio only, storyboards).
            .filter(|f| f.vcodec.as_deref() != Some("none"))
            .filter_map(|f| f.height)
            .collect();
        heights.sort_unstable_by(|a, b| b.cmp(a));
        heights.dedup();

        VideoInfo {
            id: raw.id.unwrap_or_default(),
            title: raw.title.unwrap_or_default(),
            uploader: raw.uploader.or(raw.channel),
            duration_seconds: raw.duration,
            thumbnail: raw.thumbnail,
            heights,
        }
    }
}
