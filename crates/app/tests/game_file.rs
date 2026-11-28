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

#[test]
fn a_deleted_game_without_moves_can_still_be_saved_as_a_new_game() -> io::Result<()> {
    let directory = folder("delete-empty-game")?;
    let path = directory.join("only.pgn");
    fs::write(&path, b"*\n")?;
    let games = SharedGames::with_history(Ok(directory.join("data")));
    let listed = ask(&games, json!({"type":"OpenPgnFile", "path": path}))?;
    assert_eq!(open(&games, &path, 1)?["answer"]["game"], json!(1));
    let deleted = ask(
        &games,
        json!({
            "type":"DeletePgnGame", "path": path, "number": 1,
            "version": listed["answer"]["version"]
        }),
    )?;
    let tab = &deleted["answer"]["tabs"][0];
    assert_eq!(tab["previous"], json!({"path": path, "number": 1}));
    assert_eq!(tab["file"], json!(null));
    assert_eq!(tab["unsaved"], true);
    assert_eq!(fs::read(&path)?, b"");
    assert_eq!(save_as(&games, 1, &path)?["answer"]["type"], "Saved");
    assert_eq!(fs::read(path)?, b"*\n");
    fs::remove_dir_all(directory)
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
        "revision": 0, "fen": "rnbqkbnr/pppppppp/8/8/8/8/PPPPPPPP/RNBQKBNR w KQkq - 0 1",
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
        "comment": "", "startComments": [], "moveSymbol": null, "positionSymbol": null,
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

const SECOND_GAME_WITH_COMMENT: &[u8] = b"[Event \"Example\"]\n[Site \"?\"]\n\
    [Date \"2024.01.01\"]\n[Round \"2\"]\n[White \"Gamma\"]\n[Black \"Delta\"]\n\
    [Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# {Fool's mate} 0-1\n";

/// The bytes of `clean.pgn` as the repository holds them. Its second game starts at byte 194.
fn clean_fixture() -> io::Result<Vec<u8>> {
    fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("../cli/tests/fixtures/clean.pgn"))
}

fn refusal(kind: &str, reason: &str, system: Option<&str>) -> Value {
    json!({"type": kind, "answer": {"type": "Refusal", "reason": reason, "system": system}})
}

/// Opens game 2 of `clean.pgn` and writes the comment `Fool's mate` at its last move.
fn open_and_comment(games: &SharedGames, clean: &Path) -> io::Result<Value> {
    open(games, clean, 2)?;
    ask(
        games,
        json!({"type": "GoTo", "game": 1, "place": [0, 0, 0, 0]}),
    )?;
    ask(
        games,
        json!({"type": "SetComment", "game": 1, "text": "Fool's mate"}),
    )
}

/// The picture of game 2 of `clean.pgn` at its last move, with the comment.
fn second_game_with_comment(path: &Path, unsaved: bool) -> Value {
    let mut picture = second_game_at_its_start(path, 2);
    picture["revision"] = json!(1);
    picture["fen"] = json!("rnb1kbnr/pppp1ppp/8/4p3/6Pq/5P2/PPPPP2P/RNBQKBNR w KQkq - 1 3");
    picture["inCheck"] = json!(true);
    picture["legalMoves"] = json!([]);
    picture["canUndo"] = json!(true);
    picture["hasNextMove"] = json!(false);
    picture["lastMove"] = json!({"from": "d8", "to": "h4"});
    picture["place"] = json!([0, 0, 0, 0]);
    picture["comment"] = json!("Fool's mate");
    picture["notation"][3] = notation_move(3, "Qh4#", &["Fool's mate"]);
    picture["unsaved"] = json!(unsaved);
    picture
}

#[test]
fn saves_a_changed_game_into_its_place_and_leaves_the_other_game_alone() -> io::Result<()> {
    let clean = folder("save")?.join("clean.pgn");
    let games = SharedGames::default();
    assert_eq!(
        open_and_comment(&games, &clean)?,
        json!({"type": "SetComment", "picture": second_game_with_comment(&clean, true)})
    );

    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        json!({"type": "SaveGame", "answer": {
            "type": "Saved", "picture": second_game_with_comment(&clean, false),
        }})
    );
    assert_eq!(
        fs::read(&clean)?,
        [&clean_fixture()?[..194], SECOND_GAME_WITH_COMMENT].concat()
    );
    Ok(())
}

