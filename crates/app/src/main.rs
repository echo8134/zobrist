//! The zobrist desktop app.

use zobrist_app::ipc::{self, Request, Response};

#[tauri::command]
fn answer(request: Request) -> Response {
    ipc::answer(request)
}

fn main() -> tauri::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![answer])
        .run(tauri::generate_context!())
}
