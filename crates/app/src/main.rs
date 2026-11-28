//! The zobrist desktop app.

use tauri::{
    DragDropEvent, Manager as _, RunEvent, State, Window, WindowEvent,
    async_runtime::spawn_blocking,
};
use tauri_plugin_dialog::DialogExt as _;
use zobrist_app::ipc::{DesktopRequest, Response, SharedGames};

#[tauri::command]
async fn answer(
    request: DesktopRequest,
    games: State<'_, SharedGames>,
    window: Window,
) -> Result<Response, String> {
    let data_directory = |window: &Window| {
        window
            .path()
            .app_local_data_dir()
            .map_err(std::io::Error::other)
    };
    let task = match request {
        DesktopRequest::Editor(request) => {
            return games
                .answer(request)
                .await
                .map_err(|error| error.to_string());
        }
        DesktopRequest::Session(request) => {
            let directory = data_directory(&window).map_err(|error| error.to_string())?;
            let response = games
                .answer_session(request, directory)
                .await
                .map_err(|error| error.to_string())?;
            if matches!(response, Response::QuitWithBoards) {
                window.destroy().map_err(|error| error.to_string())?;
            }
            return Ok(response);
        }
        DesktopRequest::Analysis(request) => {
            let directory = data_directory(&window).map_err(|error| error.to_string())?;
            return request
                .answer(&games, directory)
                .await
                .map_err(|error| error.to_string());
        }
        DesktopRequest::Collection(request) => spawn_blocking(move || request.answer(&window)),
        DesktopRequest::Databases(request) => spawn_blocking(move || request.answer(&window)),
        DesktopRequest::Account(request) => {
            spawn_blocking(move || request.answer(&data_directory(&window)?))
        }
        DesktopRequest::Appearance(request) => {
            spawn_blocking(move || request.answer(&data_directory(&window)?))
        }
    };
    task.await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_local_data_dir().map_err(|e| e.to_string());
            app.manage(SharedGames::with_history(directory));
            Ok(())
        })
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
            if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event
                && let Some(view) = window.get_webview_window(window.label())
            {
                let paths: Vec<_> = paths.iter().map(|path| path.to_str()).collect();
                let detail = serde_json::json!({ "paths": paths });
                drop(view.eval(format!(
                    "window.dispatchEvent(new CustomEvent('zobrist-file-drop', {{detail: {detail}}}));"
                )));
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
    builder
        .build(tauri::generate_context!())?
        .run(|app, event| {
            if matches!(event, RunEvent::Exit) {
                app.state::<SharedGames>().stop_engines();
            }
        });
    Ok(())
}
