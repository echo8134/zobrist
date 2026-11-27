//! The zobrist desktop app.

use tauri::{Manager as _, State, Window, WindowEvent, async_runtime::spawn_blocking};
use tauri_plugin_dialog::DialogExt as _;
use zobrist_app::ipc::{DesktopRequest, Response, SharedGames};

#[tauri::command]
async fn answer(
    request: DesktopRequest,
    games: State<'_, SharedGames>,
    window: Window,
) -> Result<Response, String> {
    match request {
        DesktopRequest::Editor(request) => games
            .answer(request)
            .await
            .map_err(|error| error.to_string()),
        DesktopRequest::Collection(request) => spawn_blocking(move || request.answer(&window))
            .await
            .map_err(|error| error.to_string())?
            .map_err(|error| error.to_string()),
        DesktopRequest::Session(request) => {
            let directory = window
                .path()
                .app_local_data_dir()
                .map_err(|error| error.to_string())?;
            let response = games
                .answer_session(request, directory)
                .await
                .map_err(|error| error.to_string())?;
            if matches!(response, Response::QuitWithBoards) {
                window.destroy().map_err(|error| error.to_string())?;
            }
            Ok(response)
        }
    }
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(SharedGames::default())
        .on_window_event(|window, event| {
            // Closing before startup finishes must leave the previous boards on disk,
            // including when Rust refused to read them.
            if let WindowEvent::CloseRequested { api, .. } = event
                && window.state::<SharedGames>().is_restored()
                && let Some(board) = window.get_webview_window(window.label())
            {
                api.prevent_close();
                if let Err(error) =
                    board.eval("window.dispatchEvent(new Event('zobrist-close-requested'))")
                {
                    window
                        .dialog()
                        .message(format!("Could not close the boards: {error}"))
                        .show(|_| {});
                }
            }
        })
        .invoke_handler(tauri::generate_handler![answer]);
    // The macOS Quit item normally terminates the process without asking its window to close.
    #[cfg(target_os = "macos")]
    let builder = builder
        .menu(|app| {
            use tauri::menu::{Menu, MenuItem, MenuItemKind, PredefinedMenuItem};

            let menu = Menu::default(app)?;
            let quit_text = PredefinedMenuItem::quit(app, None)?.text()?;
            if let Some(MenuItemKind::Submenu(application)) = menu.items()?.first() {
                for (position, item) in application.items()?.into_iter().enumerate() {
                    if let MenuItemKind::Predefined(original) = item
                        && original.text()? == quit_text
                    {
                        let quit = MenuItem::with_id(
                            app,
                            "quit-with-boards",
                            quit_text,
                            true,
                            Some("Cmd+Q"),
                        )?;
                        application.remove(&original)?;
                        application.insert(&quit, position)?;
                        break;
                    }
                }
            }
            Ok(menu)
        })
        .on_menu_event(|app, event| {
            if event.id().as_ref() == "quit-with-boards"
                && let Some(window) = app.get_webview_window("main")
                && let Err(error) = window.close()
            {
                window
                    .dialog()
                    .message(format!("Could not close the window: {error}"))
                    .show(|_| {});
            }
        });
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
