use std::fmt;
use std::io::{self, BufRead, BufReader, Read as _};
use std::path::Path;

use time::Date;
use time::format_description::well_known::Iso8601;
use zobrist_store::{PgnBatch, PgnHistory};

use crate::{Endpoint, FetchError, request};

const MAX_RECORD_BYTES: u64 = 4 << 20;
const MAX_BATCH_BYTES: u64 = 1 << 30;

/// Why a player's games could not be added.
#[derive(Debug)]
pub enum PlayerExportError {
    /// The name does not have the form of a public Lichess username.
    InvalidUsername,
    /// A date was invalid or did not use the YYYY-MM-DD format.
    InvalidDate,
    /// The start date was after the end date.
    ReversedDates,
    /// Lichess refused the request or the connection failed.
    Request(FetchError),
    /// An entry was incomplete or did not contain a PGN string.
    InvalidRecord,
    /// A single JSON record exceeded the size limit.
    RecordTooLarge,
    /// The complete response exceeded the size limit.
    BatchTooLarge,
    /// Staging or publishing the PGN games failed.
    Storage(io::Error),
}

impl fmt::Display for PlayerExportError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidUsername => formatter.write_str("Enter a public Lichess username."),
            Self::InvalidDate => formatter.write_str("Enter a valid date in YYYY-MM-DD format."),
            Self::ReversedDates => formatter.write_str("From must be on or before To."),
            Self::Request(FetchError::NotFound) => formatter.write_str("Player not found."),
            Self::Request(FetchError::Unavailable) => {
                formatter.write_str("Could not download the player's games.")
            }
            Self::Request(error) => error.fmt(formatter),
            Self::InvalidRecord => formatter
                .write_str("The player export contains an incomplete or invalid game record."),
            Self::RecordTooLarge => {
                formatter.write_str("One game record in the player export is too large.")
            }
            Self::BatchTooLarge => {
                formatter.write_str("The player export is too large. Choose a shorter date range.")
            }
            Self::Storage(error) => write!(
                formatter,
                "Could not add the games to the PGN file: {error}"
            ),
        }
    }
}

impl std::error::Error for PlayerExportError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Request(error) => Some(error),
            Self::Storage(error) => Some(error),
            Self::InvalidUsername
            | Self::InvalidDate
            | Self::ReversedDates
            | Self::InvalidRecord
            | Self::RecordTooLarge
            | Self::BatchTooLarge => None,
        }
    }
}

impl From<FetchError> for PlayerExportError {
    fn from(error: FetchError) -> Self {
        Self::Request(error)
    }
}

/// Add the player's public games started on either selected UTC date or between them.
/// Returns the number appended after the entire export has arrived. No games means no write.
pub fn download_player_games(
    username: &str,
    from: &str,
    to: &str,
    destination: &Path,
) -> Result<u64, PlayerExportError> {
    download_player_games_with_history(username, from, to, destination, None)
        .map(|(added, _)| added)
}

/// Download games and optionally capture the previous PGN at publication.
/// Returns the number appended and any warning from recording history.
pub fn download_player_games_with_history(
    username: &str,
    from: &str,
    to: &str,
    destination: &Path,
    history: Option<PgnHistory<'_>>,
) -> Result<(u64, Option<String>), PlayerExportError> {
    let path = export_path(username, from, to)?;
    let batch = request(
        Endpoint::Export(&path),
        "application/x-ndjson",
        None,
        |response| read_batch(BufReader::new(response), MAX_RECORD_BYTES, MAX_BATCH_BYTES),
    )?;
    match batch {
        Some(batch) => batch
            .append_to(destination, history)
            .map_err(PlayerExportError::Storage),
        None => Ok((0, None)),
    }
}

fn export_path(username: &str, from: &str, to: &str) -> Result<String, PlayerExportError> {
    // Existing usernames include forms that Lichess no longer allows at sign-up.
    if !(2..=30).contains(&username.len())
        || !username
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(PlayerExportError::InvalidUsername);
    }
    let from = parse_date(from)?;
    let to = parse_date(to)?;
    if from > to {
        return Err(PlayerExportError::ReversedDates);
    }
    let midnight = |date: Date| date.midnight().as_utc().unix_timestamp() * 1_000;
    let since = midnight(from);
    let until = midnight(to) + 86_400_000;
    Ok(format!(
        "/api/games/user/{username}?since={since}&until={until}&pgnInJson=true"
    ))
}

