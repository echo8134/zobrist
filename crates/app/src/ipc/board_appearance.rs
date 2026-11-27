//! Board preferences shared by all games on this device.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use super::Response;
use super::local_data::{LockedDataFile, read_or_default};

/// The board preferences shown on the Settings page and saved on this device.
#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct BoardAppearance {
    piece_set: PieceSet,
    board_palette: BoardPalette,
    show_coordinates: bool,
}

impl Default for BoardAppearance {
    fn default() -> Self {
        Self {
            piece_set: PieceSet::Cburnett,
            board_palette: BoardPalette::Brown,
            show_coordinates: true,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum PieceSet {
    Cburnett,
    Chessnut,
}

#[derive(Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum BoardPalette {
    Brown,
    Green,
}

/// Requests for the appearance of every board in the app.
#[derive(Deserialize, TS)]
#[serde(tag = "type")]
pub enum AppearanceRequest {
    /// Read the saved preferences or the defaults for a new installation.
    GetBoardAppearance,
    /// Save all three preferences together.
    SetBoardAppearance {
        /// The player's choice on the Settings page.
        appearance: BoardAppearance,
    },
}

impl AppearanceRequest {
    /// Answer from the desktop app's local data directory.
    pub fn answer(self, directory: &Path) -> io::Result<Response> {
        let name = "settings.json";
        match self {
            Self::GetBoardAppearance => {
                let appearance = read_or_default(
                    &directory.join(name),
                    "the board settings",
                    "Restore the file from a backup or move it aside, then restart zobrist.",
                    |bytes| Ok(serde_json::from_slice(bytes)?),
                )?;
                Ok(Response::GetBoardAppearance { appearance })
            }
            Self::SetBoardAppearance { appearance } => {
                LockedDataFile::open(directory, name)?.write(&appearance)?;
                Ok(Response::SetBoardAppearance { appearance })
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs, process};

    use serde_json::{Value, json};

    use super::*;
    use crate::ipc::DesktopRequest;

    fn ask(request: Value, directory: &Path) -> io::Result<Value> {
        let DesktopRequest::Appearance(request) = serde_json::from_value(request)? else {
            return Err(io::Error::other("Expected a board appearance request."));
        };
        Ok(json!(request.answer(directory)?))
    }

    #[test]
    fn explains_how_to_recover_unreadable_board_settings() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-damaged-settings-{}", process::id()));
        fs::create_dir(&directory)?;
        let path = directory.join("settings.json");
        fs::write(&path, b"{")?;
        let error = ask(json!({"type":"GetBoardAppearance"}), &directory).unwrap_err();
        assert_eq!(
            error.to_string(),
            format!(
                "Could not read the board settings at {}. Restore the file from a backup or move it aside, then restart zobrist.",
                path.display()
            )
        );
        assert_eq!(fs::read(&path)?, b"{");
        fs::remove_dir_all(directory)
    }

    #[test]
    fn returns_the_defaults_then_saves_and_reopens_all_three_choices() -> io::Result<()> {
        let directory = env::temp_dir().join(format!("zobrist-board-appearance-{}", process::id()));
        let get = json!({"type":"GetBoardAppearance"});
        let defaults = json!({"pieceSet":"cburnett","boardPalette":"brown","showCoordinates":true});
        let chosen = json!({"pieceSet":"chessnut","boardPalette":"green","showCoordinates":false});
        assert_eq!(
            ask(get.clone(), &directory)?,
            json!({"type":"GetBoardAppearance","appearance":defaults})
        );
        assert_eq!(
            ask(
                json!({"type":"SetBoardAppearance","appearance":chosen}),
                &directory
            )?,
            json!({"type":"SetBoardAppearance","appearance":chosen})
        );
        assert_eq!(
            serde_json::from_slice::<Value>(&fs::read(directory.join("settings.json"))?)?,
            chosen
        );
        assert_eq!(
            ask(get.clone(), &directory)?,
            json!({"type":"GetBoardAppearance","appearance":chosen})
        );

        fs::create_dir(directory.join("settings.tmp"))?;
        assert!(
            ask(
                json!({"type":"SetBoardAppearance","appearance":defaults}),
                &directory
            )
            .is_err()
        );
        assert_eq!(
            ask(get, &directory)?,
            json!({"type":"GetBoardAppearance","appearance":chosen})
        );
        fs::remove_dir_all(directory)
    }
}
