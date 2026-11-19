use std::cmp::Ordering;
use std::time::Duration;

use serde::{Deserialize, Serialize};

/// GitHub API endpoint for the latest (non-draft, non-pre-release) Grapplehook release.
pub const RELEASES_API: &str =
    "https://api.github.com/repos/kitsupanic/Grapplehook/releases/latest";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub update_available: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
}

#[derive(Deserialize)]
struct Release {
    tag_name: String,
}

/// Compare dotted numeric versions: `1.2.10` > `1.2.9`, `1.2` == `1.2.0`.
/// A pre-release or build tag (`-rc.1`, `+5`) is cut off before comparing, so
/// `1.2.3-rc.1` equals `1.2.3`. Segments that are not numbers count as 0.
pub(super) fn compare_versions(a: &str, b: &str) -> Ordering {
    let (a, b) = (segments(a), segments(b));
    (0..a.len().max(b.len()))
        .map(|i| {
            let get = |v: &[u64]| v.get(i).copied().unwrap_or(0);
            get(&a).cmp(&get(&b))
        })
        .find(|o| o.is_ne())
        .unwrap_or(Ordering::Equal)
}

fn segments(version: &str) -> Vec<u64> {
    let core = version.split(['-', '+']).next().unwrap_or("");
    core.split('.').map(|s| s.parse().unwrap_or(0)).collect()
}

/// Ask `api_url` for the latest release and compare it with `current_version`.
/// Any failure (offline, rate-limited, bad body) means "no update".
pub fn check_for_update(current_version: &str, api_url: &str) -> UpdateInfo {
    let latest_version = fetch_latest_tag(current_version, api_url)
        .map(|tag| tag.strip_prefix('v').unwrap_or(&tag).to_string());
    UpdateInfo {
        update_available: latest_version
            .as_deref()
            .is_some_and(|latest| compare_versions(latest, current_version).is_gt()),
        current_version: current_version.to_string(),
        latest_version,
    }
}

fn fetch_latest_tag(current_version: &str, api_url: &str) -> Option<String> {
    let release: Release = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(10))
        .user_agent(format!("grapplehook/{current_version}"))
        .build()
        .ok()?
        .get(api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .ok()?
        .error_for_status()
        .ok()?
        .json()
        .ok()?;
    Some(release.tag_name)
}
