//! Download public chess games from Lichess.

mod player;

pub use player::{PlayerExportError, download_player_games};

use std::fmt;
use std::io::Read as _;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use reqwest::StatusCode;
use reqwest::blocking::{Client, Response};
use reqwest::header::{ACCEPT, HeaderValue, RETRY_AFTER};
use zobrist_core::game::Game;
use zobrist_core::pgn::{self, Entry};

const MAX_EXPORT_BYTES: u64 = 1 << 20;
const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const MIN_COOLDOWN: Duration = Duration::from_secs(60);

// Hold the lock through the response body so all tabs share one request at a time.
static COOLDOWN: Mutex<Option<(Instant, Duration)>> = Mutex::new(None);

/// Why a public game could not be opened.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FetchError {
    /// The submitted link does not identify a public Lichess game.
    InvalidLink,
    /// The site returned HTTP 404.
    NotFound,
    /// The site asked this process to pause requests.
    RateLimited,
    /// The site refused the request or the connection failed.
    Unavailable,
    /// The export exceeded the maximum response size.
    TooLarge,
    /// The export did not contain exactly one complete game.
    InvalidGame,
    /// The game uses a chess variant.
    UnsupportedVariant,
    /// The move text has no final result.
    Unfinished,
}

impl fmt::Display for FetchError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidLink => "Enter a public Lichess game link.",
            Self::NotFound => "Game not found.",
            Self::RateLimited => "Too many requests. Try again later.",
            Self::Unavailable => "Could not download the game.",
            Self::TooLarge => "The game export is too large.",
            Self::InvalidGame => "The export does not contain one complete game.",
            Self::UnsupportedVariant => "Only standard chess games can be opened.",
            Self::Unfinished => "This game has no final result.",
        })
    }
}

impl std::error::Error for FetchError {}

fn game_id(link: &str) -> Result<&str, FetchError> {
    let path = link
        .strip_prefix("https://lichess.org/")
        .ok_or(FetchError::InvalidLink)?;
    let path = match path.split_once('#') {
        Some((path, fragment))
            if !fragment.is_empty() && fragment.bytes().all(|b| b.is_ascii_digit()) =>
        {
            path
        }
        Some(_) => return Err(FetchError::InvalidLink),
        None => path,
    };
    let id = path
        .strip_suffix("/white")
        .or_else(|| path.strip_suffix("/black"))
        .unwrap_or(path);
    if id.len() == 8 && id.bytes().all(|b| b.is_ascii_alphanumeric()) {
        Ok(id)
    } else {
        Err(FetchError::InvalidLink)
    }
}

/// Download the PGN export and return its single finished standard game.
pub fn fetch_game(link: &str) -> Result<Game, FetchError> {
    let id = game_id(link)?;
    let bytes = request_export(
        &format!("/game/export/{id}"),
        "application/x-chess-pgn",
        Some(REQUEST_TIMEOUT),
        |response| {
            let mut bytes = Vec::new();
            let length = response
                .take(MAX_EXPORT_BYTES + 1)
                .read_to_end(&mut bytes)
                .map_err(|_| FetchError::Unavailable)?;
            if length as u64 > MAX_EXPORT_BYTES {
                return Err(FetchError::TooLarge);
            }
            Ok(bytes)
        },
    )?;
    parse_export(&bytes)
}

// A single game has a total deadline. A player export may run longer while reads keep succeeding.
fn request_export<T, E: From<FetchError>>(
    path: &str,
    accept: &str,
    deadline: Option<Duration>,
    consume: impl FnOnce(Response) -> Result<T, E>,
) -> Result<T, E> {
    let mut cooldown = COOLDOWN.lock().map_err(|_| FetchError::Unavailable)?;
    if cooldown.is_some_and(|(since, duration)| since.elapsed() < duration) {
        return Err(FetchError::RateLimited.into());
    }

    let client = Client::builder()
        .user_agent("zobrist")
        .timeout(REQUEST_TIMEOUT)
        .redirect(reqwest::redirect::Policy::none())
        .retry(reqwest::retry::never());
    #[cfg(not(feature = "fixture"))]
    let host = "https://lichess.org";
    #[cfg(feature = "fixture")]
    let (client, host) = {
        let port = std::env::var("ZOBRIST_TEST_EXPORT_PORT")
            .ok()
            .and_then(|value| value.parse::<std::num::NonZeroU16>().ok())
            .ok_or(FetchError::Unavailable)?;
        (client.no_proxy(), format!("http://127.0.0.1:{port}"))
    };
    let request = client
        .build()
        .map_err(|_| FetchError::Unavailable)?
        .get(format!("{host}{path}"))
        .header(ACCEPT, accept);
    let request = match deadline {
        Some(timeout) => request.timeout(timeout),
        None => request,
    };
    let response = request.send().map_err(|_| FetchError::Unavailable)?;
    match response.status() {
        StatusCode::OK => consume(response),
        StatusCode::NOT_FOUND => Err(FetchError::NotFound.into()),
        StatusCode::TOO_MANY_REQUESTS => {
            *cooldown = Some((
                Instant::now(),
                retry_delay(response.headers().get(RETRY_AFTER)),
            ));
            Err(FetchError::RateLimited.into())
        }
        _ => Err(FetchError::Unavailable.into()),
    }
}

