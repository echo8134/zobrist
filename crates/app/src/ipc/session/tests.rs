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
        DesktopRequest::Analysis(request) => {
            block_on(request.answer(games, directory.to_owned())).map_err(io::Error::other)?
        }
        DesktopRequest::Collection(_)
        | DesktopRequest::Account(_)
        | DesktopRequest::Appearance(_)
        | DesktopRequest::Databases(_) => {
            return Err(io::Error::other("unexpected request in board session test"));
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
    assert_eq!(
        send(json!({"type":"Analysis","action":"Status","game":3,"generation":0}))?,
        json!({"type":"Analysis","snapshot":{
            "game":3,"place":[0,1],"revision":0,"generation":0,"state":{"type":"Stopped"}
        }})
    );
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
fn checkpoints_publish_only_changed_sessions_and_retry_a_failed_write() -> io::Result<()> {
    let directory = directory("checkpoint")?;
    let record = directory.join("boards.json");
    let original = br#"{"selected":null, "boards": []}"#;
    fs::write(&record, original)?;
    let games = SharedGames::default();
    restore(&games, &directory)?;
    let empty = json!({"type":"CheckpointBoards","boards":[],"selected":null});
    assert_eq!(
        ask(&games, &directory, empty)?,
        json!({"type":"CheckpointBoards"})
    );
    assert_eq!(fs::read(&record)?, original);

    let send = |request| ask(&games, &directory, request);
    let checkpoint = json!({"type":"CheckpointBoards","boards":[1],"selected":1});
    send(json!({"type":"NewGame"}))?;
    send(json!({"type":"PlayMove","game":1,"from":"e2","to":"e4","promotion":null}))?;
    assert!(ask(&games, &record, checkpoint.clone()).is_err());
    assert_eq!(fs::read(&record)?, original);
    assert_eq!(
        send(checkpoint.clone())?,
        json!({"type":"CheckpointBoards"})
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&record)?)?,
        json!({"boards":[{"game":"1. e4 *\n","place":[0],"origin":null}],"selected":0})
    );

    let saved = fs::File::options().write(true).open(&record)?;
    saved.set_modified(std::time::SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1000))?;
    let modified = saved.metadata()?.modified()?;
    drop(saved);
    send(json!({"type":"GoTo","game":1,"place":[0]}))?;
    send(checkpoint)?;
    assert_eq!(fs::metadata(&record)?.modified()?, modified);
    quit(&games, &directory, &[1], Some(1))?;
    assert_eq!(fs::metadata(&record)?.modified()?, modified);
    fs::remove_dir_all(directory)
}

#[test]
fn an_incomplete_write_keeps_the_previous_session_and_later_changes_win() -> io::Result<()> {
    use std::future::Future as _;
    use std::pin::pin;
    use std::task::{Context, Waker};

    for last_request in ["CheckpointBoards", "QuitWithBoards"] {
        let directory = directory(last_request)?;
        let games = SharedGames::default();
        let send = |request| ask(&games, &directory, request);
        let play = |from, to| {
            send(json!({"type":"PlayMove","game":1,"from":from,"to":to,"promotion":null}))
        };
        send(json!({"type":"NewGame"}))?;
        play("e2", "e4")?;
        send(json!({"type":"CheckpointBoards","boards":[1],"selected":1}))?;
        play("e7", "e5")?;

        let mut writer = block_on(games.one_save.lock());
        let older = block_on(games.games.lock()).snapshot(&[1], Some(1))?;
        let staged = older.stage(&directory)?;
        let after_crash = restore(&SharedGames::default(), &directory)?;
        let picture = &after_crash["boards"][0]["picture"];
        assert_eq!(picture["notation"].as_array().unwrap().len(), 1);
        assert_eq!(picture["notation"][0]["text"], "e4");
        assert_eq!(picture["place"], json!([0]));

        play("g1", "f3")?;
        let request =
            serde_json::from_value(json!({"type":last_request,"boards":[1],"selected":1}))?;
        let mut newer_write = pin!(games.answer_session(request, directory.clone()));
        assert!(
            newer_write
                .as_mut()
                .poll(&mut Context::from_waker(Waker::noop()))
                .is_pending()
        );
        fs::rename(staged, directory.join("boards.json"))?;
        writer.published = Some(older);
        drop(writer);
        assert_eq!(
            serde_json::to_value(block_on(newer_write)?)?,
            json!({"type":last_request})
        );
        let after_crash = restore(&SharedGames::default(), &directory)?;
        let picture = &after_crash["boards"][0]["picture"];
        assert_eq!(picture["notation"][2]["text"], "Nf3");
        assert_eq!(picture["place"], json!([0, 0, 0]));
        fs::remove_dir_all(directory)?;
    }
    Ok(())
}

