//! `zobrist pgn check`, run the way the maintainer runs it.

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{self, Command, Output};

fn zobrist_pgn_check(file: &Path) -> io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_zobrist"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["pgn", "check"])
        .arg(file)
        .output()
}

fn report_of_illegal_king_moves(games: u32) -> String {
    let problem_lines: Vec<String> = (1..=games)
        .map(|game| {
            format!("game {game} (? - ?): illegal move \"Ke2\" at White's move 1, 0 moves kept\n")
        })
        .collect();
    format!(
        "games: {games}\nmain line moves: 0\nvariations: 0\ncomments: 0\nproblems: {games}\n{}",
        problem_lines.concat()
    )
}

#[test]
fn reports_a_clean_file_and_exits_with_status_0() {
    let output = zobrist_pgn_check(Path::new("tests/fixtures/clean.pgn")).unwrap();

    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "games: 2\nmain line moves: 11\nvariations: 1\ncomments: 1\nproblems: 0\n"
    );
    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(output.status.code(), Some(0));
}

#[test]
fn reports_a_damaged_game_and_exits_with_status_1() {
    let output = zobrist_pgn_check(Path::new("tests/fixtures/damaged.pgn")).unwrap();

    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "games: 3\nmain line moves: 13\nvariations: 0\ncomments: 0\nproblems: 1\n\
         game 2 (Gamma - Delta): illegal move \"Ke3\" at White's move 2, 2 moves kept\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn reports_a_missing_file_and_exits_with_status_2() {
    let output = zobrist_pgn_check(Path::new("no-such-file.pgn")).unwrap();

    assert_eq!(String::from_utf8(output.stdout).unwrap(), "");
    assert!(
        String::from_utf8(output.stderr)
            .unwrap()
            .starts_with("zobrist: cannot read no-such-file.pgn")
    );
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn reports_stray_text_and_a_game_with_one_kept_move_and_a_missing_player() {
    let file = env::temp_dir().join(format!("zobrist-check-{}.pgn", process::id()));
    fs::write(
        &file,
        "[White \"A\"]\n\n1. e4 Ke7 *\n\nstray words\n\n[Black \"B\"]\n\n1. e4 e5 *\n",
    )
    .unwrap();
    let output = zobrist_pgn_check(&file).unwrap();
    fs::remove_file(&file).unwrap();

    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "games: 2\nmain line moves: 3\nvariations: 0\ncomments: 0\nproblems: 2\n\
         game 1 (A - ?): illegal move \"Ke7\" at Black's move 1, 1 move kept\n\
         after game 1: text that is not part of a game\n"
    );
    assert_eq!(output.status.code(), Some(1));
}

/// `ulimit -d` is a shell builtin, and the memory accounting it relies on is Linux's.
#[cfg(target_os = "linux")]
#[test]
fn prints_every_problem_line_of_a_file_with_more_problems_than_it_keeps_in_memory() {
    const DATA_LIMIT_KIBIBYTES: u32 = 16_384;

    let file = env::temp_dir().join(format!("zobrist-many-problems-{}.pgn", process::id()));
    fs::write(&file, "Ke2 *\n\n".repeat(300_000)).unwrap();
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ulimit -c 0 && ulimit -d {DATA_LIMIT_KIBIBYTES} && exec \"$0\" pgn check \"$1\""
        ))
        .arg(env!("CARGO_BIN_EXE_zobrist"))
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(&file).unwrap();

    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        report_of_illegal_king_moves(300_000)
    );
}

#[test]
fn prints_every_problem_line_of_a_file_with_10001_problems() {
    let file = env::temp_dir().join(format!("zobrist-10001-problems-{}.pgn", process::id()));
    fs::write(&file, "Ke2 *\n\n".repeat(10_001)).unwrap();
    let output = zobrist_pgn_check(&file).unwrap();
    fs::remove_file(&file).unwrap();

    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        report_of_illegal_king_moves(10_001)
    );
    assert_eq!(output.status.code(), Some(1));
}

/// A pipe cannot be read twice, and Windows has no `/dev/stdin`.
#[cfg(unix)]
#[test]
fn prints_every_problem_line_of_a_pipe_with_more_problems_than_it_keeps_for_a_regular_file() {
    use std::io::Write as _;
    use std::process::Stdio;
    use std::thread;

    let mut child = Command::new(env!("CARGO_BIN_EXE_zobrist"))
        .args(["pgn", "check", "/dev/stdin"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let writer = thread::spawn(move || input.write_all("Ke2 *\n\n".repeat(10_001).as_bytes()));
    let output = child.wait_with_output().unwrap();
    writer.join().unwrap().unwrap();

    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        report_of_illegal_king_moves(10_001)
    );
    assert_eq!(output.status.code(), Some(1));
}

/// `ulimit -d` is a shell builtin, and the memory accounting it relies on is Linux's.
#[cfg(target_os = "linux")]
#[test]
fn reads_a_file_of_games_without_move_text_in_flat_memory() {
    const DATA_LIMIT_KIBIBYTES: u32 = 16_384;

    let file = env::temp_dir().join(format!("zobrist-no-move-text-{}.pgn", process::id()));
    fs::write(&file, "[Event \"x\"]\n[White \"y\"]\n\n".repeat(200_000)).unwrap();
    let output = Command::new("sh")
        .arg("-c")
        .arg(format!(
            "ulimit -c 0 && ulimit -d {DATA_LIMIT_KIBIBYTES} && exec \"$0\" pgn check \"$1\""
        ))
        .arg(env!("CARGO_BIN_EXE_zobrist"))
        .arg(&file)
        .output()
        .unwrap();
    fs::remove_file(&file).unwrap();

    assert_eq!(String::from_utf8(output.stderr).unwrap(), "");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "games: 200000\nmain line moves: 0\nvariations: 0\ncomments: 0\nproblems: 0\n"
    );
    assert_eq!(output.status.code(), Some(0));
}
