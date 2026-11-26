//! Listing games and adding them to a PGN file through the CLI.

use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};
use std::{env, fs, io};

const CLEAN: &str = "tests/fixtures/clean.pgn";
const DAMAGED: &str = "tests/fixtures/damaged.pgn";
const PROBLEM: &[u8] =
    b"game 2 (Gamma - Delta): illegal move \"Ke3\" at White's move 2, 2 moves kept\n";
const CLEAN_LIST: &[u8] =
    b"1\tAlpha\tBeta\t1-0\t2024.01.01\tExample\n2\tGamma\tDelta\t0-1\t2024.01.01\tExample\n";

fn pgn(command: &str, paths: &[&Path]) -> io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_zobrist"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(["pgn", command])
        .args(paths)
        .output()
}

fn temporary_path(name: &str) -> PathBuf {
    env::temp_dir().join(format!("zobrist-pgn-{name}-{}", process::id()))
}

fn assert_output(output: &Output, status: i32, stdout: &[u8], stderr: &[u8]) {
    assert_eq!(output.status.code(), Some(status));
    assert_eq!(output.stdout, stdout);
    assert_eq!(output.stderr, stderr);
}

#[test]
fn lists_clean_and_damaged_games_in_file_order() -> io::Result<()> {
    assert_output(&pgn("list", &[Path::new(CLEAN)])?, 0, CLEAN_LIST, b"");
    assert_output(
        &pgn("list", &[Path::new(DAMAGED)])?,
        1,
        b"1\tAlpha\tBeta\t1-0\t?\tExample\n2\tGamma\tDelta\t*\t?\tExample\n3\tEpsilon\tZeta\t0-1\t?\tExample\n",
        PROBLEM,
    );
    Ok(())
}

#[test]
fn lists_missing_tags_and_keeps_tag_values_in_one_field() -> io::Result<()> {
    let source = temporary_path("tag-values");
    fs::write(&source, b"1. e4 *\n\n[White \"First\tLast\"]\n[Event \"two\\\nlines\"]\n[Result \"1-0\"]\n\n1. d4 1/2-1/2\n")?;
    assert_output(
        &pgn("list", &[&source])?,
        0,
        b"1\t?\t?\t*\t?\t?\n2\tFirst Last\t?\t1/2-1/2\t?\ttwo\\ lines\n",
        b"",
    );
    fs::remove_file(source)
}

#[test]
fn creates_a_pgn_and_adds_games_again_without_changing_old_bytes() -> io::Result<()> {
    let target = temporary_path("add");
    let source = Path::new(CLEAN);
    let expected = pgn("write", &[source])?.stdout;
    assert_output(
        &pgn("add", &[&target, source])?,
        0,
        b"games added: 2\n",
        b"",
    );
    assert_eq!(fs::read(&target)?, expected);
    assert_output(
        &pgn("add", &[&target, source])?,
        0,
        b"games added: 2\n",
        b"",
    );
    assert_eq!(
        fs::read(&target)?,
        [&expected[..], b"\n", &expected].concat()
    );
    assert_output(
        &pgn("list", &[&target])?,
        0,
        &[CLEAN_LIST, b"3\tAlpha\tBeta\t1-0\t2024.01.01\tExample\n4\tGamma\tDelta\t0-1\t2024.01.01\tExample\n"].concat(),
        b"",
    );
    fs::remove_file(target)
}

#[test]
fn adds_after_an_unterminated_game_and_keeps_games_with_problems() -> io::Result<()> {
    let target = temporary_path("open");
    fs::write(&target, b"1. e4 *")?;
    assert_output(
        &pgn("add", &[&target, Path::new(CLEAN)])?,
        0,
        b"games added: 2\n",
        b"",
    );
    assert!(fs::read(&target)?.starts_with(b"1. e4 *\n\n["));
    assert_output(
        &pgn("check", &[&target])?,
        0,
        b"games: 3\nmain line moves: 12\nvariations: 1\ncomments: 1\nproblems: 0\n",
        b"",
    );
    fs::remove_file(&target)?;
    let source = Path::new(DAMAGED);
    assert_output(
        &pgn("add", &[&target, source])?,
        1,
        b"games added: 3\n",
        PROBLEM,
    );
    assert_eq!(fs::read(&target)?, pgn("write", &[source])?.stdout);
    fs::remove_file(target)
}

#[test]
fn file_failures_name_the_path_and_preserve_the_target() -> io::Result<()> {
    let target = temporary_path("failure");
    let absent = Path::new("missing-source.pgn");
    for old in [None, Some(b"1. e4 *")] {
        if let Some(bytes) = old {
            fs::write(&target, bytes)?;
        }
        let output = pgn("add", &[&target, absent])?;
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert!(
            output
                .stderr
                .starts_with(b"zobrist: cannot read missing-source.pgn:")
        );
        if let Some(bytes) = old {
            assert_eq!(fs::read(&target)?, bytes);
        } else {
            assert!(!target.try_exists()?);
        }
    }
    let missing_parent = target.join("mine.pgn");
    let output = pgn("add", &[&missing_parent, Path::new(CLEAN)])?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(String::from_utf8_lossy(&output.stderr).contains(&*missing_parent.to_string_lossy()));
    fs::remove_file(target)
}
