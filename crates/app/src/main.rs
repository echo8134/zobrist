//! The zobrist desktop app.

use zobrist_app::ipc::{self, Request, Response};

#[tauri::command]
fn answer(request: Request) -> Response {
    ipc::answer(request)
}

fn main() -> tauri::Result<()> {
    let builder = tauri::Builder::default().invoke_handler(tauri::generate_handler![answer]);
    #[cfg(feature = "webdriver")]
    let builder = builder.plugin(tauri_plugin_wdio_webdriver::init());
    builder.run(tauri::generate_context!())
}
