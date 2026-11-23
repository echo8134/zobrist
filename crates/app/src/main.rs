//! The zobrist desktop app.

fn main() -> tauri::Result<()> {
    tauri::Builder::default().run(tauri::generate_context!())
}
