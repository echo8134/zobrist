//! The public Lichess username the player keeps on this device.

use std::fs::{self, File};
use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Response;

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

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Accounts {
    lichess_username: Option<String>,
}

impl AccountRequest {
    /// Reads or writes the preference in the app's local data directory.
    pub fn answer(self, directory: &Path) -> io::Result<Response> {
        let path = directory.join("accounts.json");
        match self {
            Self::GetLichessName => {
                let name = match fs::read(&path) {
                    Ok(bytes) => serde_json::from_slice::<Accounts>(&bytes)?.lichess_username,
                    Err(error) if error.kind() == io::ErrorKind::NotFound => None,
                    Err(error) => return Err(error),
                };
                Ok(Response::GetLichessName { name })
            }
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

fn save(directory: &Path, lichess_username: Option<String>) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    // Serialize writers across windows and processes before opening their shared staging file.
    let lock = File::options()
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("accounts.lock"))?;
    lock.lock()?;
    let staged = directory.join("accounts.tmp");
    let mut file = File::create(&staged)?;
    serde_json::to_writer(&mut file, &Accounts { lichess_username })?;
    file.sync_all()?;
    drop(file);
    fs::rename(staged, directory.join("accounts.json"))
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
