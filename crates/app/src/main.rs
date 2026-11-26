//! The zobrist desktop app.

use tauri::{State, async_runtime::Mutex};
use zobrist_app::ipc::{Games, Request, Response};

#[tauri::command]
async fn answer(request: Request, games: State<'_, Mutex<Games>>) -> Result<Response, ()> {
    Ok(games.lock().await.answer(request))
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .manage(Mutex::new(Games::default()))
        .invoke_handler(tauri::generate_handler![answer]);
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
