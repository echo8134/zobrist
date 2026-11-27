//! Exercise the CLI against a local game export and record each request.

use std::io::{self, BufRead, BufReader, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Command, Output};
use std::thread;

const LINK: &str = "https://lichess.org/Ab12Cd34";
const PGN: &str =
    "[White \"Aster\"]\n[Black \"Birch\"]\n[Result \"0-1\"]\n\n1. f3 e5 2. g4 Qh4# 0-1\n";

fn fetch(link: &str, response: String) -> io::Result<(Output, Vec<Vec<String>>)> {
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
            stream.write_all(response.as_bytes())?;
        }
        Ok::<_, io::Error>(requests)
    });
    let output = Command::new(env!("CARGO_BIN_EXE_zobrist"))
        .args(["pgn", "fetch", link])
        .env("ZOBRIST_TEST_EXPORT_PORT", address.port().to_string())
        .output()?;
    // An empty connection ends the fixture after the CLI has made its last request.
    drop(TcpStream::connect(address)?);
    let requests = fixture
        .join()
        .map_err(|_| io::Error::other("export fixture panicked"))??;
    Ok((output, requests))
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