#[test]
fn a_truncated_session_keeps_its_bytes_beside_earlier_backups_and_allows_a_normal_quit()
-> io::Result<()> {
    let directory = directory("truncated")?;
    let original = br#"{"boards":["#;
    let earlier = directory.join("boards-recovery-1.json");
    let backup = directory.join("boards-recovery-2.json");
    fs::write(directory.join("boards.json"), original)?;
    fs::write(&earlier, b"earlier recovery")?;
    let games = SharedGames::default();
    assert_eq!(
        restore(&games, &directory)?,
        json!({"type":"RestoreBoards","boards":[],"selected":null,"warning":format!(
            "Could not read the board session. The original file was kept at {}. Your PGN files are untouched. You can open a board and keep playing.", backup.display())})
    );
    assert!(games.is_restored());
    assert!(!directory.join("boards.json").exists());
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    quit(&games, &directory, &[1], Some(1))?;
    let next = restore(&SharedGames::default(), &directory)?;
    assert_eq!(next["selected"], 1);
    assert_eq!(next["boards"].as_array().unwrap().len(), 1);
    assert!(next.get("warning").is_none());
    assert_eq!(fs::read(backup)?, original);
    assert_eq!(fs::read(earlier)?, b"earlier recovery");
    fs::remove_dir_all(directory)
}

#[test]
fn bad_boards_leave_readable_boards_in_order_with_the_saved_selection() -> io::Result<()> {
    let directory = directory("partial")?;
    let illegal = json!({"game":"1. e4 e5 2. e5 *\n","place":[],"origin":null});
    let missing_move = json!({"game":"*\n","place":[0],"origin":null});
    let first = json!({"game":"1. d4 *\n","place":[0],"origin":null});
    let second = json!({"game":"1. e4 *\n","place":[],"origin":null});
    for (number, boards, selected, lost) in [
        (1, json!([illegal, first]), 1, "1 board"),
        (2, json!([missing_move, first]), 1, "1 board"),
        (
            3,
            json!([illegal, first, missing_move, second]),
            3,
            "2 boards",
        ),
    ] {
        let original = serde_json::to_vec(&json!({"boards":boards,"selected":selected}))?;
        fs::write(directory.join("boards.json"), &original)?;
        let backup = directory.join(format!("boards-recovery-{number}.json"));
        let games = SharedGames::default();
        let answer = restore(&games, &directory)?;
        assert_eq!(
            answer["warning"],
            format!(
                "{lost} could not be reopened. The original file was kept at {}. Your PGN files are untouched. You can open a board and keep playing.",
                backup.display()
            )
        );
        let boards = answer["boards"].as_array().unwrap();
        assert_eq!(boards[0]["picture"]["notation"][0]["text"], "d4");
        assert_eq!(boards[0]["picture"]["place"], json!([0]));
        let after_crash = restore(&SharedGames::default(), &directory)?;
        assert_eq!(after_crash["boards"], answer["boards"]);
        assert_eq!(after_crash["selected"], answer["selected"]);
        assert!(after_crash.get("warning").is_none());
        if number == 3 {
            assert_eq!(boards.len(), 2);
            assert_eq!(boards[1]["picture"]["notation"][0]["text"], "e4");
            assert_eq!(answer["selected"], 2);
            quit(&games, &directory, &[1, 2], Some(2))?;
        } else {
            assert_eq!(boards.len(), 1);
            assert_eq!(answer["selected"], 1);
            quit(&games, &directory, &[1], Some(1))?;
        }
        let next = restore(&SharedGames::default(), &directory)?;
        assert_eq!(next["boards"], answer["boards"]);
        assert_eq!(next["selected"], answer["selected"]);
        assert!(next.get("warning").is_none());
        assert_eq!(fs::read(backup)?, original);
    }
    let original = serde_json::to_vec(&json!({"boards":[missing_move],"selected":0}))?;
    fs::write(directory.join("boards.json"), &original)?;
    let backup = directory.join("boards-recovery-4.json");
    assert_eq!(
        restore(&SharedGames::default(), &directory)?,
        json!({"type":"RestoreBoards","boards":[],"selected":null,"warning":format!(
            "1 board could not be reopened. The original file was kept at {}. Your PGN files are untouched. You can open a board and keep playing.", backup.display())})
    );
    assert_eq!(fs::read(backup)?, original);
    fs::remove_dir_all(directory)
}