#[test]
fn a_game_with_a_problem_opens_with_its_kept_moves_and_a_save_replaces_it() -> io::Result<()> {
    let damaged = folder("save-damaged")?.join("damaged.pgn");
    let fixture = fs::read(&damaged)?;
    let games = SharedGames::default();
    let opened = open(&games, &damaged, 2)?;
    assert_eq!(
        opened["answer"]["picture"]["notation"],
        json!([notation_move(0, "e4", &[]), notation_move(1, "e5", &[])])
    );
    assert_eq!(opened["answer"]["picture"]["unsaved"], json!(false));

    let saved = ask(&games, json!({"type": "SaveGame", "game": 1}))?;
    assert_eq!(saved["answer"]["type"], json!("Saved"));
    assert_eq!(saved["answer"]["picture"]["unsaved"], json!(false));
    let kept: &[u8] =
        b"[Event \"Example\"]\n[White \"Gamma\"]\n[Black \"Delta\"]\n[Result \"*\"]\n\n1. e4 e5 *";
    assert_eq!(
        fs::read(&damaged)?,
        [&fixture[..110], kept, &fixture[195..]].concat()
    );
    Ok(())
}

#[test]
fn an_undo_behind_a_save_has_changes_and_the_redo_has_none() -> io::Result<()> {
    let clean = folder("save-undo")?.join("clean.pgn");
    let games = SharedGames::default();
    open_and_comment(&games, &clean)?;
    ask(&games, json!({"type": "SaveGame", "game": 1}))?;

    let undone = ask(&games, json!({"type": "Undo", "game": 1}))?;
    assert_eq!(undone["picture"]["notation"][3]["commentsAfter"], json!([]));
    assert_eq!(undone["picture"]["unsaved"], json!(true));
    let redone = ask(&games, json!({"type": "Redo", "game": 1}))?;
    assert_eq!(redone["picture"]["unsaved"], json!(false));
    Ok(())
}

#[test]
fn refuses_to_save_into_a_file_that_changed_and_keeps_every_byte_of_it() -> io::Result<()> {
    let clean = folder("save-changed")?.join("clean.pgn");
    let games = SharedGames::default();
    open_and_comment(&games, &clean)?;

    let in_front = [b"[White \"Front\"]\n\n1. c4 *\n\n", &clean_fixture()?[..]].concat();
    fs::write(&clean, &in_front)?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        refusal("SaveGame", "fileChanged", None)
    );
    assert_eq!(fs::read(&clean)?, in_front);

    let one_game = &clean_fixture()?[..194];
    fs::write(&clean, one_game)?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        refusal("SaveGame", "fileChanged", None)
    );
    assert_eq!(fs::read(&clean)?, one_game);
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn says_why_a_game_cannot_be_saved_into_its_place() -> io::Result<()> {
    use std::os::unix::fs::PermissionsExt as _;

    let folder = folder("save-refused")?;
    let clean = folder.join("clean.pgn");
    let games = SharedGames::default();
    open_and_comment(&games, &clean)?;

    fs::set_permissions(&clean, fs::Permissions::from_mode(0o444))?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        refusal(
            "SaveGame",
            "cannotWrite",
            Some("Permission denied (os error 13)")
        )
    );
    assert_eq!(fs::read(&clean)?, clean_fixture()?);

    fs::remove_file(&clean)?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        refusal(
            "SaveGame",
            "cannotRead",
            Some("No such file or directory (os error 2)")
        )
    );

    let only_tags = folder.join("only-tags.pgn");
    let two_games: &[u8] = b"[White \"A\"]\n\n[White \"B\"]\n\n1. e4 *\n";
    fs::write(&only_tags, two_games)?;
    assert_eq!(open(&games, &only_tags, 2)?["answer"]["game"], json!(2));
    ask(
        &games,
        json!({"type": "SetTag", "game": 2, "tag": "Black", "text": "Beta"}),
    )?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 2}))?,
        refusal(
            "SaveGame",
            "cannotReplace",
            Some(
                "game 2 has only tags or follows a game with only tags, \
                 so its place in the file is unknown"
            )
        )
    );
    assert_eq!(fs::read(&only_tags)?, two_games);
    Ok(())
}

#[test]
fn a_game_without_a_file_cannot_be_saved_in_place() -> io::Result<()> {
    let games = SharedGames::default();
    new_game_with_e4(&games)?;
    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        refusal("SaveGame", "noFile", None)
    );
    Ok(())
}

fn new_game_with_e4(games: &SharedGames) -> io::Result<Value> {
    ask(games, json!({"type": "NewGame"}))?;
    ask(
        games,
        json!({"type": "PlayMove", "game": 1, "from": "e2", "to": "e4", "promotion": null}),
    )
}

