//! The zobrist desktop app.

use tauri::State;
use zobrist_app::ipc::{Request, Response, SharedGames};

#[tauri::command]
async fn answer(request: Request, games: State<'_, SharedGames>) -> Result<Response, ()> {
    games.answer(request).await.map_err(|_| ())
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .manage(SharedGames::default())
        .invoke_handler(tauri::generate_handler![answer]);
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