#[test]
fn an_invalid_selection_keeps_all_readable_boards() -> io::Result<()> {
    let directory = directory("bad-selection")?;
    let board = json!({"game":"1. d4 *\n","place":[],"origin":null});
    for (number, selected) in [(1, json!(4)), (2, json!("invalid"))] {
        let original = serde_json::to_vec(&json!({"boards":[board],"selected":selected}))?;
        fs::write(directory.join("boards.json"), &original)?;
        let backup = directory.join(format!("boards-recovery-{number}.json"));
        let answer = restore(&SharedGames::default(), &directory)?;
        assert_eq!(answer["boards"].as_array().unwrap().len(), 1);
        assert_eq!(answer["boards"][0]["picture"]["notation"][0]["text"], "d4");
        assert_eq!(answer["selected"], Value::Null);
        assert_eq!(
            answer["warning"],
            format!(
                "Could not restore the selected board. The original file was kept at {}. Your PGN files are untouched. You can open a board and keep playing.",
                backup.display()
            )
        );
        assert_eq!(fs::read(backup)?, original);
    }
    fs::remove_dir_all(directory)
}

#[test]
fn a_failed_backup_names_the_original_and_quit_retries_before_replacing_it() -> io::Result<()> {
    let directory = directory("failed-backup")?;
    let record = directory.join("boards.json");
    fs::create_dir(&record)?;
    let games = SharedGames::default();
    let answer = restore(&games, &directory)?;
    assert_eq!(
        answer["warning"],
        format!(
            "Could not read the board session. The original file could not be moved and is still at {}. Your PGN files are untouched. You can open a board and keep playing.",
            record.display()
        )
    );
    assert!(games.is_restored());
    ask(&games, &directory, json!({"type":"NewGame"}))?;
    assert!(
        ask(
            &games,
            &directory,
            json!({"type":"CheckpointBoards","boards":[1],"selected":1})
        )
        .is_err()
    );
    assert!(quit(&games, &directory, &[1], Some(1)).is_err());
    assert!(record.is_dir());
    assert!(!directory.join("boards-recovery-1.json").exists());
    fs::remove_dir(&record)?;
    fs::write(&record, b"original now accessible")?;
    quit(&games, &directory, &[1], Some(1))?;
    assert_eq!(
        fs::read(directory.join("boards-recovery-1.json"))?,
        b"original now accessible"
    );
    assert_eq!(restore(&SharedGames::default(), &directory)?["selected"], 1);
    fs::remove_dir_all(directory)
}

#[cfg(unix)]
#[test]
fn a_dangling_session_link_is_preserved_as_a_link() -> io::Result<()> {
    let directory = directory("dangling-link")?;
    let record = directory.join("boards.json");
    std::os::unix::fs::symlink("missing-target", &record)?;
    let backup = directory.join("boards-recovery-1.json");
    let games = SharedGames::default();
    assert_eq!(
        restore(&games, &directory)?,
        json!({"type":"RestoreBoards","boards":[],"selected":null,"warning":format!(
            "Could not read the board session. The original file was kept at {}. Your PGN files are untouched. You can open a board and keep playing.", backup.display())})
    );
    quit(&games, &directory, &[], None)?;
    assert_eq!(fs::read_link(backup)?, Path::new("missing-target"));
    assert!(!record.is_symlink());
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
