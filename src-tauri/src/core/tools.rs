use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use super::process::{run_captured, CancelHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Tool {
    YtDlp,
    Ffmpeg,
    Ffprobe,
    Aria2c,
}

impl Tool {
    pub const ALL: [Tool; 4] = [Tool::YtDlp, Tool::Ffmpeg, Tool::Ffprobe, Tool::Aria2c];

    /// File name of the Tool's executable on this platform.
    pub fn file_name(self) -> String {
        let name = match self {
            Tool::YtDlp => "yt-dlp",
            Tool::Ffmpeg => "ffmpeg",
            Tool::Ffprobe => "ffprobe",
            Tool::Aria2c => "aria2c",
        };
        format!("{name}{}", std::env::consts::EXE_SUFFIX)
    }

    /// ffmpeg and ffprobe reject `--version` (they only know `-version`).
    fn version_arg(self) -> &'static str {
        match self {
            Tool::Ffmpeg | Tool::Ffprobe => "-version",
            Tool::YtDlp | Tool::Aria2c => "--version",
        }
    }

    /// Required Tools are the ones without which nothing can be downloaded (GLOSSARY).
    pub fn required(self) -> bool {
        self != Tool::Aria2c
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ToolState {
    Managed { path: PathBuf, version: String },
    System { path: PathBuf },
    Missing,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToolStatus {
    pub tool: Tool,
    pub required: bool,
    pub state: ToolState,
}

/// Where to look for Tools. Anything left `None` falls back to the real environment.
#[derive(Debug, Clone, Default)]
pub struct ToolPaths {
    /// Value to search instead of the process `PATH`.
    pub search_path: Option<OsString>,
    /// Folder holding Managed Tools; they are preferred over System Tools.
    pub tools_dir: Option<PathBuf>,
    /// Searched after the `PATH`, for System Tools that a GUI app's minimal `PATH` misses.
    pub extra_dirs: Vec<PathBuf>,
}

/// Where package managers put System Tools that a GUI app launched from the macOS
/// Dock does not see on its `PATH`. Empty on Windows.
pub fn default_extra_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    if cfg!(windows) {
        return Vec::new();
    }
    ["/opt/homebrew/bin", "/usr/local/bin", "/opt/local/bin"]
        .map(PathBuf::from)
        .into_iter()
        .chain(home.map(|h| h.join(".local/bin")))
        .collect()
}

/// What `check_tools` tells the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolReport {
    pub tools: Vec<ToolStatus>,
    /// Every Required Tool is Managed or System, so downloads can start.
    pub required_present: bool,
}

/// Once `cancel` is set (the app quits), every version command still running is stopped.
pub fn check_tools(paths: &ToolPaths, cancel: &CancelHandle) -> ToolReport {
    // In parallel: right after an install the OS scans each new file on its first run, so one
    // after another the check would take all those scans added up.
    let tools: Vec<ToolStatus> = std::thread::scope(|scope| {
        Tool::ALL
            .map(|tool| scope.spawn(move || tool_state(tool, paths, cancel)))
            .into_iter()
            .zip(Tool::ALL)
            .map(|(state, tool)| ToolStatus {
                tool,
                required: tool.required(),
                state: state.join().unwrap_or(ToolState::Missing),
            })
            .collect()
    });
    let required_present = tools
        .iter()
        .all(|t| !t.required || t.state != ToolState::Missing);
    ToolReport {
        tools,
        required_present,
    }
}

/// Where a Tool can be run from, or `None` if it is missing. The single place that turns a
/// Tool's state into a path, so everything that runs a Tool (Managed or System) resolves it
/// the same way.
///
/// A Tool is taken as soon as its file is there, without running its version command: the
/// yt-dlp build can take a second to start, which every lookup and download would otherwise
/// wait for. A Tool that cannot run then fails with its own error. `check_tools` still runs
/// the version command, so the Tools screen shows such a Tool as Missing.
pub(super) fn tool_path(paths: &ToolPaths, tool: Tool) -> Option<PathBuf> {
    paths
        .tools_dir
        .as_ref()
        .map(|dir| dir.join(tool.file_name()))
        .filter(|p| p.is_file())
        .or_else(|| find_on_system(&tool.file_name(), paths))
}

/// The state of the file [`tool_path`] runs. A Managed file that does not run is Missing, not
/// hidden behind a System Tool, since it is what runs: Install Tools then replaces it.
pub(super) fn tool_state(tool: Tool, paths: &ToolPaths, cancel: &CancelHandle) -> ToolState {
    let Some(path) = tool_path(paths, tool) else {
        return ToolState::Missing;
    };
    let managed = paths
        .tools_dir
        .as_ref()
        .is_some_and(|dir| path == dir.join(tool.file_name()));
    match version(tool, &path, cancel) {
        None => ToolState::Missing,
        Some(version) if managed => ToolState::Managed { path, version },
        Some(_) => ToolState::System { path },
    }
}

fn find_on_system(file_name: &str, paths: &ToolPaths) -> Option<PathBuf> {
    let search_path = paths
        .search_path
        .clone()
        .or_else(|| std::env::var_os("PATH"))
        .unwrap_or_default();

    std::env::split_paths(&search_path)
        .chain(paths.extra_dirs.iter().cloned())
        .map(|dir| dir.join(file_name))
        .find(|candidate| candidate.is_file())
}

/// How long a version command may take. Generous, because a first start after install can be
/// slow while the OS scans the file; past it the Tool is stopped and counts as Missing.
const VERSION_WAIT: Duration = Duration::from_secs(15);

/// A Tool counts as present only if its version flag succeeds, as in the old Node core.
/// ffmpeg prints a whole banner and aria2c a sentence, so the version is the word after
/// "version", without a build suffix: `ffmpeg version 9.0.2-https://... Copyright` -> `9.0.2`.
fn version(tool: Tool, path: &Path, cancel: &CancelHandle) -> Option<String> {
    let mut cmd = Command::new(path);
    cmd.arg(tool.version_arg());
    // A Tool that hangs (say, on a network drive that is gone) would otherwise hold up the check,
    // and with it every Tool Change, until the app quits. The deadline, or quitting, stops it and
    // its children.
    let deadline = cancel.with_deadline(VERSION_WAIT);
    let output = run_captured(cmd, &deadline)
        .ok()
        .filter(|o| o.status.success() && !deadline.timed_out())?;
    let line = output.stdout.lines().next().unwrap_or_default().trim();
    let version = match line.split_once("version ") {
        Some((_, rest)) => rest.split([' ', '-']).next().unwrap_or(rest),
        None => line,
    };
    Some(version.to_string())
}
