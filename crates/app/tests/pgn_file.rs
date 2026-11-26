//! The requests of the page Files and their complete answers, as the view sends and receives them.

use std::path::PathBuf;
use std::{env, fs, io, process};

use serde_json::{Value, json};
use tauri::async_runtime::block_on;
use zobrist_app::ipc::{Request, SharedGames};

fn ask(request: Value) -> serde_json::Result<Value> {
    ask_games(&SharedGames::default(), request)
}

fn ask_games(games: &SharedGames, request: Value) -> serde_json::Result<Value> {
    let request: Request = serde_json::from_value(request)?;
    let response = block_on(games.answer(request)).map_err(serde::ser::Error::custom)?;
    serde_json::to_value(response)
}

fn fixture(name: &str) -> String {
    format!(
        "{}/../cli/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn temporary_path(name: &str) -> PathBuf {
    env::temp_dir().join(format!("zobrist-app-files-{name}-{}", process::id()))
}

#[test]
fn opens_the_clean_file_and_lists_its_two_games() -> serde_json::Result<()> {
    let path = fixture("clean.pgn");
    assert_eq!(
        ask(json!({"type": "OpenPgnFile", "path": path}))?,
        json!({"type": "OpenPgnFile", "answer": {
            "type": "File",
            "path": path,
            "rows": [
                {"number": 1, "white": "Alpha", "black": "Beta", "result": "1-0",
                 "date": "2024.01.01", "event": "Example"},
                {"number": 2, "white": "Gamma", "black": "Delta", "result": "0-1",
                 "date": "2024.01.01", "event": "Example"},
            ],
            "problems": 0,
        }})
    );
    Ok(())
}

#[test]
fn opens_the_damaged_file_and_counts_its_game_with_a_problem() -> serde_json::Result<()> {
    let path = fixture("damaged.pgn");
    assert_eq!(
        ask(json!({"type": "OpenPgnFile", "path": path}))?,
        json!({"type": "OpenPgnFile", "answer": {
            "type": "File",
            "path": path,
            "rows": [
                {"number": 1, "white": "Alpha", "black": "Beta", "result": "1-0",
                 "date": "?", "event": "Example"},
                {"number": 2, "white": "Gamma", "black": "Delta", "result": "*",
                 "date": "?", "event": "Example"},
                {"number": 3, "white": "Epsilon", "black": "Zeta", "result": "0-1",
                 "date": "?", "event": "Example"},
            ],
            "problems": 1,
        }})
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn answers_a_game_request_while_a_file_is_still_read() -> io::Result<()> {
    use std::fs::File;
    use std::io::Write as _;
    use std::process::Command;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    let pipe = temporary_path("held.pgn");
    assert!(Command::new("mkfifo").arg(&pipe).status()?.success());
    let games = SharedGames::default();
    let open = json!({"type": "OpenPgnFile", "path": pipe.to_string_lossy()});
    let (opened, new_game) = thread::scope(|scope| {
        let reading = scope.spawn(|| ask_games(&games, open));
        // This returns when Rust has the pipe open. From then on Rust waits there for a game.
        let mut writer = File::options().write(true).open(&pipe)?;
        let (sender, receiver) = mpsc::channel();
        let shared = &games;
        scope.spawn(move || sender.send(ask_games(shared, json!({"type": "NewGame"}))));
        let new_game = receiver.recv_timeout(Duration::from_secs(10));
        writer.write_all(b"[White \"Late\"]\n\n1. e4 *\n")?;
        drop(writer);
        io::Result::Ok((reading.join(), new_game))
    })?;
    assert_eq!(new_game.unwrap()?["game"], json!(1));
    assert_eq!(
        opened.unwrap()?["answer"]["rows"],
        json!([{"number": 1, "white": "Late", "black": "?", "result": "*",
                "date": "?", "event": "?"}])
    );
    fs::remove_file(pipe)
}

#[test]
fn takes_white_space_away_from_the_ends_of_a_typed_path() -> serde_json::Result<()> {
    let path = fixture("clean.pgn");
    let answer = ask(json!({"type": "OpenPgnFile", "path": format!("  {path}\t\n")}))?;
    assert_eq!(answer["answer"]["path"], json!(path));
    assert_eq!(answer["answer"]["problems"], json!(0));
    Ok(())
}

#[test]
fn creates_an_empty_file_and_never_replaces_a_file_that_exists() -> io::Result<()> {
    let path = temporary_path("create.pgn");
    let typed = path.to_string_lossy();
    assert_eq!(
        ask(json!({"type": "CreatePgnFile", "path": typed}))?,
        json!({"type": "CreatePgnFile", "answer": {
            "type": "File", "path": typed, "rows": [], "problems": 0,
        }})
    );
    assert_eq!(fs::read(&path)?, b"");

    fs::write(&path, b"1. e4 *\n")?;
    assert_eq!(
        ask(json!({"type": "CreatePgnFile", "path": typed}))?,
        json!({"type": "CreatePgnFile", "answer": {
            "type": "Refusal", "reason": "existsAlready", "system": null,
        }})
    );
    assert_eq!(fs::read(&path)?, b"1. e4 *\n");
    fs::remove_file(path)
}

#[cfg(target_os = "linux")]
#[test]
fn refuses_a_file_that_cannot_be_read_and_a_file_that_cannot_be_created() -> serde_json::Result<()>
{
    let missing = temporary_path("missing-folder").join("mine.pgn");
    let typed = missing.to_string_lossy();
    assert_eq!(
        ask(json!({"type": "OpenPgnFile", "path": typed}))?,
        json!({"type": "OpenPgnFile", "answer": {
            "type": "Refusal",
            "reason": "cannotRead",
            "system": "No such file or directory (os error 2)",
        }})
    );
    assert_eq!(
        ask(json!({"type": "CreatePgnFile", "path": typed}))?,
        json!({"type": "CreatePgnFile", "answer": {
            "type": "Refusal",
            "reason": "cannotCreate",
            "system": "No such file or directory (os error 2)",
        }})
    );
    Ok(())
}
