//! The public Lichess username the player keeps on this device.

use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Response;
use super::file_collection::{history_at, list_recovery, publish_collection, read_list};

/// Operations on the saved public Lichess username and its games.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum AccountRequest {
    /// Read the saved name.
    GetLichessName,
    /// Save the nonempty name after trimming it.
    SaveLichessName {
        /// The text entered on User.
        name: String,
    },
    /// Forget the saved name.
    RemoveLichessName,
    /// Append public games for the saved name and keep the previous PGN.
    DownloadLichessGames {
        /// The saved name shown when the player chose Download.
        name: String,
        /// First UTC date, inclusive.
        from: String,
        /// Last UTC date, inclusive.
        to: String,
        /// The local destination from the PGN picker.
        path: PathBuf,
    },
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Accounts {
    lichess_username: Option<String>,
}

impl AccountRequest {
    /// Uses the app data directory for the saved name and PGN history.
    pub fn answer(self, directory: &Path) -> io::Result<Response> {
        match self {
            Self::GetLichessName => Ok(Response::GetLichessName {
                name: read(&directory.join("accounts.json"))?.lichess_username,
            }),
            Self::SaveLichessName { name } => {
                let name = name.trim();
                if name.is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Enter a Lichess username.",
                    ));
                }
                save(directory, Some(name.to_owned()))?;
                Ok(Response::SaveLichessName {
                    name: name.to_owned(),
                })
            }
            Self::DownloadLichessGames {
                name,
                from,
                to,
                path,
            } => {
                if read(&directory.join("accounts.json"))?
                    .lichess_username
                    .as_deref()
                    != Some(&name)
                {
                    return Err(io::Error::other(
                        "The saved Lichess username changed. Reopen User to download games.",
                    ));
                }
                let file_name = path
                    .file_name()
                    .ok_or_else(|| io::Error::other("Choose a PGN file."))?;
                let (added, warning) = zobrist_online::download_player_games_with_history(
                    &name,
                    &from,
                    &to,
                    &path,
                    Some(history_at(directory)),
                )
                .map_err(io::Error::other)?;
                Ok(Response::DownloadLichessGames {
                    added,
                    file: file_name.to_string_lossy().into_owned(),
                    warning,
                })
            }
            Self::RemoveLichessName => {
                save(directory, None)?;
                Ok(Response::RemoveLichessName)
            }
        }
    }
}

fn read(path: &Path) -> io::Result<Accounts> {
    read_list(
        path,
        "the account list",
        &list_recovery("PGN files and online accounts", "User"),
        |bytes| Ok(serde_json::from_slice(bytes)?),
    )
}

fn save(directory: &Path, lichess_username: Option<String>) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    // Serialize writers across windows and processes before opening their shared staging file.
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("accounts.lock"))?;
    lock.lock()?;
    let path = directory.join("accounts.json");
    let mut accounts = read(&path)?;
    accounts.lichess_username = lichess_username;
    publish_collection(&path, &accounts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saves_any_nonempty_trimmed_text_and_keeps_it_after_a_failed_write() -> io::Result<()> {
        let directory =
            std::env::temp_dir().join(format!("zobrist-accounts-{}", std::process::id()));
        fs::create_dir(&directory)?;
        AccountRequest::SaveLichessName {
            name: "  Knight / ♘  ".to_owned(),
        }
        .answer(&directory)?;
        let path = directory.join("accounts.json");
        let original = fs::read(&path)?;
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&original)?,
            serde_json::json!({"lichessUsername":"Knight / ♘"})
        );
        let empty = AccountRequest::SaveLichessName {
            name: "  ".to_owned(),
        }
        .answer(&directory);
        assert!(matches!(empty, Err(error) if error.to_string() == "Enter a Lichess username."));
        fs::create_dir(directory.join("accounts.tmp"))?;
        assert!(
            AccountRequest::RemoveLichessName
                .answer(&directory)
                .is_err()
        );
        assert_eq!(fs::read(path)?, original);
        fs::remove_dir_all(directory)
    }
}