fn save_as(games: &SharedGames, game: u64, path: &Path) -> io::Result<Value> {
    ask(
        games,
        json!({"type": "SaveGameAs", "game": game, "path": path.to_string_lossy()}),
    )
}

#[test]
fn saves_a_new_game_as_the_last_game_of_a_file() -> io::Result<()> {
    let clean = folder("save-as")?.join("clean.pgn");
    let games = SharedGames::default();
    let mut picture = new_game_with_e4(&games)?["picture"].take();
    assert_eq!(picture["file"], json!(null));
    assert_eq!(picture["unsaved"], json!(true));

    picture["file"] = json!({"path": clean.to_string_lossy(), "number": 3});
    picture["unsaved"] = json!(false);
    assert_eq!(
        save_as(&games, 1, &clean)?,
        json!({"type": "SaveGameAs", "answer": {"type": "Saved", "picture": picture}})
    );
    assert_eq!(
        fs::read(&clean)?,
        [&clean_fixture()?[..], b"\n1. e4 *\n"].concat()
    );
    Ok(())
}

#[test]
fn reports_a_saved_game_when_the_desktop_cannot_locate_its_data_folder() -> io::Result<()> {
    let path = folder("save-no-data-folder")?.join("game.pgn");
    fs::write(&path, b"1. d4 *\n")?;
    let games = SharedGames::with_history(Err("Local data folder unavailable.".into()));
    new_game_with_e4(&games)?;

    let saved = save_as(&games, 1, &path)?;
    assert_eq!(saved["answer"]["type"], "SavedWithWarning");
    assert_eq!(
        saved["answer"]["warning"],
        "Game saved. Could not keep the earlier state. Local data folder unavailable."
    );
    assert_eq!(saved["answer"]["picture"]["unsaved"], false);
    assert_eq!(fs::read(path)?, b"1. d4 *\n\n1. e4 *\n");
    Ok(())
}

#[test]
fn saving_as_a_new_game_creates_a_file_that_is_not_there() -> io::Result<()> {
    let new = folder("save-as-new")?.join("new.pgn");
    let games = SharedGames::default();
    new_game_with_e4(&games)?;

    let saved = save_as(&games, 1, &new)?;
    assert_eq!(
        saved["answer"]["picture"]["file"],
        json!({"path": new.to_string_lossy(), "number": 1})
    );
    assert_eq!(saved["answer"]["picture"]["unsaved"], json!(false));
    assert_eq!(fs::read(&new)?, b"1. e4 *\n");
    Ok(())
}

#[test]
fn saving_a_game_of_a_file_as_a_new_game_moves_it_to_the_new_place() -> io::Result<()> {
    let clean = folder("save-as-own")?.join("clean.pgn");
    let games = SharedGames::default();
    open(&games, &clean, 2)?;

    assert_eq!(
        with_sorted_moves(save_as(&games, 1, &clean)?),
        json!({"type": "SaveGameAs", "answer": {
            "type": "Saved", "picture": second_game_at_its_start(&clean, 3),
        }})
    );
    let fixture = clean_fixture()?;
    assert_eq!(
        fs::read(&clean)?,
        [&fixture[..], b"\n", &fixture[194..]].concat()
    );

    assert_eq!(open(&games, &clean, 3)?["answer"]["game"], json!(1));
    assert_eq!(open(&games, &clean, 2)?["answer"]["game"], json!(2));
    Ok(())
}

