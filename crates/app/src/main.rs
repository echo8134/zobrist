//! The zobrist desktop app.

use tauri::{Manager as _, State, Window, async_runtime::spawn_blocking};
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
    }
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .setup(|app| {
            let directory = app.path().app_local_data_dir().map_err(|e| e.to_string());
            app.manage(SharedGames::with_history(directory));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![answer]);
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