fn retry_delay(value: Option<&HeaderValue>) -> Duration {
    let seconds = value
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.parse::<u64>().ok())
        .unwrap_or(0);
    MIN_COOLDOWN.max(Duration::from_secs(seconds))
}

fn parse_export(bytes: &[u8]) -> Result<Game, FetchError> {
    let mut found = None;
    pgn::read_games(bytes, |entry| {
        found = Some(match (found.is_none(), entry) {
            (
                true,
                Entry::Game {
                    game,
                    problem: None,
                    ..
                },
            ) => Ok(game),
            _ => Err(FetchError::InvalidGame),
        });
    })
    .map_err(|_| FetchError::InvalidGame)?;
    let game = found.ok_or(FetchError::InvalidGame)??;
    if game
        .tag("Variant")
        .is_some_and(|variant| variant != "Standard")
    {
        return Err(FetchError::UnsupportedVariant);
    }
    let marker = game.result().to_string();
    // The file reader recovers games even when their move text is cut off or trails the marker.
    if bytes
        .trim_ascii_end()
        .rsplit(u8::is_ascii_whitespace)
        .next()
        != Some(marker.as_bytes())
    {
        return Err(FetchError::InvalidGame);
    }
    if game.result().is_unknown() {
        return Err(FetchError::Unfinished);
    }
    if game.tag("Result").is_some_and(|result| result != marker) {
        return Err(FetchError::InvalidGame);
    }
    Ok(*game)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn links_select_only_the_public_id() {
        for suffix in ["", "#0", "/white", "/white#123", "/black", "/black#4"] {
            assert_eq!(
                game_id(&format!("https://lichess.org/Ab12Cd34{suffix}")),
                Ok("Ab12Cd34")
            );
        }
        for link in [
            "https://example.invalid/Ab12Cd34",
            "http://lichess.org/Ab12Cd34",
            "https://lichess.org@other.invalid/Ab12Cd34",
            "https://lichess.org/Ab12Cd345678",
            "https://lichess.org/Ab12Cd3",
            "https://lichess.org/Ab12Cdé",
            "https://lichess.org/Ab12Cd34/black/white",
            "https://lichess.org/Ab12Cd34?moves=0",
            "https://lichess.org/Ab12Cd34#",
            "https://lichess.org/Ab12Cd34#1/black",
        ] {
            assert_eq!(game_id(link), Err(FetchError::InvalidLink), "{link}");
        }
    }

    #[test]
    fn wait_at_least_a_minute_and_honor_a_longer_delay() {
        assert_eq!(retry_delay(None), Duration::from_secs(60));
        assert_eq!(
            retry_delay(Some(&HeaderValue::from_static("2"))),
            Duration::from_secs(60)
        );
        assert_eq!(
            retry_delay(Some(&HeaderValue::from_static("120"))),
            Duration::from_secs(120)
        );
    }

    #[test]
    fn export_requires_one_finished_standard_game() {
        for result in ["0-1", "1-0", "1/2-1/2"] {
            let bytes =
                format!("[Variant \"Standard\"]\n[Result \"{result}\"]\n\n1. e4 e5 {result}\n");
            assert_eq!(
                parse_export(bytes.as_bytes()).unwrap().result().to_string(),
                result
            );
        }
        for (pgn, error) in [
            ("", FetchError::InvalidGame),
            ("[Result \"*\"]\n\n1. e4 *\n", FetchError::Unfinished),
            ("[Result \"1-0\"]\n\n1. e4\n", FetchError::InvalidGame),
            ("1. e4 1-0 e5", FetchError::InvalidGame),
            ("[Result \"0-1\"]\n\n1. e4 1-0\n", FetchError::InvalidGame),
            (
                "[Variant \"Atomic\"]\n\n1. e4 1-0\n",
                FetchError::UnsupportedVariant,
            ),
            (
                "[White \"Aster\"]\n\n1. e4 e5 2. Ke8 1-0\n",
                FetchError::InvalidGame,
            ),
            ("1. e4 1-0\n\n1. d4 0-1\n", FetchError::InvalidGame),
        ] {
            assert_eq!(parse_export(pgn.as_bytes()), Err(error), "{pgn}");
        }
    }
}
