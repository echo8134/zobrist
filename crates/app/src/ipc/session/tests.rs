use std::path::{Path, PathBuf};
use std::{env, fs, io, process};

use serde_json::{Value, json};
use tauri::async_runtime::block_on;

use crate::ipc::{DesktopRequest, SharedGames};

fn directory(test: &str) -> io::Result<PathBuf> {
    let directory = env::temp_dir().join(format!("zobrist-boards-{test}-{}", process::id()));
    fs::create_dir(&directory)?;
    directory.canonicalize()
}

fn ask(games: &SharedGames, directory: &Path, request: Value) -> io::Result<Value> {
    let response = match serde_json::from_value(request)? {
        DesktopRequest::Editor(request) => {
            block_on(games.answer(request)).map_err(io::Error::other)?
        }
        DesktopRequest::Session(request) => {
            block_on(games.answer_session(request, directory.to_owned()))?
        }
        DesktopRequest::Collection(_) => {
            return Err(io::Error::other("unexpected collection request"));
        }
    };
    Ok(serde_json::to_value(response)?)
}

fn restore(games: &SharedGames, directory: &Path) -> io::Result<Value> {
    ask(games, directory, json!({"type":"RestoreBoards"}))
}

fn quit(
    games: &SharedGames,
    directory: &Path,
    boards: &[usize],
    selected: Option<usize>,
) -> io::Result<Value> {
    ask(
        games,
        directory,
        json!({"type":"QuitWithBoards", "boards":boards, "selected":selected}),
    )
}

#[test]
fn reopens_dirty_and_clean_boards_in_tab_order_and_saves_to_the_original_game() -> io::Result<()> {
    let directory = directory("round-trip")?;
    let source = directory.join("games.pgn");
    let fixture = include_bytes!("../../../../cli/tests/fixtures/clean.pgn");
    fs::write(&source, fixture)?;
    let games = SharedGames::default();
    let send = |request| ask(&games, &directory, request);
    let open = |number| json!({"type":"OpenGame","path":source,"number":number});

    assert_eq!(
        restore(&games, &directory)?,
        json!({"type":"RestoreBoards","boards":[],"selected":null})
    );
    send(open(2))?;
    send(json!({"type":"GoTo","game":1,"place":[0,0,0,0]}))?;
    send(json!({"type":"SetComment","game":1,"text":"Final move"}))?;
    send(open(1))?;
    send(json!({"type":"NewGame"}))?;
    send(json!({"type":"CloseGame","game":3}))?;
    send(json!({"type":"OpenPgnText","text":"1. d4 d5 (1... Nf6) *"}))?;
    send(json!({"type":"GoTo","game":4,"place":[0,1]}))?;
    assert_eq!(
        quit(&games, &directory, &[2, 1, 4], Some(4))?,
        json!({"type":"QuitWithBoards"})
    );
    assert_eq!(fs::read(&source)?, fixture);

    let restored = SharedGames::default();
    let answer = restore(&restored, &directory)?;
    let boards = answer["boards"].as_array().unwrap();
    assert_eq!(boards.len(), 3);
    for (board, (game, number, dirty)) in boards.iter().zip([(1, 1, false), (2, 2, true)]) {
        assert_eq!(board["game"], game);
        assert_eq!(
            board["picture"]["file"],
            json!({"path":source,"number":number})
        );
        assert_eq!(board["picture"]["unsaved"], dirty);
    }
    assert_eq!(answer["selected"], 3);
    assert_eq!(boards[1]["picture"]["place"], json!([0, 0, 0, 0]));
    assert_eq!(boards[1]["picture"]["notation"][3]["text"], "Qh4#");
    assert_eq!(
        boards[1]["picture"]["notation"][3]["commentsAfter"],
        json!(["Final move"])
    );
    assert_eq!(boards[2]["picture"]["place"], json!([0, 1]));
    assert_eq!(boards[2]["picture"]["notation"][1]["text"], "d5");
    assert_eq!(
        boards[2]["picture"]["notation"][1]["variations"][0][0]["text"],
        "Nf6"
    );
    assert_eq!(boards[2]["picture"]["unsaved"], true);
    assert_eq!(boards[2]["picture"]["file"], json!(null));
    assert_eq!(boards[1]["picture"]["canUndo"], false);
    assert_eq!(restore(&restored, &directory)?, answer);

    let send = |request| ask(&restored, &directory, request);
    let saved = send(json!({"type":"SaveGame","game":2}))?;
    assert_eq!(saved["answer"]["picture"]["unsaved"], false);
    assert_eq!(send(open(2))?["answer"]["game"], 2);
    let expected = String::from_utf8_lossy(fixture).replace("Qh4# 0-1", "Qh4# {Final move} 0-1");
    assert_eq!(fs::read(&source)?, expected.as_bytes());

    send(json!({"type":"SetComment","game":3,"text":"still open"}))?;
    send(json!({"type":"CloseGame","game":1}))?;
    let after_refresh = restore(&restored, &directory)?;
    assert_eq!(after_refresh["boards"].as_array().unwrap().len(), 2);
    assert_eq!(after_refresh["boards"][0]["game"], 2);
    assert_eq!(
        after_refresh["boards"][1]["picture"]["notation"][1]["variations"][0][0]["commentsAfter"],
        json!(["still open"])
    );
    fs::remove_dir_all(directory)
}

