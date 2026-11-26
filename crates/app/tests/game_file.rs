//! Opening a game of a PGN file and saving it back, as the view sends and receives it.

use std::path::{Path, PathBuf};
use std::{env, fs, io, process};

use serde_json::{Value, json};
use tauri::async_runtime::block_on;
use zobrist_app::ipc::{Request, SharedGames};

fn ask(games: &SharedGames, request: Value) -> io::Result<Value> {
    let request: Request = serde_json::from_value(request)?;
    let response = block_on(games.answer(request)).map_err(io::Error::other)?;
    Ok(serde_json::to_value(response)?)
}

/// A new folder with copies of the two fixtures. Its path has every symbolic link followed, as
/// the path in an answer has.
fn folder(test: &str) -> io::Result<PathBuf> {
    let folder = env::temp_dir().join(format!("zobrist-app-games-{test}-{}", process::id()));
    fs::create_dir(&folder)?;
    for name in ["clean.pgn", "damaged.pgn"] {
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/tests/fixtures");
        fs::copy(fixture.join(name), folder.join(name))?;
    }
    folder.canonicalize()
}

fn open(games: &SharedGames, path: &Path, number: u64) -> io::Result<Value> {
    ask(
        games,
        json!({"type": "OpenGame", "path": path.to_string_lossy(), "number": number}),
    )
}

fn start_moves() -> Vec<Value> {
    "a2a3 a2a4 b1a3 b1c3 b2b3 b2b4 c2c3 c2c4 d2d3 d2d4 \
     e2e3 e2e4 f2f3 f2f4 g1f3 g1h3 g2g3 g2g4 h2h3 h2h4"
        .split_whitespace()
        .map(|entry| {
            let (from, to) = entry.split_at(2);
            json!({"from": from, "to": to, "promotion": false})
        })
        .collect()
}

fn notation_move(ply: usize, text: &str, comments_after: &[&str]) -> Value {
    let side = if ply.is_multiple_of(2) {
        "white"
    } else {
        "black"
    };
    json!({"place": vec![0; ply + 1], "number": ply / 2 + 1, "side": side, "text": text,
           "commentsBefore": [], "commentsAfter": comments_after, "symbols": [],
           "variations": []})
}

/// The picture of game 2 of `clean.pgn` at its start, as it comes from the file at `path`.
fn second_game_at_its_start(path: &Path, number: u64) -> Value {
    let mut legal_moves = start_moves();
    legal_moves.sort_by_key(Value::to_string);
    json!({
        "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
        "turn": "white", "inCheck": false,
        "legalMoves": legal_moves,
        "canUndo": false, "canRedo": false, "inVariation": false, "hasNextMove": true,
        "tags": [
            {"name": "Event", "value": "Example"}, {"name": "Site", "value": "?"},
            {"name": "Date", "value": "2024.01.01"}, {"name": "Round", "value": "2"},
            {"name": "White", "value": "Gamma"}, {"name": "Black", "value": "Delta"},
            {"name": "Result", "value": "0-1"},
        ],
        "result": "blackWin", "squares": [], "arrows": [],
        "startComments": [], "moveSymbol": null, "positionSymbol": null,
        "lastMove": null, "place": [],
        "notation": [
            notation_move(0, "f3", &[]), notation_move(1, "e5", &[]),
            notation_move(2, "g4", &[]), notation_move(3, "Qh4#", &[]),
        ],
        "file": {"path": path.to_string_lossy(), "number": number},
        "unsaved": false,
    })
}

/// The board offers its moves as a set, so the order of the answer does not count.
fn with_sorted_moves(mut answer: Value) -> Value {
    for path in ["/picture/legalMoves", "/answer/picture/legalMoves"] {
        if let Some(moves) = answer.pointer_mut(path).and_then(Value::as_array_mut) {
            moves.sort_by_key(Value::to_string);
        }
    }
    answer
}

#[test]
fn opens_a_game_of_a_file_at_its_start_with_its_file_and_without_changes() -> io::Result<()> {
    let clean = folder("open")?.join("clean.pgn");
    let games = SharedGames::default();
    assert_eq!(
        with_sorted_moves(open(&games, &clean, 2)?),
        json!({"type": "OpenGame", "answer": {
            "type": "Game", "game": 1, "picture": second_game_at_its_start(&clean, 2),
        }})
    );
    Ok(())
}

#[test]
fn opening_a_game_again_answers_with_the_game_that_is_open() -> io::Result<()> {
    let folder = folder("open-again")?;
    let clean = folder.join("clean.pgn");
    let games = SharedGames::default();
    let first = open(&games, &clean, 2)?;
    assert_eq!(first["answer"]["game"], json!(1));

    assert_eq!(open(&games, &clean, 2)?, first);
    let other_spelling = folder.join(".").join("clean.pgn");
    assert_eq!(open(&games, &other_spelling, 2)?, first);

    assert_eq!(open(&games, &clean, 1)?["answer"]["game"], json!(2));
    assert_eq!(ask(&games, json!({"type": "NewGame"}))?["game"], json!(3));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn refuses_a_number_that_the_file_does_not_have_and_a_file_that_cannot_be_read() -> io::Result<()> {
    let folder = folder("open-refused")?;
    let games = SharedGames::default();
    for number in [0, 3] {
        assert_eq!(
            open(&games, &folder.join("clean.pgn"), number)?,
            json!({"type": "OpenGame", "answer": {
                "type": "Refusal", "reason": "noSuchGame", "system": null,
            }})
        );
    }
    assert_eq!(
        open(&games, &folder.join("missing.pgn"), 1)?,
        json!({"type": "OpenGame", "answer": {
            "type": "Refusal", "reason": "cannotRead",
            "system": "No such file or directory (os error 2)",
        }})
    );
    assert_eq!(ask(&games, json!({"type": "NewGame"}))?["game"], json!(1));
    Ok(())
}

#[test]
fn a_new_game_has_changes_only_while_it_has_a_move() -> io::Result<()> {
    let games = SharedGames::default();
    let new_game = ask(&games, json!({"type": "NewGame"}))?;
    assert_eq!(new_game["picture"]["file"], json!(null));
    assert_eq!(new_game["picture"]["unsaved"], json!(false));

    let played = ask(
        &games,
        json!({"type": "PlayMove", "game": 1, "from": "e2", "to": "e4", "promotion": null}),
    )?;
    assert_eq!(played["picture"]["unsaved"], json!(true));

    let undone = ask(&games, json!({"type": "Undo", "game": 1}))?;
    assert_eq!(undone["picture"]["unsaved"], json!(false));
    Ok(())
}
