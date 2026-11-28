//! The zobrist desktop app.

use tauri::{Manager as _, RunEvent, State, Window, async_runtime::spawn_blocking};
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
        .invoke_handler(tauri::generate_handler![answer]);
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
