//! Exercise the CLI against a local game export and record each request.

use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::{self, Command, Output};
use std::thread;

const LINK: &str = "https://lichess.org/Ab12Cd34";
const PGN: &str =
    "[White \"Aster\"]\n[Black \"Birch\"]\n[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1\n";

fn fetch(link: &str, response: String) -> io::Result<(Output, Vec<Vec<String>>)> {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zobrist"));
    command.args(["pgn", "fetch", link]);
    with_fixture(command, move |stream| stream.write_all(response.as_bytes()))
}

fn with_fixture(
    mut command: Command,
    respond: impl Fn(&mut TcpStream) -> io::Result<()> + Send + 'static,
) -> io::Result<(Output, Vec<Vec<String>>)> {
    let listener = TcpListener::bind(("127.0.0.1", 0))?;
    let address = listener.local_addr()?;
    let fixture = thread::spawn(move || {
        let mut requests = Vec::new();
        for stream in listener.incoming() {
            let mut stream = stream?;
            let headers: Vec<_> = BufReader::new(&stream)
                .lines()
                .map_while(Result::ok)
                .take_while(|line| !line.is_empty())
                .collect();
            if headers.is_empty() {
                break;
            }
            requests.push(headers);
            respond(&mut stream)?;
        }
        Ok::<_, io::Error>(requests)
    });
    let output = command
        .env("ZOBRIST_TEST_EXPORT_PORT", address.port().to_string())
        .output();
    // An empty connection ends the fixture after the CLI has made its last request.
    drop(TcpStream::connect(address)?);
    let requests = fixture
        .join()
        .map_err(|_| io::Error::other("export fixture panicked"))??;
    Ok((output?, requests))
}

#[test]
fn fetch_writes_the_finished_game_in_pgn() {
    let (output, requests) = fetch(
        LINK,
        format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\n{PGN}",
            PGN.len()
        ),
    )
    .unwrap();
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stderr, b"");
    assert_eq!(output.stdout, PGN.as_bytes());
    assert_eq!(requests.len(), 1);
    let headers = &requests[0];
    assert_eq!(headers[0], "GET /game/export/Ab12Cd34 HTTP/1.1");
    assert!(
        headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("Accept: application/x-chess-pgn"))
    );
    assert!(
        headers
            .iter()
            .any(|header| header.eq_ignore_ascii_case("User-Agent: zobrist"))
    );
}

#[test]
fn invalid_link_never_contacts_the_site() {
    let (output, requests) = fetch("https://example.invalid/Ab12Cd34", String::new()).unwrap();
    assert_eq!(requests.len(), 0);
    assert_eq!(output.status.code(), Some(2));
    assert_eq!(output.stdout, b"");
    assert_eq!(
        output.stderr,
        b"zobrist: Enter a public Lichess game link.\n"
    );
}

#[test]
fn failed_exports_write_no_pgn() {
    for (response, expected) in [
        (
            "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "Game not found.",
        ),
        (
            "HTTP/1.1 429 Too Many Requests\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "Too many requests. Try again later.",
        ),
        (
            "HTTP/1.1 302 Found\r\nLocation: /another-game\r\nContent-Length: 0\r\n\r\n".to_owned(),
            "Could not download the game.",
        ),
        (
            format!("HTTP/1.1 200 OK\r\nContent-Length: 1000\r\n\r\n{PGN}"),
            "Could not download the game.",
        ),
        (
            format!(
                "HTTP/1.1 200 OK\r\nContent-Length: 1048577\r\n\r\n{}",
                " ".repeat(1_048_577)
            ),
            "The game export is too large.",
        ),
    ] {
        let (output, requests) = fetch(LINK, response).unwrap();
        assert_eq!(requests.len(), 1);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, format!("zobrist: {expected}\n").as_bytes());
    }
}

const ORIGINAL: &[u8] = b"[Event \"Original\"]\r\n\r\n1. e4 {Keep my note} *";
const JANUARY_31: &str = r#"{"pgn":"[Event \"January 31\"]\n[Result \"1-0\"]\n\n1. e4 1-0"}"#;
const JANUARY_1: &str = r#"{"pgn":"[Event \"January 1\"]\n[Result \"0-1\"]\n\n1. d4 0-1"}"#;

fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("zobrist-download-{name}-{}.pgn", process::id()))
}

fn download(name: &str, from: &str, to: &str, target: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_zobrist"));
    command
        .args(["pgn", "download-player", name, "--from", from, "--to", to])
        .arg(target);
    command
}

#[test]
fn download_validates_player_and_calendar_dates_before_contacting_lichess() -> io::Result<()> {
    let target = test_path("invalid");
    for (name, from, to, reason) in [
        (
            "RookExample",
            "2026-02-01",
            "2026-01-31",
            "From must be on or before To.",
        ),
        (
            "RookExample",
            "2026-02-29",
            "2026-03-01",
            "Enter a valid date in YYYY-MM-DD format.",
        ),
        (
            "Rook/Example",
            "2024-02-29",
            "2024-02-29",
            "Enter a public Lichess username.",
        ),
    ] {
        let (output, requests) = with_fixture(download(name, from, to, &target), |_| Ok(()))?;
        assert_eq!(requests.len(), 0);
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stdout, b"");
        assert_eq!(output.stderr, format!("zobrist: {reason}\n").as_bytes());
        assert!(!target.try_exists()?);
    }
    Ok(())
}