#[test]
fn refuses_to_add_a_game_behind_a_comment_that_is_still_open() -> io::Result<()> {
    let open = folder("save-as-open")?.join("open.pgn");
    let old: &[u8] = b"1. d4 {unfinished";
    fs::write(&open, old)?;
    let games = SharedGames::default();
    new_game_with_e4(&games)?;

    assert_eq!(
        save_as(&games, 1, &open)?,
        refusal(
            "SaveGameAs",
            "cannotWrite",
            Some("the file would not read back with the new game as its last game")
        )
    );
    assert_eq!(fs::read(&open)?, old);
    let picture = ask(&games, json!({"type": "GoTo", "game": 1, "place": [0]}))?;
    assert_eq!(picture["picture"]["file"], json!(null));
    assert_eq!(picture["picture"]["unsaved"], json!(true));
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn says_why_a_game_cannot_be_saved_as_a_new_game() -> io::Result<()> {
    let folder = folder("save-as-refused")?;
    let games = SharedGames::default();
    new_game_with_e4(&games)?;
    assert_eq!(
        save_as(&games, 1, &folder.join("missing-folder").join("new.pgn"))?,
        refusal(
            "SaveGameAs",
            "cannotWrite",
            Some("No such file or directory (os error 2)")
        )
    );
    Ok(())
}

#[test]
fn a_request_to_save_a_closed_game_answers_without_a_picture() -> io::Result<()> {
    let clean = folder("closed")?.join("clean.pgn");
    let games = SharedGames::default();
    open(&games, &clean, 2)?;
    ask(&games, json!({"type": "CloseGame", "game": 1}))?;

    assert_eq!(
        ask(&games, json!({"type": "SaveGame", "game": 1}))?,
        json!({"type": "SaveGame", "answer": null})
    );
    assert_eq!(
        save_as(&games, 1, &clean)?,
        json!({"type": "SaveGameAs", "answer": null})
    );
    assert_eq!(fs::read(&clean)?, clean_fixture()?);
    assert_eq!(open(&games, &clean, 2)?["answer"]["game"], json!(2));
    Ok(())
}

/// Sends a request that waits at the pipe at `path`, and in that time a request for a new
/// game. Then it writes `text` into the pipe. Returns the two answers in that order.
#[cfg(target_os = "linux")]
fn ask_at_a_pipe(
    games: &SharedGames,
    path: &Path,
    request: Value,
    text: &[u8],
) -> io::Result<(Value, Value)> {
    use std::io::Write as _;
    use std::sync::mpsc;
    use std::time::Duration;

    std::thread::scope(|scope| {
        let waiting = scope.spawn(|| ask(games, request));
        // This returns when Rust has the pipe open. From then on Rust waits there for the text.
        let mut writer = fs::File::options().write(true).open(path)?;
        let (sender, receiver) = mpsc::channel();
        scope.spawn(move || sender.send(ask(games, json!({"type": "NewGame"}))));
        let new_game = receiver.recv_timeout(Duration::from_secs(10));
        writer.write_all(text)?;
        drop(writer);
        let waited = waiting
            .join()
            .map_err(|_| io::Error::other("the request at the pipe panicked"))??;
        Ok((waited, new_game.map_err(io::Error::other)??))
    })
}

#[cfg(target_os = "linux")]
fn pipe_at(path: &Path) -> io::Result<()> {
    let made = process::Command::new("mkfifo").arg(path).status()?;
    assert!(made.success());
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn opens_a_new_game_while_a_game_of_a_slow_file_is_read() -> io::Result<()> {
    let slow = folder("slow-open")?.join("slow.pgn");
    pipe_at(&slow)?;
    let games = SharedGames::default();
    let request = json!({"type": "OpenGame", "path": slow.to_string_lossy(), "number": 1});

    let (opened, new_game) = ask_at_a_pipe(&games, &slow, request, b"1. e4 *\n")?;
    assert_eq!(new_game["game"], json!(1));
    assert_eq!(opened["answer"]["game"], json!(2));
    assert_eq!(
        opened["answer"]["picture"]["file"],
        json!({"path": slow.to_string_lossy(), "number": 1})
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn opens_a_new_game_while_a_save_waits_at_its_file() -> io::Result<()> {
    let clean = folder("slow-save")?.join("clean.pgn");
    let games = SharedGames::default();
    open_and_comment(&games, &clean)?;
    fs::remove_file(&clean)?;
    pipe_at(&clean)?;

    let save = json!({"type": "SaveGame", "game": 1});
    let (saved, new_game) = ask_at_a_pipe(&games, &clean, save, b"1. c4 *\n")?;
    assert_eq!(new_game["game"], json!(2));
    assert_eq!(saved, refusal("SaveGame", "fileChanged", None));
    Ok(())
}

#[test]
fn two_saves_at_once_into_one_file_both_arrive() -> io::Result<()> {
    let clean = folder("two-saves")?.join("clean.pgn");
    let games = SharedGames::default();
    open(&games, &clean, 1)?;
    open(&games, &clean, 2)?;
    for round in 0..40 {
        let site = format!("round {round}");
        for game in [1, 2] {
            ask(
                &games,
                json!({"type": "SetTag", "game": game, "tag": "Site", "text": site}),
            )?;
        }
        let answers = std::thread::scope(|scope| {
            let save = |game| ask(&games, json!({"type": "SaveGame", "game": game}));
            [1, 2]
                .map(|game| scope.spawn(move || save(game)))
                .map(std::thread::ScopedJoinHandle::join)
        });
        for answer in answers {
            assert_eq!(answer.unwrap()?["answer"]["type"], json!("Saved"));
        }
        let text = fs::read_to_string(&clean)?;
        assert_eq!(text.matches(&format!("[Site \"{site}\"]")).count(), 2);
    }
    Ok(())
}
