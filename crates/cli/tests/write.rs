//! The stdout, stderr, and exit status of `zobrist pgn write`.

use std::env;
use std::fs;
use std::io;
use std::path::Path;
use std::process::{self, Command, Output};

fn write_command(file: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zobrist"));
    command
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["pgn", "write"])
        .arg(file);
    command
}

fn write_input(name: &str, bytes: &[u8]) -> io::Result<Output> {
    let file = env::temp_dir().join(format!("zobrist-write-{name}-{}.pgn", process::id()));
    fs::write(&file, bytes)?;
    let output = write_command(&file).output()?;
    fs::remove_file(&file)?;
    Ok(output)
}

#[test]
fn writes_the_clean_file_with_numeric_annotation_symbols() {
    let output = write_command(Path::new("tests/fixtures/clean.pgn"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stderr, b"");
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        r#"[Event "Example"]
[Site "?"]
[Date "2024.01.01"]
[Round "1"]
[White "Alpha"]
[Black "Beta"]
[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 $4 {Overlooks the mate.} (3... g6 4. Qf3) 4. Qxf7# 1-0

[Event "Example"]
[Site "?"]
[Date "2024.01.01"]
[Round "2"]
[White "Gamma"]
[Black "Delta"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1
"#
    );
}

#[test]
fn writes_every_kept_game_and_prints_its_problem_on_stderr() {
    let output = write_command(Path::new("tests/fixtures/damaged.pgn"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(
        output.stderr,
        b"game 2 (Gamma - Delta): illegal move \"Ke3\" at White's move 2, 2 moves kept\n"
    );
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        r#"[Event "Example"]
[White "Alpha"]
[Black "Beta"]
[Result "1-0"]

1. e4 e5 2. Qh5 Nc6 3. Bc4 Nf6 4. Qxf7# 1-0

[Event "Example"]
[White "Gamma"]
[Black "Delta"]
[Result "*"]

1. e4 e5 *

[Event "Example"]
[White "Epsilon"]
[Black "Zeta"]
[Result "0-1"]

1. f3 e5 2. g4 Qh4# 0-1
"#
    );
}

#[test]
fn leaves_no_gap_for_stray_text() {
    let output = write_input("stray", b"1. e4 *\n\nstray words\n\n1. d4 *\n").unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stdout, b"1. e4 *\n\n1. d4 *\n");
    assert_eq!(
        output.stderr,
        b"after game 1: text that is not part of a game\n"
    );
}

#[test]
fn an_empty_file_writes_nothing() {
    let output = write_input("empty", b"").unwrap();

    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"");
    assert_eq!(output.stderr, b"");
}

#[test]
fn names_an_unreadable_input() {
    let output = write_command(Path::new("no-such-file.pgn"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(
        output
            .stderr
            .starts_with(b"zobrist: cannot read no-such-file.pgn")
    );
}

#[cfg(target_os = "linux")]
#[test]
fn reports_a_failed_output() {
    let sink = fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let output = write_command(Path::new("tests/fixtures/clean.pgn"))
        .stdout(sink)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert!(output.stderr.starts_with(b"zobrist: cannot write output:"));
}

#[test]
fn keeps_completed_games_when_a_comment_exceeds_the_utf8_limit() {
    let mut bytes = b"1. d4 *\n\n1. e4 {".to_vec();
    bytes.extend(vec![0xff; (1 << 19) + 1]);
    bytes.extend_from_slice(b"} *\n");
    let output = write_input("long-comment", &bytes).unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"1. d4 *\n");
    assert_eq!(
        output.stderr,
        b"zobrist: cannot write output: PGN comment exceeds 1 MiB in UTF-8\n"
    );
}

#[test]
fn reports_a_tag_that_exceeds_the_supported_length_in_utf8() {
    let mut bytes = b"[Label \"".to_vec();
    bytes.extend(vec![0xfc; 40_000]);
    bytes.extend_from_slice(b"\"]\n\n1. e4 *\n");
    let output = write_input("long-tag", &bytes).unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"zobrist: cannot write output: PGN tag Label exceeds the supported UTF-8 tag length\n"
    );
}

#[test]
fn reports_a_tag_with_an_embedded_line_break() {
    let output = write_input("multiline-tag", b"[Label \"a\\\nb\"]\n\n1. e4 *\n").unwrap();

    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"zobrist: cannot write output: PGN tag Label contains a line break\n"
    );
}