#[test]
fn empty_and_failed_player_exports_preserve_existing_and_absent_files() -> io::Result<()> {
    let target = test_path("refused");
    let open_comment =
        r#"{"pgn":"[Event \"Open\"]\n[Variant \"Crazyhouse\"]\n\n1. N@e4 1-0 {open 1-0"}"#;
    for exists in [false, true] {
        if exists {
            fs::write(&target, ORIGINAL)?;
        }
        for (response, code) in [
            ("HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n".to_owned(), 0),
            (
                "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\n\r\n".to_owned(),
                2,
            ),
            (
                format!("HTTP/1.1 200 OK\r\nContent-Length: 500\r\n\r\n{JANUARY_31}\n"),
                2,
            ),
            (
                format!("HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{JANUARY_31}\n{{}}\n"),
                2,
            ),
            (
                format!(
                    "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{JANUARY_31}\n{open_comment}\n"
                ),
                2,
            ),
        ] {
            let missing_player = response.starts_with("HTTP/1.1 404");
            let command = download("RookExample", "2024-02-29", "2024-02-29", &target);
            let (output, requests) =
                with_fixture(command, move |stream| stream.write_all(response.as_bytes()))?;
            assert_eq!(requests.len(), 1);
            let request = &requests[0][0];
            assert!(request.starts_with("GET /api/games/user/RookExample?"));
            assert!(request.contains("since=1709164800000"));
            assert!(request.contains("until=1709251200000"));
            assert!(request.contains("pgnInJson=true"));
            assert!(
                requests[0]
                    .iter()
                    .any(|header| header.eq_ignore_ascii_case("Accept: application/x-ndjson"))
            );
            assert_eq!(output.status.code(), Some(code));
            if code == 0 {
                assert_eq!(output.stdout, b"games added: 0\n");
                assert_eq!(output.stderr, b"");
            } else {
                assert_eq!(output.stdout, b"");
                assert!(output.stderr.starts_with(b"zobrist: "));
                if missing_player {
                    assert_eq!(output.stderr, b"zobrist: Player not found.\n");
                }
            }
            if exists {
                assert_eq!(fs::read(&target)?, ORIGINAL);
            } else {
                assert!(!target.try_exists()?);
            }
        }
    }
    let command = download("RookExample", "2026-01-01", "2026-01-31", &target);
    let (retry, _) = with_fixture(command, |stream| {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{JANUARY_31}\n{JANUARY_1}\n"
        )
    })?;
    assert_eq!(retry.status.code(), Some(0));
    assert_eq!(retry.stdout, b"games added: 2\n");
    let contents = fs::read_to_string(&target)?;
    assert!(contents.as_bytes().starts_with(ORIGINAL));
    assert_eq!(contents.matches("[Event").count(), 3);
    fs::remove_file(target)
}

#[test]
fn a_game_saved_during_a_download_precedes_the_downloaded_games() -> io::Result<()> {
    let target = test_path("concurrent");
    let source = test_path("local-game");
    fs::write(&target, ORIGINAL)?;
    fs::write(&source, "[Event \"Local save\"]\n\n1. f3 *\n")?;
    let local_target = target.clone();
    let local_source = source.clone();
    let command = download("RookExample", "2026-01-01", "2026-01-31", &target);
    let (output, _) = with_fixture(command, move |stream| {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nConnection: close\r\n\r\n{JANUARY_31}\n"
        )?;
        let saved = Command::new(env!("CARGO_BIN_EXE_zobrist"))
            .args(["pgn", "add"])
            .arg(&local_target)
            .arg(&local_source)
            .output()?;
        assert_eq!(saved.status.code(), Some(0));
        assert_eq!(saved.stdout, b"games added: 1\n");
        writeln!(stream, "{JANUARY_1}")
    })?;
    assert_eq!(output.status.code(), Some(0));
    assert_eq!(output.stdout, b"games added: 2\n");
    assert_eq!(output.stderr, b"");
    let listed = Command::new(env!("CARGO_BIN_EXE_zobrist"))
        .args(["pgn", "list"])
        .arg(&target)
        .output()?;
    assert_eq!(listed.status.code(), Some(0));
    assert_eq!(listed.stdout, b"1\t?\t?\t*\t?\tOriginal\n2\t?\t?\t*\t?\tLocal save\n3\t?\t?\t1-0\t?\tJanuary 31\n4\t?\t?\t0-1\t?\tJanuary 1\n");
    assert!(fs::read(&target)?.starts_with(ORIGINAL));
    fs::remove_file(source)?;
    fs::remove_file(target)
}
