//! The zobrist desktop app.

use tauri::State;
use tauri::async_runtime::{Mutex, spawn_blocking};
use zobrist_app::ipc::{FileRequest, Games, Request, Response};

#[tauri::command]
async fn answer(request: Request, games: State<'_, Mutex<Games>>) -> Result<Response, ()> {
    match FileRequest::try_from(request) {
        // A file can take long to read, and it needs none of the open games. So the lock on the
        // games stays free for the boards, and the read runs on a thread that may wait.
        Ok(file) => spawn_blocking(move || file.answer()).await.map_err(|_| ()),
        Err(request) => Ok(games.lock().await.answer(request)),
    }
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default()
        .manage(Mutex::new(Games::default()))
        .invoke_handler(tauri::generate_handler![answer]);
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
