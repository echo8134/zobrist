//! Listing games, adding them to a PGN file, and replacing one through the CLI.

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
fn adding_games_refuses_an_open_comment_and_preserves_the_target() -> io::Result<()> {
    let target = temporary_path("open-comment");
    let old = b"1. d4 {unfinished";
    fs::write(&target, old)?;
    let reason = format!(
        "zobrist: cannot add games to {}: the added games would not read back \
         as separate games after the existing games\n",
        target.display()
    );
    assert_output(
        &pgn("add", &[&target, Path::new(CLEAN)])?,
        2,
        b"",
        reason.as_bytes(),
    );
    assert_eq!(fs::read(&target)?, old);
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

const NEW_GAME: &[u8] = b"[White \"New\"]\n[Black \"Game\"]\n\n1. d4 d5 *\n";

#[test]
fn replaces_a_game_and_puts_it_back_without_changing_the_other_game() -> io::Result<()> {
    let target = temporary_path("replace");
    let new = temporary_path("replace-new");
    let first = temporary_path("replace-first");
    let clean = fs::read(CLEAN)?;
    fs::write(&target, &clean)?;
    fs::write(&new, NEW_GAME)?;
    let first_nine_lines = clean.split_inclusive(|byte| *byte == b'\n').take(9);
    fs::write(&first, first_nine_lines.collect::<Vec<_>>().concat())?;

    assert_output(
        &pgn("replace", &[&target, Path::new("1"), &new])?,
        0,
        b"game replaced: 1\n",
        b"",
    );
    assert_output(
        &pgn("list", &[&target])?,
        0,
        b"1\tNew\tGame\t*\t?\t?\n2\tGamma\tDelta\t0-1\t2024.01.01\tExample\n",
        b"",
    );
    assert_eq!(
        fs::read(&target)?,
        [&NEW_GAME[..NEW_GAME.len() - 1], &clean[192..]].concat()
    );

    assert_output(
        &pgn("replace", &[&target, Path::new("1"), &first])?,
        0,
        b"game replaced: 1\n",
        b"",
    );
    assert_eq!(
        fs::read(&target)?,
        pgn("write", &[Path::new(CLEAN)])?.stdout
    );
    fs::remove_file(target)?;
    fs::remove_file(new)?;
    fs::remove_file(first)
}

#[test]
fn replaces_a_game_that_has_a_problem() -> io::Result<()> {
    let target = temporary_path("replace-damaged");
    let new = temporary_path("replace-damaged-new");
    fs::copy(DAMAGED, &target)?;
    fs::write(&new, NEW_GAME)?;

    assert_output(
        &pgn("replace", &[&target, Path::new("2"), &new])?,
        0,
        b"game replaced: 2\n",
        b"",
    );
    assert_output(
        &pgn("check", &[&target])?,
        0,
        b"games: 3\nmain line moves: 13\nvariations: 0\ncomments: 0\nproblems: 0\n",
        b"",
    );
    fs::remove_file(target)?;
    fs::remove_file(new)
}

#[test]
fn a_refused_replacement_says_why_and_keeps_every_byte_of_the_target() -> io::Result<()> {
    let target = temporary_path("refuse");
    let new = temporary_path("refuse-new");
    let empty = temporary_path("refuse-empty");
    let damaged = temporary_path("refuse-damaged");
    let clean = fs::read(CLEAN)?;
    fs::write(&target, &clean)?;
    fs::write(&new, NEW_GAME)?;
    fs::write(&empty, b"")?;
    fs::write(&damaged, b"[White \"Gamma\"]\n\n1. e4 e5 2. Ke3 *\n")?;
    let refusals: [(&[&Path], String); 4] = [
        (
            &[&target, Path::new("3"), &new],
            format!(
                "zobrist: cannot replace a game of {}: there is no game 3, the file holds 2\n",
                target.display()
            ),
        ),
        (
            &[&target, Path::new("1"), &empty],
            format!(
                "zobrist: cannot replace a game with {}: it holds no game\n",
                empty.display()
            ),
        ),
        (
            &[&target, Path::new("1"), Path::new(CLEAN)],
            format!(
                "zobrist: cannot replace a game with {CLEAN}: \
                 it holds 2 games, and a replacement is one game\n"
            ),
        ),
        (
            &[&target, Path::new("1"), &damaged],
            format!(
                "game 1 (Gamma - ?): illegal move \"Ke3\" at White's move 2, 2 moves kept\n\
                 zobrist: cannot replace a game with {}: it has a problem\n",
                damaged.display()
            ),
        ),
    ];
    for (arguments, stderr) in refusals {
        assert_output(&pgn("replace", arguments)?, 2, b"", stderr.as_bytes());
        assert_eq!(fs::read(&target)?, clean);
    }

    let output = pgn("replace", &[&target, Path::new("0"), &new])?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(
        output
            .stderr
            .starts_with(b"error: invalid value '0' for '<NUMBER>': ")
    );
    assert_eq!(fs::read(&target)?, clean);

    let absent = Path::new("missing-source.pgn");
    let output = pgn("replace", &[&target, Path::new("1"), absent])?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(
        output
            .stderr
            .starts_with(b"zobrist: cannot read missing-source.pgn:")
    );
    assert_eq!(fs::read(&target)?, clean);

    fs::remove_file(&new)?;
    fs::remove_file(empty)?;
    fs::remove_file(damaged)?;
    fs::remove_file(target)
}

#[test]
fn a_target_that_cannot_be_read_or_written_is_refused() -> io::Result<()> {
    let target = temporary_path("unwritable");
    let new = temporary_path("unwritable-new");
    fs::write(&new, NEW_GAME)?;
    let cannot_replace = format!("zobrist: cannot replace a game of {}: ", target.display());

    let output = pgn("replace", &[&target, Path::new("1"), &new])?;
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert!(output.stderr.starts_with(cannot_replace.as_bytes()));
    assert!(!target.try_exists()?);

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        fs::copy(CLEAN, &target)?;
        fs::set_permissions(&target, fs::Permissions::from_mode(0o444))?;
        let output = pgn("replace", &[&target, Path::new("1"), &new])?;
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert!(output.stderr.starts_with(cannot_replace.as_bytes()));
        assert_eq!(fs::read(&target)?, fs::read(CLEAN)?);
        fs::remove_file(&target)?;
    }
    fs::remove_file(new)
}