#[test]
fn reopens_both_boards_when_a_new_game_reuses_the_first_boards_file_and_number() -> io::Result<()> {
    let directory = directory("shared-origin")?;
    let source = directory.join("games.pgn");
    fs::write(&source, "1. d4 *\n")?;
    let games = SharedGames::default();
    let send = |request| ask(&games, &directory, request);
    send(json!({"type":"OpenGame","path":source,"number":1}))?;
    fs::remove_file(&source)?;
    send(json!({"type":"NewGame"}))?;
    send(json!({"type":"PlayMove","game":2,"from":"e2","to":"e4","promotion":null}))?;
    send(json!({"type":"SaveGameAs","game":2,"path":source}))?;
    assert_eq!(
        quit(&games, &directory, &[1, 2], Some(2))?,
        json!({"type":"QuitWithBoards"})
    );

    let answer = restore(&SharedGames::default(), &directory)?;
    let boards = answer["boards"].as_array().unwrap();
    assert_eq!(boards.len(), 2);
    assert_eq!(answer["selected"], 2);
    for (board, first_move) in boards.iter().zip(["d4", "e4"]) {
        assert_eq!(board["picture"]["file"], json!({"path":source,"number":1}));
        assert_eq!(board["picture"]["notation"][0]["text"], first_move);
    }
    fs::remove_dir_all(directory)
}

#[test]
fn retains_the_game_last_read_for_conflict_detection_even_if_its_file_is_missing() -> io::Result<()>
{
    let directory = directory("missing-file")?;
    let source = directory.join("games.pgn");
    fs::write(&source, "[White \"Gamma\"]\n\n1. d4 *\n")?;
    let games = SharedGames::default();
    ask(
        &games,
        &directory,
        json!({"type":"OpenGame","path":source,"number":1}),
    )?;
    ask(
        &games,
        &directory,
        json!({"type":"SetComment","game":1,"text":"Final move"}),
    )?;
    quit(&games, &directory, &[1], Some(1))?;
    fs::remove_file(&source)?;

    let restored = SharedGames::default();
    let answer = restore(&restored, &directory)?;
    assert_eq!(answer["boards"][0]["picture"]["tags"][0]["value"], "Gamma");
    assert_eq!(
        answer["boards"][0]["picture"]["notation"][0]["commentsBefore"],
        json!(["Final move"])
    );
    let outside = b"[White \"Outside\"]\n\n1. d4 *\n";
    fs::write(&source, outside)?;
    let saved = ask(&restored, &directory, json!({"type":"SaveGame","game":1}))?;
    assert_eq!(
        saved,
        json!({"type":"SaveGame","answer":{"type":"Refusal","reason":"fileChanged","system":null}})
    );
    assert_eq!(fs::read(source)?, outside);
    fs::remove_dir_all(directory)
}

