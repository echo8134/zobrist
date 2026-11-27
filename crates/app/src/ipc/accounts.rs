//! The public Lichess username the player keeps on this device.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Response;
use super::file_collection::{publish_collection, read_list};

/// Offline operations on the local account preference.
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
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Accounts {
    lichess_username: Option<String>,
}

impl AccountRequest {
    /// Reads or writes the preference in the app's local data directory.
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
        "account list",
        "PGN files and online accounts",
        "User",
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
