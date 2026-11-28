//! The report from database creation and the PGN from database export.

use std::env;
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};
use std::{fs, io};

struct Directory(PathBuf);

impl Directory {
    fn new(name: &str) -> io::Result<Self> {
        let path = env::temp_dir().join(format!("zobrist-database-{name}-{}", process::id()));
        fs::create_dir(&path)?;
        Ok(Self(path))
    }
}

impl Drop for Directory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

fn command() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zobrist"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
    command
}

fn create(input: &Path, database: &Path) -> io::Result<Output> {
    command()
        .args(["database", "create"])
        .arg(input)
        .arg(database)
        .output()
}

fn export(database: &Path) -> io::Result<Output> {
    command()
        .args(["database", "export"])
        .arg(database)
        .output()
}

#[test]
fn info_prints_the_verified_game_count() {
    let directory = Directory::new("info").unwrap();
    let empty = directory.0.join("empty.pgn");
    fs::write(&empty, "").unwrap();
    for (input, count) in [
        (Path::new("tests/fixtures/clean.pgn"), "games: 2\n"),
        (&empty, "games: 0\n"),
    ] {
        let database = directory
            .0
            .join(input.file_name().unwrap())
            .with_extension("zobrist");
        assert!(create(input, &database).unwrap().status.success());
        let output = command()
            .args(["database", "info"])
            .arg(database)
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(0));
        assert_eq!(output.stdout, count.as_bytes());
        assert_eq!(output.stderr, b"");
    }
    let refused = command()
        .args(["database", "info", "tests/fixtures/clean.pgn"])
        .output()
        .unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(refused.stdout, b"");
    assert_eq!(
        refused.stderr,
        b"zobrist: cannot read tests/fixtures/clean.pgn: not a zobrist database\n"
    );
}

#[test]
fn imports_every_game_and_exports_the_same_bytes_as_pgn_write() {
    let directory = Directory::new("round-trip").unwrap();
    for (name, status, report) in [
        (
            "clean",
            0,
            "games: 2\nmain line moves: 11\nvariations: 1\ncomments: 1\nproblems: 0\n",
        ),
        (
            "damaged",
            1,
            "games: 3\nmain line moves: 13\nvariations: 0\ncomments: 0\nproblems: 1\n\
             game 2 (Gamma - Delta): illegal move \"Ke3\" at White's move 2, 2 moves kept\n",
        ),
    ] {
        let input = PathBuf::from(format!("tests/fixtures/{name}.pgn"));
        let database = directory.0.join(format!("{name}.zobrist"));
        let created = create(&input, &database).unwrap();
        assert_eq!(created.status.code(), Some(status));
        assert_eq!(created.stdout, report.as_bytes());
        assert_eq!(created.stderr, b"");

        let written = command()
            .args(["pgn", "write"])
            .arg(input)
            .output()
            .unwrap();
        assert_eq!(written.status.code(), Some(status));
        let exported = export(&database).unwrap();
        assert_eq!(exported.status.code(), Some(0));
        assert_eq!(exported.stderr, b"");
        assert_eq!(exported.stdout, written.stdout);
    }
}

#[test]
fn creates_identical_bytes_and_leaves_an_existing_target_intact() {
    let directory = Directory::new("no-replace").unwrap();
    let input = Path::new("tests/fixtures/clean.pgn");
    let first = directory.0.join("first.zobrist");
    let second = directory.0.join("second.zobrist");
    assert_eq!(create(input, &first).unwrap().status.code(), Some(0));
    assert_eq!(create(input, &second).unwrap().status.code(), Some(0));
    let expected = fs::read(first).unwrap();
    assert_eq!(fs::read(&second).unwrap(), expected);

    let refused = create(Path::new("tests/fixtures/damaged.pgn"), &second).unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(refused.stdout, b"");
    assert!(
        String::from_utf8(refused.stderr)
            .unwrap()
            .starts_with(&format!("zobrist: cannot create {}:", second.display()))
    );
    assert_eq!(fs::read(second).unwrap(), expected);
}

#[test]
fn a_truncated_or_changed_database_prints_no_games() {
    let directory = Directory::new("damage").unwrap();
    let database = directory.0.join("clean.zobrist");
    assert_eq!(
        create(Path::new("tests/fixtures/clean.pgn"), &database)
            .unwrap()
            .status
            .code(),
        Some(0)
    );
    let original = fs::read(database).unwrap();
    let cut = &original[..original.len() - 1];
    let changed: Vec<_> = original
        .iter()
        .map(|&byte| if byte == b'a' { b'b' } else { byte })
        .collect();
    for (name, bytes) in [("cut", cut), ("changed", &changed[..])] {
        let path = directory.0.join(name);
        fs::write(&path, bytes).unwrap();
        let refused = export(&path).unwrap();
        assert_eq!(refused.status.code(), Some(2));
        assert_eq!(refused.stdout, b"");
        assert!(
            String::from_utf8(refused.stderr)
                .unwrap()
                .starts_with(&format!("zobrist: cannot read {}:", path.display()))
        );
    }

    let refused = export(Path::new("tests/fixtures/clean.pgn")).unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(refused.stdout, b"");
    assert_eq!(
        refused.stderr,
        b"zobrist: cannot read tests/fixtures/clean.pgn: not a zobrist database\n"
    );
}

#[test]
fn an_empty_file_creates_an_empty_database_and_an_unreadable_input_creates_none() {
    let directory = Directory::new("empty").unwrap();
    let input = directory.0.join("empty.pgn");
    let database = directory.0.join("empty.zobrist");
    let refused = create(&input, &database).unwrap();
    assert_eq!(refused.status.code(), Some(2));
    assert_eq!(refused.stdout, b"");
    assert!(
        String::from_utf8(refused.stderr)
            .unwrap()
            .starts_with(&format!("zobrist: cannot read {}:", input.display()))
    );
    assert!(!database.exists());

    fs::write(&input, "").unwrap();
    let created = create(&input, &database).unwrap();
    assert_eq!(created.status.code(), Some(0));
    assert_eq!(
        created.stdout,
        b"games: 0\nmain line moves: 0\nvariations: 0\ncomments: 0\nproblems: 0\n"
    );
    assert_eq!(created.stderr, b"");
    let exported = export(&database).unwrap();
    assert_eq!(exported.status.code(), Some(0));
    assert_eq!(exported.stdout, b"");
    assert_eq!(exported.stderr, b"");
}

#[cfg(unix)]
#[test]
fn reads_the_input_once_and_reports_stray_text_in_order() {
    use std::io::Write as _;
    use std::process::Stdio;

    let directory = Directory::new("pipe").unwrap();
    let database = directory.0.join("pipe.zobrist");
    let mut child = command()
        .args(["database", "create", "/dev/stdin"])
        .arg(&database)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(b"1. e4 Ke7 *\n\nstray words\n\n1. d4 *\n")
        .unwrap();

    let output = child.wait_with_output().unwrap();
    assert_eq!(output.status.code(), Some(1));
    assert_eq!(output.stderr, b"");
    assert_eq!(
        output.stdout,
        b"games: 2\nmain line moves: 2\nvariations: 0\ncomments: 0\nproblems: 2\n\
        game 1 (? - ?): illegal move \"Ke7\" at Black's move 1, 1 move kept\n\
        after game 1: text that is not part of a game\n"
    );
    assert_eq!(export(&database).unwrap().stdout, b"1. e4 *\n\n1. d4 *\n");
    assert_eq!(fs::read_dir(&directory.0).unwrap().count(), 1);
}