#[test]
fn empty_boards_and_closed_tabs_survive_repeated_restarts() -> io::Result<()> {
    let directory = directory("closed")?;
    let games = SharedGames::default();
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    quit(&games, &directory, &[1], None)?;
    let reopened = SharedGames::default();
    let answer = restore(&reopened, &directory)?;
    assert_eq!(answer["boards"].as_array().unwrap().len(), 1);
    assert_eq!(answer["boards"][0]["picture"]["unsaved"], false);
    assert_eq!(answer["boards"][0]["picture"]["place"], json!([]));
    assert_eq!(answer["selected"], Value::Null);
    ask(&reopened, &directory, json!({"type":"CloseGame","game":1}))?;
    quit(&reopened, &directory, &[], None)?;
    for _ in 0..2 {
        let fresh = SharedGames::default();
        assert_eq!(
            restore(&fresh, &directory)?,
            json!({"type":"RestoreBoards","boards":[],"selected":null})
        );
        quit(&fresh, &directory, &[], None)?;
    }
    fs::remove_dir_all(directory)
}

#[test]
fn a_damaged_record_installs_no_games_and_can_be_retried() -> io::Result<()> {
    let directory = directory("damaged")?;
    let record = directory.join("boards.json");
    let board = json!({"game":"*\n","place":[],"origin":null});
    let invalid_place = json!({"game":"*\n","place":[0],"origin":null});
    let invalid_game = json!({"game":"1. e4 e5 2. e5 *\n","place":[],"origin":null});
    let games = SharedGames::default();
    for bad in [
        json!({"boards":[board],"selected":1}),
        json!({"boards":[board,invalid_place],"selected":0}),
        json!({"boards":[board,invalid_game],"selected":0}),
    ] {
        let bytes = serde_json::to_vec(&bad)?;
        fs::write(&record, &bytes)?;
        assert!(restore(&games, &directory).is_err());
        assert!(!games.is_restored());
        assert_eq!(fs::read(&record)?, bytes);
    }
    fs::write(
        record,
        serde_json::to_vec(&json!({"boards":[board],"selected":0}))?,
    )?;
    let answer = restore(&games, &directory)?;
    assert_eq!(answer["selected"], 1);
    assert!(games.is_restored());
    assert_eq!(answer["boards"].as_array().unwrap().len(), 1);
    fs::remove_dir_all(directory)
}

#[test]
fn failed_writes_and_bad_tab_lists_leave_the_previous_session_and_current_edits_intact()
-> io::Result<()> {
    let directory = directory("failed-quit")?;
    let games = SharedGames::default();
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    quit(&games, &directory, &[1], Some(1))?;
    let record = directory.join("boards.json");
    let previous = fs::read(&record)?;
    ask(
        &games,
        &directory,
        json!({"type":"SetComment","game":1,"text":"an edit"}),
    )?;
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    for (boards, selected) in [
        (&[1][..], Some(1)),
        (&[1, 1][..], Some(1)),
        (&[1, 3][..], None),
        (&[1, 2][..], Some(3)),
    ] {
        assert!(quit(&games, &directory, boards, selected).is_err());
    }
    // The data directory is unavailable when its path names a regular file.
    assert!(quit(&games, &record, &[1, 2], Some(1)).is_err());
    assert_eq!(fs::read(&record)?, previous);
    assert_eq!(
        ask(&games, &directory, json!({"type":"GamePgn","game":1}))?["text"],
        "{an edit} *\n"
    );
    fs::remove_dir_all(directory)
}

#[test]
fn an_unwritable_game_keeps_the_previous_record() -> io::Result<()> {
    let directory = directory("unwritable-game")?;
    let games = SharedGames::default();
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    quit(&games, &directory, &[1], Some(1))?;
    let record = directory.join("boards.json");
    let previous = fs::read(&record)?;
    ask(
        &games,
        &directory,
        json!({"type":"OpenPgnText","text":"[FEN \"invalid\"]\n\n*\n"}),
    )?;
    ask(
        &games,
        &directory,
        json!({"type":"PlayMove","game":2,"from":"e2","to":"e4","promotion":null}),
    )?;
    assert!(quit(&games, &directory, &[1, 2], Some(2)).is_err());
    assert_eq!(fs::read(record)?, previous);
    fs::remove_dir_all(directory)
}
