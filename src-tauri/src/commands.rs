//! Thin Tauri layer over `core`: no logic here beyond reading the environment, turning
//! core's callbacks into events keyed by task id, and moving blocking work off the main thread.

use std::path::PathBuf;

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_dialog::DialogExt;

use crate::core::{
    self, DownloadError, DownloadOptions, FailureKind, InstallProgress, Progress, Running, Tool,
    ToolPaths, ToolReport, UpdateInfo, VideoInfo, YtDlpUpdate,
};

/// Managed Tools live in the app-data folder.
fn tools_dir(app: &AppHandle) -> Result<PathBuf, String> {
    let dir = app.path().app_data_dir().map_err(|e| e.to_string())?;
    Ok(dir.join("tools"))
}

/// Where the app looks for Tools.
fn tool_paths(app: &AppHandle) -> ToolPaths {
    ToolPaths {
        tools_dir: tools_dir(app).ok(),
        extra_dirs: core::default_extra_dirs(app.path().home_dir().ok().as_deref()),
        ..Default::default()
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ProgressEvent {
    task_id: u64,
    #[serde(flatten)]
    progress: Progress,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DoneEvent {
    task_id: u64,
    #[serde(flatten)]
    outcome: Outcome,
}

/// How a download ended: `{ "outcome": "done", "outputPath": ... }` and so on.
#[derive(Clone, Serialize)]
#[serde(tag = "outcome", rename_all = "camelCase")]
enum Outcome {
    Done {
        #[serde(rename = "outputPath")]
        output_path: PathBuf,
    },
    Cancelled,
    Failed {
        kind: FailureKind,
        error: String,
        /// The file that was still saved, if any (the original, when only converting failed).
        #[serde(rename = "outputPath")]
        output_path: Option<PathBuf>,
    },
}

impl From<Result<PathBuf, DownloadError>> for Outcome {
    fn from(result: Result<PathBuf, DownloadError>) -> Self {
        match result {
            Ok(output_path) => Outcome::Done { output_path },
            Err(DownloadError::Cancelled) => Outcome::Cancelled,
            Err(DownloadError::Failed {
                kind,
                message,
                saved,
            }) => Outcome::Failed {
                kind,
                error: message,
                output_path: saved,
            },
        }
    }
}

/// Runs `job` off the async runtime's threads, so blocking work (Tools, HTTP) never stalls them.
/// A thread that dies is an error like any other.
async fn blocking<T: Send + 'static>(
    job: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(job)
        .await
        .map_err(|e| e.to_string())?
}

/// Async so the yt-dlp run happens off the main thread and the window stays responsive.
#[tauri::command]
pub async fn get_info(
    app: AppHandle,
    running: State<'_, Running>,
    url: String,
) -> Result<VideoInfo, String> {
    let (paths, running) = (tool_paths(&app), running.inner().clone());
    blocking(move || {
        running
            .run(|cancel| core::get_info(&paths, &url, cancel))
            .map_err(|e| e.to_string())?
    })
    .await
}

/// Checking runs each Tool's version command, so it happens off the main thread too. A Tool
/// Run, so no Tool Change replaces or deletes a Tool while it runs (Windows refuses that), and
/// quitting stops it.
#[tauri::command]
pub async fn check_tools(
    app: AppHandle,
    running: State<'_, Running>,
) -> Result<ToolReport, String> {
    let (paths, running) = (tool_paths(&app), running.inner().clone());
    blocking(move || {
        running
            .run(|cancel| core::check_tools(&paths, cancel))
            .map_err(|e| e.to_string())
    })
    .await
}

/// Total bytes `install_tools` will download for the `missing` Tools of the last
/// `check_tools` report, or `None` if a host does not say.
#[tauri::command]
pub async fn install_size(missing: Vec<Tool>) -> Option<u64> {
    tauri::async_runtime::spawn_blocking(move || {
        core::install_size(&missing, &core::default_sources())
    })
    .await
    .ok()
    .flatten()
}

/// Downloads off the async runtime's threads; progress streams over `on_progress`.
#[tauri::command]
pub async fn install_tools(
    app: AppHandle,
    running: State<'_, Running>,
    on_progress: Channel<InstallProgress>,
) -> Result<(), String> {
    let (paths, running) = (tool_paths(&app), running.inner().clone());
    blocking(move || {
        core::install_tools(&paths, &running, &core::default_sources(), |p| {
            let _ = on_progress.send(p);
        })
        .map_err(|e| e.to_string())
    })
    .await
}

/// Removes the Managed Tools folder; System Tools stay as they are. Off the main thread, as
/// removing a few hundred MB can take seconds while antivirus scans.
#[tauri::command]
pub async fn delete_tools(app: AppHandle, running: State<'_, Running>) -> Result<(), String> {
    let (paths, running) = (tool_paths(&app), running.inner().clone());
    blocking(move || core::delete_tools(&paths, &running).map_err(|e| e.to_string())).await
}

/// Blocking HTTP runs off the async runtime's threads. Core turns every failure into "no
/// update", so this only fails if that thread dies, which the UI treats the same way.
#[tauri::command]
pub async fn check_update(app: AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    blocking(move || Ok(core::check_for_update(&current, core::RELEASES_API))).await
}

/// `yt-dlp -U` downloads, so it runs off the async runtime's threads.
#[tauri::command]
pub async fn update_yt_dlp(
    app: AppHandle,
    running: State<'_, Running>,
) -> Result<YtDlpUpdate, String> {
    let (paths, running) = (tool_paths(&app), running.inner().clone());
    blocking(move || core::update_yt_dlp(&paths, &running).map_err(|e| e.to_string())).await
}

/// Start a download and return its task id. Progress arrives as `download-progress`
/// events and the end as one `download-done` event, both carrying the id.
#[tauri::command]
pub fn start_download(
    app: AppHandle,
    running: State<'_, Running>,
    options: DownloadOptions,
) -> Result<u64, String> {
    let (progress_app, done_app) = (app.clone(), app.clone());
    running.download(
        &tool_paths(&app),
        options,
        move |task_id, progress| {
            let _ = progress_app.emit("download-progress", ProgressEvent { task_id, progress });
        },
        move |task_id, result| {
            let event = DoneEvent {
                task_id,
                outcome: result.into(),
            };
            let _ = done_app.emit("download-done", event);
        },
    )
}

#[tauri::command]
pub fn cancel_download(running: State<'_, Running>, task_id: u64) {
    running.cancel(task_id);
}

/// The native folder picker. `None` when the user closes it without choosing.
#[tauri::command]
pub async fn choose_dir(app: AppHandle) -> Option<PathBuf> {
    tauri::async_runtime::spawn_blocking(move || app.dialog().file().blocking_pick_folder())
        .await
        .ok()
        .flatten()
        .and_then(|folder| folder.into_path().ok())
}

/// The clipboard's text, read natively so the webview needs no clipboard permission.
#[tauri::command]
pub fn read_clipboard() -> Result<String, String> {
    arboard::Clipboard::new()
        .and_then(|mut clipboard| clipboard.get_text())
        .map_err(|err| match err {
            arboard::Error::ContentNotAvailable => {
                "The clipboard has no text to paste.".to_string()
            }
            other => format!("Could not read the clipboard: {other}"),
        })
}

// The UI reads these events by hand-written field names (src/lib/api.ts), so their JSON is
// pinned here: a dropped `rename` would otherwise pass every test and hide "Show in folder".
#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Stage;
    use serde_json::json;

    fn done(result: Result<PathBuf, DownloadError>) -> serde_json::Value {
        serde_json::to_value(DoneEvent {
            task_id: 7,
            outcome: result.into(),
        })
        .unwrap()
    }

    #[test]
    fn download_done_events_have_the_field_names_the_ui_reads() {
        assert_eq!(
            done(Ok("/out/a.mp4".into())),
            json!({ "taskId": 7, "outcome": "done", "outputPath": "/out/a.mp4" })
        );
        assert_eq!(
            done(Err(DownloadError::Cancelled)),
            json!({ "taskId": 7, "outcome": "cancelled" })
        );
        assert_eq!(
            done(Err(DownloadError::Failed {
                kind: FailureKind::ConversionFailed,
                message: "Converting failed".into(),
                saved: Some("/out/a.webm".into()),
            })),
            json!({
                "taskId": 7,
                "outcome": "failed",
                "kind": "conversionFailed",
                "error": "Converting failed",
                "outputPath": "/out/a.webm",
            })
        );
        assert_eq!(
            done(Err("No space left".into()))["outputPath"],
            serde_json::Value::Null
        );
    }

    #[test]
    fn download_progress_events_have_the_field_names_the_ui_reads() {
        let event = ProgressEvent {
            task_id: 7,
            progress: Progress {
                stage: Stage::Download,
                percent: Some(50.0),
                speed: Some(1.0),
                eta: Some(2.0),
                downloaded_bytes: Some(3.0),
                total_bytes: Some(4.0),
            },
        };
        assert_eq!(
            serde_json::to_value(event).unwrap(),
            json!({
                "taskId": 7,
                "stage": "download",
                "percent": 50.0,
                "speed": 1.0,
                "eta": 2.0,
                "downloadedBytes": 3.0,
                "totalBytes": 4.0,
            })
        );
    }
}
