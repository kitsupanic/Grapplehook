mod commands;
pub mod core;

use tauri::Manager;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // One app at a time: `Running` only guards the Tools within one process, so a second
        // copy could delete or replace them mid-download. A second launch shows the open window.
        // Registered first, as the plugin asks.
        // Its `deep-link` feature hands a link from the second launch to the deep-link plugin.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            show_window(app)
        }))
        // The window reads each link itself (src/lib/sentLink.ts); here it only comes to the front.
        // So a link the window then rejects still raises it: accepted, as Rust doesn't parse links.
        .plugin(tauri_plugin_deep_link::init())
        .setup(|app| {
            let handle = app.handle().clone();
            app.deep_link().on_open_url(move |_| show_window(&handle));
            Ok(())
        })
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .manage(core::Running::default())
        // Quitting cancels every download and Tool install and throws away what they fetched, so
        // closing the window during one asks first.
        // shortcut: Cmd+Q on macOS quits without this question, as tao gives no way to stop it;
        // replace the menu's Quit item with a custom one if that matters.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                if window.state::<core::Running>().quitting_loses_work() {
                    api.prevent_close();
                    ask_to_quit(window.clone());
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::cancel_download,
            commands::check_tools,
            commands::check_update,
            commands::choose_dir,
            commands::delete_tools,
            commands::get_info,
            commands::install_size,
            commands::install_tools,
            commands::read_clipboard,
            commands::start_download,
            commands::update_yt_dlp
        ])
        .build(tauri::generate_context!())
        .expect("error while building Grapplehook")
        .run(|app, event| {
            // Quitting must not leave yt-dlp, aria2c or ffmpeg running in the background.
            if let tauri::RunEvent::Exit = event {
                app.state::<core::Running>().cancel_all_and_wait();
            }
        });
}

fn show_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// Off the main thread, which a blocking dialog would freeze. Closes the window on "Quit".
fn ask_to_quit(window: tauri::Window) {
    std::thread::spawn(move || {
        let quit = window
            .dialog()
            .message(
                "Quitting stops the downloads and Tool installs in progress, and they will start over next time.",
            )
            .title("Still working")
            .kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom(
                "Quit".into(),
                "Keep going".into(),
            ))
            .parent(&window)
            .blocking_show();
        if quit {
            let _ = window.destroy();
        }
    });
}