fn parse_date(text: &str) -> Result<Date, PlayerExportError> {
    if text.len() != 10
        || !text.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 => byte == b'-',
            _ => byte.is_ascii_digit(),
        })
    {
        return Err(PlayerExportError::InvalidDate);
    }
    Date::parse(text, &Iso8601::DATE).map_err(|_| PlayerExportError::InvalidDate)
}

fn read_batch(
    mut source: impl BufRead,
    record_limit: u64,
    batch_limit: u64,
) -> Result<Option<PgnBatch>, PlayerExportError> {
    let mut batch = None;
    let mut line = Vec::new();
    let mut total = 0;
    loop {
        line.clear();
        let bytes = (&mut source)
            .take(record_limit + 1)
            .read_until(b'\n', &mut line)
            .map_err(|_| FetchError::Unavailable)? as u64;
        if bytes == 0 {
            return Ok(batch);
        }
        if bytes > record_limit {
            return Err(PlayerExportError::RecordTooLarge);
        }
        total += bytes;
        if total > batch_limit {
            return Err(PlayerExportError::BatchTooLarge);
        }
        if !line.ends_with(b"\n") {
            return Err(PlayerExportError::InvalidRecord);
        }
        if line.trim_ascii().is_empty() {
            continue;
        }
        let record: serde_json::Value =
            serde_json::from_slice(&line).map_err(|_| PlayerExportError::InvalidRecord)?;
        let pgn = record
            .get("pgn")
            .and_then(serde_json::Value::as_str)
            .ok_or(PlayerExportError::InvalidRecord)?;
        let batch = match &mut batch {
            Some(existing) => existing,
            None => batch.insert(PgnBatch::new().map_err(PlayerExportError::Storage)?),
        };
        batch.push(pgn).map_err(PlayerExportError::Storage)?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_leap_day_starts_at_midnight_and_ends_at_the_next_midnight() {
        assert_eq!(
            export_path("A_-1", "2024-02-29", "2024-02-29").unwrap(),
            "/api/games/user/A_-1?since=1709164800000&until=1709251200000&pgnInJson=true"
        );
        for invalid in [
            "2026-02-29",
            "2024-2-29",
            " 024-02-29",
            "+024-02-29",
            "2024-02-0１",
        ] {
            assert!(matches!(
                export_path("RookExample", invalid, "2026-12-31"),
                Err(PlayerExportError::InvalidDate)
            ));
        }
        assert!(matches!(
            export_path("RookExample", "2026-02-01", "2026-01-31"),
            Err(PlayerExportError::ReversedDates)
        ));
    }

    #[test]
    fn rejects_names_that_are_not_public_handles() {
        for name in [
            "a",
            "name/else",
            "Rook?since=0",
            "Müller",
            "abcdefghijklmnopqrstuvwxyz12345",
        ] {
            assert!(matches!(
                export_path(name, "2024-02-29", "2024-02-29"),
                Err(PlayerExportError::InvalidUsername)
            ));
        }
    }

    #[test]
    fn framing_and_limits_apply_even_without_a_final_newline() {
        assert!(matches!(read_batch(&b"\r\n\n"[..], 2, 3), Ok(None)));
        assert!(matches!(
            read_batch(&b"123456"[..], 5, 20),
            Err(PlayerExportError::RecordTooLarge)
        ));
        assert!(matches!(
            read_batch(&b"\n\n\n"[..], 5, 2),
            Err(PlayerExportError::BatchTooLarge)
        ));
        for bytes in [&b"{}\n"[..], b"{\"pgn\":13}\n", b"{\"pgn\":\"1. e4 1-0\"}"] {
            assert!(matches!(
                read_batch(bytes, 30, 90),
                Err(PlayerExportError::InvalidRecord)
            ));
        }
    }
}
