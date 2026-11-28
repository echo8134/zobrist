//! Everything that crosses between the view and Rust.
//!
//! The view sends a [`DesktopRequest`] and gets back the [`Response`] case with the same name. The
//! TypeScript types of the view are generated from these two types.

mod accounts;
mod analysis;
mod board_appearance;
mod database_collection;
mod engines;
mod file_collection;
mod game_file;
mod long_tasks;
mod pgn_file;
mod session;
mod tablebase;

pub use accounts::AccountRequest;
pub use analysis::{AnalysisRequest, AnalysisSnapshot, VariationAnswer};
pub use board_appearance::{AppearanceRequest, BoardAppearance};
pub use database_collection::{DatabaseEntry, DatabaseRequest};
pub use engines::{EngineAction, EngineEntry};
pub use file_collection::{
    AddedPgnPath, CollectionEntry, CollectionRequest, EarlierState, EntryKind, FileCollection,
    PickerChoice,
};
pub use game_file::{
    DeletePgnGameAnswer, GameFile, GameFileRefusal, ListedPgnGame, OpenGameAnswer, SaveGameAnswer,
};
use game_file::{GameSource, OpenGame, SaveHistory, SavedState};
pub use long_tasks::RunningWrite;
pub use pgn_file::{GameRow, PgnFile, PgnFileAnswer, RefusalReason};
pub use session::{BoardPicture, SessionRequest};
pub use tablebase::{TablebaseAnswer, TablebaseMove};

use std::collections::BTreeMap;
use std::io;
use std::path::PathBuf;
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};
use shakmaty::{
    Chess, Color, EnPassantMode, KnownOutcome, Outcome, Position, Role, Square, fen::Fen,
};
use tauri::async_runtime::{Mutex, RwLock, spawn_blocking};
use ts_rs::TS;
use zobrist_core::comment::MoveReadings;
use zobrist_core::editor::{self, GameEditor, Place, PlacedStep, board_target};
use zobrist_core::game::{Game, GameBuilder, MainLine, Move};
use zobrist_core::pgn::{self, Entry};
use zobrist_core::shapes;

mod drawing;
use drawing::BoardShapes;
pub use drawing::ShapeEdit;

/// The request sent to the desktop command.
#[derive(Deserialize, TS)]
#[serde(untagged)]
#[ts(export)]
pub enum DesktopRequest {
    /// Requests answered by the games and PGN editor.
    Editor(Request),
    /// Requests to restore or save the open boards.
    Session(SessionRequest),
    /// Requests about the engine analysing an open game.
    Analysis(AnalysisRequest),
    /// Requests about the player's collection or the system picker.
    Collection(CollectionRequest),
    /// Requests about the name saved on this device.
    Account(AccountRequest),
    /// Read or save the board preferences for this device.
    Appearance(AppearanceRequest),
    /// Requests about native databases registered on this device.
    Databases(DatabaseRequest),
}

/// What the view can ask for.
#[derive(Clone, Deserialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum Request {
    /// The version of zobrist.
    Version,
    /// Open a game at the starting position.
    NewGame,
    /// Describe the position being set up.
    DescribePosition {
        /// Text from the FEN field or the edited board fields.
        input: PositionInput,
    },
    /// Start the open game again from a FEN position.
    SetStartPosition {
        /// The open game's number.
        game: usize,
        /// The requested position. An invalid FEN leaves the game as it was.
        #[serde(with = "start_position")]
        #[ts(as = "String")]
        fen: Option<Chess>,
    },
    /// Open the first game in PGN text.
    OpenPgnText {
        /// The PGN to read.
        text: String,
    },
    /// Open a finished game from its public link.
    OpenGameLink {
        /// The link entered by the player.
        link: String,
    },
    /// Look up the board position the player selected.
    LookupTablebase {
        /// The open game on the board.
        game: usize,
        /// The position revision the board displayed at the click.
        revision: usize,
    },
    /// Get the current game as PGN text.
    GamePgn {
        /// The open game's number.
        game: usize,
    },
    /// Set or clear one field of the game data.
    SetTag {
        /// The number from the new-game answer.
        game: usize,
        /// The field to edit.
        tag: EditableTag,
        /// The field's value. Empty text removes the tag.
        text: String,
    },
    /// Set the game's result tag and move-text marker together.
    SetResult {
        /// The number from the new-game answer.
        game: usize,
        /// The result to record.
        result: GameResult,
    },
    /// Play a board move in an open game.
    PlayMove {
        /// The number from the new-game answer.
        game: usize,
        /// The square the piece moves from.
        #[serde(with = "square")]
        #[ts(as = "String")]
        from: Square,
        /// The square the piece moves to.
        #[serde(with = "square")]
        #[ts(as = "String")]
        to: Square,
        /// The new piece when a pawn promotes.
        promotion: Option<Promotion>,
    },
    /// Show the position reached by the sequence of choices.
    GoTo {
        /// The number from the new-game answer.
        game: usize,
        /// One choice per ply, starting from the initial position.
        place: Vec<usize>,
    },
    /// Replace the comment at the current place.
    SetComment {
        /// The number from the new-game answer.
        game: usize,
        /// The new text. Empty text leaves just the shapes.
        text: String,
    },
    /// Replace the colored squares and arrows at the current place.
    SetShapes(#[ts(as = "drawing::ShapeEditFields")] ShapeEdit),
    /// Replace the symbol that annotates the current move.
    SetMoveSymbol {
        /// The number from the new-game answer.
        game: usize,
        /// The move symbol to keep, or none to remove it.
        symbol: Option<MoveSymbol>,
    },
    /// Replace the symbol that evaluates the current position.
    SetPositionSymbol {
        /// The number from the new-game answer.
        game: usize,
        /// The position symbol to keep, or none to remove it.
        symbol: Option<PositionSymbol>,
    },
    /// Take back the last change to the game.
    Undo {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Restore the last change taken back.
    Redo {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Promote the current variation one level.
    PromoteVariation {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Delete the current variation.
    DeleteVariation {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Delete the moves after the current place.
    DeleteRest {
        /// The number from the new-game answer.
        game: usize,
    },
    /// Close an open game.
    CloseGame {
        /// The number from the new-game answer.
        game: usize,
    },
    /// List the games of a PGN file.
    OpenPgnFile {
        /// The path as it was typed.
        path: String,
    },
    /// Create a new, empty PGN file where nothing is yet.
    CreatePgnFile {
        /// The path as it was typed.
        path: String,
    },
    /// Delete the confirmed row from the listed version of its file.
    DeletePgnGame(ListedPgnGame),
    /// Open one game of a PGN file as an open game of the app.
    OpenGame {
        /// The path of the file.
        path: String,
        /// The number of the game in the file, counting from 1.
        #[ts(type = "number")]
        number: u64,
    },
    /// Save an open game into its place in its file.
    SaveGame {
        /// The open game's number.
        game: usize,
    },
    /// Save an open game as the last game of a file.
    SaveGameAs {
        /// The open game's number.
        game: usize,
        /// The path of the file. Rust creates the file when nothing is there.
        path: String,
    },
}

/// Text or board controls supplied by the player setting up a position.
#[derive(Clone, Deserialize, TS)]
#[serde(tag = "type")]
pub enum PositionInput {
    /// The contents of the FEN field.
    Fen {
        /// The text to read.
        fen: String,
    },
    /// The current board and its controls.
    Fields {
        /// The edited values.
        fields: PositionFields,
    },
}

/// The fields the player can edit on the board or beside it.
#[expect(
    clippy::struct_excessive_bools,
    reason = "The board has four independent castling checkboxes."
)]
#[derive(Clone, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct PositionFields {
    pieces: String,
    turn: Side,
    white_short: bool,
    white_long: bool,
    black_short: bool,
    black_long: bool,
    en_passant: Option<String>,
}

/// A complete description of the position being set up.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct PositionDraft {
    fen: String,
    fields: PositionFields,
}

impl PositionInput {
    fn describe(self) -> Option<PositionDraft> {
        use zobrist_core::position::PositionDraft as Draft;

        let draft = match self {
            Self::Fen { fen } => Draft::from_fen(&fen),
            Self::Fields { fields } => Draft::from_fields(
                &fields.pieces,
                match fields.turn {
                    Side::White => Color::White,
                    Side::Black => Color::Black,
                },
                [
                    fields.white_short,
                    fields.white_long,
                    fields.black_short,
                    fields.black_long,
                ],
                fields.en_passant.as_deref(),
            ),
        }?;
        let [white_short, white_long, black_short, black_long] = draft.castling;
        Some(PositionDraft {
            fen: draft.fen,
            fields: PositionFields {
                pieces: draft.pieces,
                turn: draft.turn.into(),
                white_short,
                white_long,
                black_short,
                black_long,
                en_passant: draft.en_passant.map(|square| square.to_string()),
            },
        })
    }
}

mod start_position {
    use serde::{Deserialize, Deserializer};
    use shakmaty::{CastlingMode, Chess, fen::Fen};

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Option<Chess>, D::Error> {
        let text = String::deserialize(deserializer)?;
        Ok(Fen::from_ascii(text.as_bytes())
            .ok()
            .and_then(|fen| fen.into_position(CastlingMode::Standard).ok()))
    }
}

mod square {
    use serde::{Deserialize, Deserializer, de};
    use shakmaty::Square;

    pub(super) fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Square, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(de::Error::custom)
    }
}

/// The piece a pawn becomes on promotion.
#[derive(Clone, Copy, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
pub enum Promotion {
    /// Promote to a queen.
    Queen,
    /// Promote to a rook.
    Rook,
    /// Promote to a bishop.
    Bishop,
    /// Promote to a knight.
    Knight,
}

impl From<Promotion> for Role {
    fn from(promotion: Promotion) -> Self {
        match promotion {
            Promotion::Queen => Self::Queen,
            Promotion::Rook => Self::Rook,
            Promotion::Bishop => Self::Bishop,
            Promotion::Knight => Self::Knight,
        }
    }
}

/// A PGN tag the view can edit directly.
#[derive(Clone, Copy, Deserialize, TS)]
pub enum EditableTag {
    /// Name of the event.
    Event,
    /// Place where the game was played.
    Site,
    /// Date of play.
    Date,
    /// Round of the event.
    Round,
    /// Player of the white pieces.
    White,
    /// Player of the black pieces.
    Black,
}

impl From<EditableTag> for editor::EditableTag {
    fn from(tag: EditableTag) -> Self {
        match tag {
            EditableTag::Event => Self::Event,
            EditableTag::Site => Self::Site,
            EditableTag::Date => Self::Date,
            EditableTag::Round => Self::Round,
            EditableTag::White => Self::White,
            EditableTag::Black => Self::Black,
        }
    }
}

/// The recorded outcome of the game.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum GameResult {
    /// White won.
    WhiteWin,
    /// Black won.
    BlackWin,
    /// The game was drawn.
    Draw,
    /// The outcome is unknown.
    Unknown,
}

impl From<GameResult> for Outcome {
    fn from(result: GameResult) -> Self {
        match result {
            GameResult::WhiteWin => Self::Known(KnownOutcome::Decisive {
                winner: Color::White,
            }),
            GameResult::BlackWin => Self::Known(KnownOutcome::Decisive {
                winner: Color::Black,
            }),
            GameResult::Draw => Self::Known(KnownOutcome::Draw),
            GameResult::Unknown => Self::Unknown,
        }
    }
}

impl From<Outcome> for GameResult {
    fn from(result: Outcome) -> Self {
        match result {
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::White,
            }) => Self::WhiteWin,
            Outcome::Known(KnownOutcome::Decisive {
                winner: Color::Black,
            }) => Self::BlackWin,
            Outcome::Known(KnownOutcome::Draw) => Self::Draw,
            Outcome::Unknown => Self::Unknown,
        }
    }
}

// The same spelling crosses the wire in a request, the current selection, and the notation.
macro_rules! wire_symbols {
    ($(#[$doc:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$doc])*
        #[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize, TS)]
        pub enum $name {
            $(
                #[doc = $text]
                #[serde(rename = $text)]
                $variant,
            )+
        }

        impl $name {
            fn text(self) -> &'static str {
                match self { $(Self::$variant => $text,)+ }
            }
        }

        impl From<editor::$name> for $name {
            fn from(symbol: editor::$name) -> Self {
                match symbol { $(editor::$name::$variant => Self::$variant,)+ }
            }
        }

        impl From<$name> for editor::$name {
            fn from(symbol: $name) -> Self {
                match symbol { $($name::$variant => Self::$variant,)+ }
            }
        }
    };
}

wire_symbols! {
    /// The player's assessment of a move.
    MoveSymbol {
        Good => "!", Mistake => "?", Brilliant => "!!", Blunder => "??",
        Interesting => "!?", Dubious => "?!",
    }
}

wire_symbols! {
    /// The player's assessment of a position.
    PositionSymbol {
        Equal => "=", Unclear => "∞", WhiteSlight => "⩲", BlackSlight => "⩱",
        WhiteClear => "±", BlackClear => "∓", WhiteDecisive => "+-", BlackDecisive => "-+",
    }
}

/// What Rust answers. Each case has the name of its request.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
#[ts(export)]
pub enum Response {
    /// Whether the player needs to wait for a background task before quitting.
    PrepareToClose {
        /// The sentence to show while waiting.
        message: Option<&'static str>,
    },
    /// The boards in tab order after restoring the session.
    RestoreBoards {
        /// The games and their pictures at the saved places.
        boards: Vec<BoardPicture>,
        /// The selected runtime game number, when a board was selected.
        selected: Option<usize>,
        /// Where to find the original session if any part could not be restored.
        #[serde(skip_serializing_if = "Option::is_none")]
        #[ts(optional)]
        warning: Option<String>,
    },
    /// The session was written and the window can close.
    QuitWithBoards,
    /// The result of adding the engine row selected on the board.
    AddEngineVariation {
        /// Whether anything changed in the game.
        answer: VariationAnswer,
    },
    /// The state of one search at one position.
    Analysis {
        /// Tagged with the game, revision and search generation.
        snapshot: AnalysisSnapshot,
    },
    /// The public Lichess name saved on this device.
    GetLichessName {
        /// Absent when the player has not saved a name.
        name: Option<String>,
    },
    /// The name after trimming and saving.
    SaveLichessName {
        /// The exact name now on disk.
        name: String,
    },
    /// The name has been removed from this device.
    RemoveLichessName,
    /// The version of zobrist.
    Version {
        /// The version of this crate.
        version: String,
    },
    /// The newly opened game.
    NewGame {
        /// The game's number for this run of the app.
        game: usize,
        /// The picture at the starting position.
        picture: GamePicture,
    },
    /// The position being set up, including its FEN.
    DescribePosition {
        /// Absent when the input does not have the form of a position.
        draft: Option<PositionDraft>,
    },
    /// The position after accepting or refusing the FEN.
    SetStartPosition {
        /// Absent when the game is no longer open.
        change: Option<PositionChange>,
    },
    /// The first game read from the supplied text.
    OpenPgnText {
        /// Absent when the text contains no game.
        opened: Option<OpenedGame>,
    },
    /// The fetched game or the reason it could not be opened.
    OpenGameLink {
        /// The outcome of the fetch.
        answer: OpenGameLinkAnswer,
    },
    /// The outcomes for the position and its moves, or a refusal.
    LookupTablebase {
        /// The answer to this board's lookup.
        answer: TablebaseAnswer,
    },
    /// The PGN text or the reason it could not be written.
    GamePgn(PgnExport),
    /// The game after the editor accepts or refuses the tag edit.
    SetTag {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting the result.
    SetResult {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the editor accepts or refuses the move.
    PlayMove {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the editor accepts or refuses the place.
    GoTo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing a comment.
    SetComment {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing the shapes.
    SetShapes {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing a move symbol.
    SetMoveSymbol {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after setting or refusing a position symbol.
    SetPositionSymbol {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the undo request.
    Undo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the redo request.
    Redo {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to promote the variation.
    PromoteVariation {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to delete the variation.
    DeleteVariation {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game after the request to delete the remaining moves.
    DeleteRest {
        /// Absent when the game is no longer open.
        picture: Option<GamePicture>,
    },
    /// The game is closed.
    CloseGame,
    /// The board preferences for this device.
    GetBoardAppearance {
        /// The saved preferences, or the defaults on first use.
        appearance: BoardAppearance,
    },
    /// The board preferences after a successful save.
    SetBoardAppearance {
        /// The preferences to apply to every board.
        appearance: BoardAppearance,
    },
    /// The executable path selected on this computer.
    PickEnginePath {
        /// Absent after cancellation.
        path: Option<std::path::PathBuf>,
    },
    /// The engines registered on this computer.
    EngineInventory {
        /// The default comes first.
        entries: Vec<EngineEntry>,
    },
    /// The path the player chose, or nothing after cancellation.
    PickPgnPath {
        /// The absolute path returned by the system.
        path: Option<std::path::PathBuf>,
    },
    /// The chosen native database, or nothing after cancellation.
    PickDatabasePath {
        /// The absolute path returned by the system.
        path: Option<std::path::PathBuf>,
    },
    /// The chosen destination for a new database, or nothing after cancellation.
    PickNewDatabasePath {
        /// The absolute path returned by the system.
        path: Option<std::path::PathBuf>,
    },
    /// The native databases registered on this device.
    ListDatabases {
        /// Names and locations of the registered collections.
        entries: Vec<DatabaseEntry>,
    },
    /// The updated list and count from the selected database.
    AddDatabase {
        /// Names and locations after registration.
        entries: Vec<DatabaseEntry>,
        /// The verified number of games in the database.
        #[ts(type = "number")]
        games: u64,
    },
    /// The published and registered PGN import.
    ImportDatabase {
        /// The registered collections after the import.
        entries: Vec<DatabaseEntry>,
        /// Where the new collection was saved.
        path: std::path::PathBuf,
        /// Number of games written, including games with recoverable problems.
        #[ts(type = "number")]
        games: u64,
        /// Total number of problems in the source.
        problems: usize,
        /// The first diagnostic in source order, when present.
        #[serde(rename = "firstProblem")]
        first_problem: Option<String>,
    },
    /// The result of opening the selected native database.
    InspectDatabase {
        /// The verified number of games in the database.
        #[ts(type = "number")]
        games: u64,
    },
    /// The remaining databases after forgetting an entry.
    RemoveDatabase {
        /// Names and locations after removal.
        entries: Vec<DatabaseEntry>,
    },
    /// The saved references.
    GetFileCollection {
        /// The list or the available recovery action.
        collection: FileCollection,
    },
    /// The result of the player's request to recover the collection list.
    RecoverFileCollection {
        /// The recovered entries or the reason recovery could not complete.
        collection: FileCollection,
    },
    /// The files the player recently opened.
    GetRecentFiles {
        /// Up to ten PGN files, newest first.
        entries: Vec<CollectionEntry>,
    },
    /// The PGN selected on Home or Files, or the refusal.
    OpenListedPgnFile {
        /// The file with its games or why it could not be opened.
        answer: PgnFileAnswer,
    },
    /// Earlier states of this PGN, most recent first.
    ListPgnStates {
        /// A damaged state still has its number.
        states: Vec<EarlierState>,
    },
    /// The current file as seen when the player chose a state.
    PreparePgnRestore {
        /// Identifies the bytes the player is about to replace.
        stamp: String,
    },
    /// The chosen state is now on disk.
    RestorePgnState,
    /// The collection and the selected file or folder after applying the picker result.
    AddPgnPath {
        /// The new selection or the reason the file operation was refused.
        answer: AddedPgnPath,
    },
    /// The immediate PGN files and subfolders of a folder.
    ReadPgnFolder {
        /// Folders first, then files, in name order.
        entries: Vec<CollectionEntry>,
    },
    /// The file with its games, or the refusal.
    OpenPgnFile {
        /// What the page Files shows next.
        answer: PgnFileAnswer,
    },
    /// The new file without games, or the refusal.
    CreatePgnFile {
        /// What the page Files shows next.
        answer: PgnFileAnswer,
    },
    /// Updated tab bindings after publication, or the reason deletion was refused.
    DeletePgnGame {
        /// The result of confirming deletion.
        answer: DeletePgnGameAnswer,
    },
    /// The open game, or the refusal.
    OpenGame {
        /// The game and its picture, or why there is none.
        answer: OpenGameAnswer,
    },
    /// The game after it was saved into its place, or the refusal.
    SaveGame {
        /// Absent when the game is no longer open.
        answer: Option<SaveGameAnswer>,
    },
    /// The game after it was saved as the last game of a file, or the refusal.
    SaveGameAs {
        /// Absent when the game is no longer open.
        answer: Option<SaveGameAnswer>,
    },
}

/// The complete picture after a request to set the start position.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct PositionChange {
    picture: GamePicture,
    refused: bool,
}

/// A newly opened game and the reader's first problem, if any.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
pub struct OpenedGame {
    game: usize,
    picture: GamePicture,
    problem: Option<String>,
}

/// The result of opening a public game link.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(tag = "type")]
pub enum OpenGameLinkAnswer {
    /// The game was opened without a local file.
    Opened {
        /// The game's number for this run of the app.
        game: usize,
        /// The picture at the starting position.
        picture: Box<GamePicture>,
    },
    /// The link did not produce a finished game.
    Refused {
        /// The reason to show the player.
        reason: String,
    },
}

/// The result of writing an open game to PGN.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(untagged)]
pub enum PgnExport {
    /// The game as PGN, including its final newline.
    Text {
        /// Absent when the game is no longer open.
        text: Option<String>,
    },
    /// The game could not be written as PGN.
    Error {
        /// The writer's explanation.
        error: String,
    },
}

fn pgn_text(game: &Game) -> io::Result<String> {
    let mut bytes = Vec::new();
    pgn::write_game(game, &mut bytes)?;
    String::from_utf8(bytes).map_err(io::Error::other)
}

impl From<Option<&OpenGame>> for PgnExport {
    fn from(board: Option<&OpenGame>) -> Self {
        let Some(board) = board else {
            return Self::Text { text: None };
        };
        match pgn_text(board.editor.game()) {
            Ok(text) => Self::Text { text: Some(text) },
            Err(error) => Self::Error {
                error: error.to_string(),
            },
        }
    }
}

/// Everything the view draws for one game, taken from the same editor position.
#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct GamePicture {
    revision: usize,
    tablebase_unavailable_reason: Option<String>,
    tags: Vec<Tag>,
    result: GameResult,
    fen: String,
    turn: Side,
    in_check: bool,
    legal_moves: Vec<LegalMove>,
    #[serde(flatten)]
    current: CurrentPlace,
    #[serde(flatten)]
    shapes: BoardShapes,
    comment: String,
    can_undo: bool,
    can_redo: bool,
    start_comments: Vec<String>,
    place: Vec<usize>,
    notation: Vec<NotationMove>,
    #[serde(flatten)]
    saved: SavedState,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct Tag {
    name: String,
    value: String,
}

#[derive(Debug, Default, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct CurrentPlace {
    in_variation: bool,
    has_next_move: bool,
    last_move: Option<MoveSquares>,
    move_symbol: Option<MoveSymbol>,
    position_symbol: Option<PositionSymbol>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    evaluation: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    clock: Option<RecordedClock>,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct RecordedClock {
    side: Side,
    time: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, TS)]
#[serde(rename_all = "lowercase")]
enum Side {
    White,
    Black,
}

impl From<Color> for Side {
    fn from(color: Color) -> Self {
        match color {
            Color::White => Self::White,
            Color::Black => Self::Black,
        }
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct MoveSquares {
    from: String,
    to: String,
}

impl MoveSquares {
    fn from_move(chess_move: shakmaty::Move, turn: Color) -> Option<Self> {
        Some(Self {
            from: chess_move.from()?.to_string(),
            to: board_target(chess_move, turn).to_string(),
        })
    }
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
struct LegalMove {
    #[serde(flatten)]
    squares: MoveSquares,
    promotion: bool,
}

#[derive(Debug, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
struct NotationMove {
    place: Vec<usize>,
    number: u32,
    side: Side,
    text: String,
    comments_before: Vec<String>,
    comments_after: Vec<String>,
    symbols: Vec<String>,
    variations: Vec<Vec<Self>>,
}

fn comment_texts(comments: &[String]) -> Vec<String> {
    comments
        .iter()
        .map(|comment| shapes::split(comment).0)
        .filter(|text| !text.is_empty())
        .collect()
}

impl NotationMove {
    fn from_step(
        placed: &PlacedStep<'_>,
        current_place: &Place,
        current: &mut CurrentPlace,
    ) -> Self {
        let place = placed.place();
        let before = placed.step().position_before();
        let node = placed.step().node();
        let is_current = &place == current_place;
        if is_current && let Move::Legal(chess_move) = node.played() {
            current.last_move = MoveSquares::from_move(chess_move, before.turn());
        }
        let mut symbols = Vec::new();
        let mut position_symbols = Vec::new();
        let mut other_symbols = Vec::new();
        for &glyph in node.annotation_symbols() {
            if let Some(symbol) = editor::MoveSymbol::from_glyph(glyph).map(MoveSymbol::from) {
                symbols.push(symbol.text().to_owned());
                if is_current {
                    current.move_symbol.get_or_insert(symbol);
                }
            } else if let Some(symbol) =
                editor::PositionSymbol::from_glyph(glyph).map(PositionSymbol::from)
            {
                position_symbols.push(symbol.text().to_owned());
                if is_current {
                    current.position_symbol.get_or_insert(symbol);
                }
            } else {
                other_symbols.push(format!("${}", glyph.0));
            }
        }
        symbols.extend(position_symbols);
        symbols.extend(other_symbols);
        // The first main-line move holds the comment for the starting position.
        let comments_before = if place.0 == [0] {
            comment_texts(node.comments_before())
        } else {
            node.comments_before().to_vec()
        };
        let mut readings = MoveReadings::default();
        let comments_after = node
            .comments_after()
            .iter()
            .map(|comment| readings.read_comment(comment))
            .filter(|text| !text.is_empty())
            .collect();
        if is_current {
            current.evaluation = readings.evaluation;
            current.clock = readings.clock.map(|time| RecordedClock {
                side: before.turn().into(),
                time,
            });
        }
        Self {
            place: place.0,
            number: before.fullmoves().get(),
            side: before.turn().into(),
            text: placed.step().notation().to_string(),
            comments_before,
            comments_after,
            symbols,
            variations: placed
                .variations()
                .map(|line| {
                    line.map(|step| Self::from_step(&step, current_place, current))
                        .collect()
                })
                .collect(),
        }
    }
}

impl GamePicture {
    fn from_editor(editor: &GameEditor, file: &GameSource, revision: usize) -> Self {
        let position = editor.position();
        let (comment, shapes) = editor.comment();
        let mut current = CurrentPlace {
            in_variation: editor.in_variation(),
            has_next_move: editor.has_next_move(),
            ..CurrentPlace::default()
        };
        let notation = editor
            .walk()
            .map(|step| NotationMove::from_step(&step, editor.place(), &mut current))
            .collect();
        let legal_moves = position
            .legal_moves()
            .into_iter()
            // All four promotions have the same squares. Offer that pair once on the board.
            .filter(|chess_move| {
                chess_move
                    .promotion()
                    .is_none_or(|role| role == Role::Queen)
            })
            .filter_map(|chess_move| {
                Some(LegalMove {
                    squares: MoveSquares::from_move(chess_move, position.turn())?,
                    promotion: chess_move.is_promotion(),
                })
            })
            .collect();
        Self {
            revision,
            tags: editor
                .game()
                .tags()
                .iter()
                .map(|tag| Tag {
                    name: tag.name.clone(),
                    value: tag.value.clone(),
                })
                .collect(),
            result: editor.game().result().into(),
            fen: Fen::from_position(position, EnPassantMode::Legal).to_string(),
            turn: position.turn().into(),
            in_check: position.is_check(),
            tablebase_unavailable_reason: tablebase::unavailable_reason(position),
            legal_moves,
            current,
            shapes: BoardShapes::from(&shapes),
            comment,
            can_undo: editor.can_undo(),
            can_redo: editor.can_redo(),
            start_comments: match editor.game().main_line() {
                MainLine::Empty { comments } => comment_texts(comments),
                MainLine::Moves(_) => Vec::new(),
            },
            place: editor.place().0.clone(),
            notation,
            saved: SavedState::of(editor.game(), file),
        }
    }
}

/// The open games of one run of the app.
pub struct Games {
    open_games: BTreeMap<usize, OpenGame>,
    next_number: usize,
}

impl Default for Games {
    fn default() -> Self {
        Self {
            open_games: BTreeMap::new(),
            next_number: 1,
        }
    }
}

impl Games {
    fn open(&mut self, game: Game, file: GameSource) -> (usize, GamePicture) {
        let open = OpenGame::new(GameEditor::new(game), file);
        let picture = open.picture();
        (self.insert(open), picture)
    }

    fn insert(&mut self, open: OpenGame) -> usize {
        let number = self.next_number;
        self.next_number = number.strict_add(1);
        self.open_games.insert(number, open);
        number
    }

    fn open_pgn(&mut self, text: &str) -> Option<OpenedGame> {
        let mut first = None;
        pgn::read_games(text.as_bytes(), |entry| {
            if first.is_none()
                && let Entry::Game { game, problem, .. } = entry
            {
                first = Some((game, problem));
            }
        })
        .ok()?;
        let (game, problem) = first?;
        let problem = problem.map(|problem| {
            let moves = game.counts().main_line_moves;
            let noun = if moves == 1 { "move" } else { "moves" };
            format!("{}, {moves} {noun} kept", problem.reason)
        });
        let (game, picture) = self.open(*game, GameSource::New);
        Some(OpenedGame {
            game,
            picture,
            problem,
        })
    }

    fn edit<T>(
        &mut self,
        game: usize,
        change: impl FnOnce(&mut GameEditor) -> T,
    ) -> Option<GamePicture> {
        let open = self.open_games.get_mut(&game)?;
        // A refused edit leaves the editor at the position the view should draw.
        let _result = open.edit(change);
        Some(open.picture())
    }

    /// Answers a request one step after the other. The app does not call this, because it
    /// would hold the lock on its games during file or network I/O. It calls
    /// [`SharedGames::answer`].
    #[must_use]
    pub fn answer(&mut self, request: Request) -> Response {
        match self.begin(request, &SaveHistory::default()) {
            FirstStep::Answer(response) => *response,
            FirstStep::BlockingWork(work) => work()(self),
        }
    }

    /// Applies the view's request and builds its answer before another request can edit the
    /// game. A request that waits for I/O only starts here.
    fn begin(&mut self, request: Request, history: &SaveHistory) -> FirstStep {
        let response = match request {
            Request::Version => Response::Version {
                version: env!("CARGO_PKG_VERSION").to_owned(),
            },
            Request::NewGame => {
                let new = GameBuilder::new(Vec::new(), Chess::default()).finish(Outcome::Unknown);
                let (game, picture) = self.open(new, GameSource::New);
                Response::NewGame { game, picture }
            }
            Request::DescribePosition { input } => Response::DescribePosition {
                draft: input.describe(),
            },
            Request::SetStartPosition { game, fen } => Response::SetStartPosition {
                change: self.open_games.get_mut(&game).map(|open| {
                    let refused = fen.is_none_or(|position| {
                        open.edit(|editor| editor.set_start_position(position))
                            .is_err()
                    });
                    PositionChange {
                        picture: open.picture(),
                        refused,
                    }
                }),
            },
            Request::OpenPgnText { text } => Response::OpenPgnText {
                opened: self.open_pgn(&text),
            },
            Request::OpenGameLink { link } => return open_game_link(link),
            Request::LookupTablebase { game, revision } => return self.tablebase(game, revision),
            Request::GamePgn { game } => Response::GamePgn(self.open_games.get(&game).into()),
            Request::SetTag { game, tag, text } => Response::SetTag {
                picture: self.edit(game, |editor| editor.set_tag(tag.into(), &text)),
            },
            Request::SetResult { game, result } => Response::SetResult {
                picture: self.edit(game, |editor| editor.set_result(result.into())),
            },
            Request::PlayMove {
                game,
                from,
                to,
                promotion,
            } => Response::PlayMove {
                picture: self.edit(game, |editor| {
                    editor.play(from, to, promotion.map(Into::into))
                }),
            },
            Request::GoTo { game, place } => Response::GoTo {
                picture: self.edit(game, |editor| editor.go_to(Place(place))),
            },
            Request::SetComment { game, text } => Response::SetComment {
                picture: self.edit(game, |editor| editor.set_comment(&text)),
            },
            Request::SetShapes(edit) => Response::SetShapes {
                picture: self.edit(edit.game, |editor| editor.set_shapes(&edit.shapes)),
            },
            Request::SetMoveSymbol { game, symbol } => Response::SetMoveSymbol {
                picture: self.edit(game, |editor| {
                    editor.set_move_symbol(symbol.map(Into::into))
                }),
            },
            Request::SetPositionSymbol { game, symbol } => Response::SetPositionSymbol {
                picture: self.edit(game, |editor| {
                    editor.set_position_symbol(symbol.map(Into::into))
                }),
            },
            Request::Undo { game } => Response::Undo {
                picture: self.edit(game, GameEditor::undo),
            },
            Request::Redo { game } => Response::Redo {
                picture: self.edit(game, GameEditor::redo),
            },
            Request::PromoteVariation { game } => Response::PromoteVariation {
                picture: self.edit(game, GameEditor::promote),
            },
            Request::DeleteVariation { game } => Response::DeleteVariation {
                picture: self.edit(game, GameEditor::delete_variation),
            },
            Request::DeleteRest { game } => Response::DeleteRest {
                picture: self.edit(game, GameEditor::delete_rest),
            },
            Request::CloseGame { game } => {
                if let Some(open) = self.open_games.remove(&game) {
                    return FirstStep::blocking_work(move || {
                        analysis::close(&open.analysis);
                        |_| Response::CloseGame
                    });
                }
                Response::CloseGame
            }
            Request::OpenPgnFile { path } => return pgn_file::open(path),
            Request::CreatePgnFile { path } => return pgn_file::create(path),
            Request::DeletePgnGame(listed) => return game_file::delete(listed, history),
            Request::OpenGame { path, number } => return game_file::open(path, number),
            Request::SaveGame { game } => return game_file::save(self, game, history),
            Request::SaveGameAs { game, path } => {
                return game_file::save_as(self, game, path, history);
            }
        };
        FirstStep::Answer(Box::new(response))
    }
}

fn open_game_link(link: String) -> FirstStep {
    FirstStep::blocking_work(move || {
        let fetched = zobrist_online::fetch_game(&link);
        move |games: &mut Games| Response::OpenGameLink {
            answer: match fetched {
                Ok(fetched) => {
                    let (game, picture) = games.open(fetched, GameSource::New);
                    OpenGameLinkAnswer::Opened {
                        game,
                        picture: Box::new(picture),
                    }
                }
                Err(refusal) => OpenGameLinkAnswer::Refused {
                    reason: refusal.to_string(),
                },
            },
        }
    })
}

/// How far a request got in its first step, which has the games.
enum FirstStep {
    Answer(Box<Response>),
    /// Only the final step of a file or network operation needs the games lock again.
    BlockingWork(Box<dyn FnOnce() -> LastStep + Send>),
}

type LastStep = Box<dyn FnOnce(&mut Games) -> Response + Send>;

impl FirstStep {
    fn blocking_work<Last>(work: impl FnOnce() -> Last + Send + 'static) -> Self
    where
        Last: FnOnce(&mut Games) -> Response + Send + 'static,
    {
        Self::BlockingWork(Box::new(move || Box::new(work())))
    }
}

/// The open games behind their lock, as the requests of the view share them.
#[derive(Default)]
pub struct SharedGames {
    long_tasks: std::sync::Mutex<long_tasks::LongTasks>,
    games: std::sync::Arc<Mutex<Games>>,
    // A failed recovery must preserve this original before the next save of the boards.
    one_save: Mutex<Option<PathBuf>>,
    restored_selection: OnceLock<Option<usize>>,
    file_rows: RwLock<()>,
    history: SaveHistory,
}

impl SharedGames {
    /// Enables PGN history for desktop saves, reporting a directory lookup failure at Save.
    #[must_use]
    pub fn with_history(directory: Result<PathBuf, String>) -> Self {
        Self {
            history: match directory {
                Ok(path) => SaveHistory::Directory(path),
                Err(reason) => SaveHistory::Unavailable(reason),
            },
            ..Self::default()
        }
    }

    /// Answers a request. It holds the lock on the games only for the steps that touch them.
    /// File and network I/O run between those steps, on a thread that may wait.
    pub async fn answer(&self, request: Request) -> tauri::Result<Response> {
        // A game read before a deletion belongs to its old row until its tab is registered.
        // Take the row lock first so a slow open cannot block saves to another file.
        let _opening = if matches!(request, Request::OpenGame { .. }) {
            Some(self.file_rows.read().await)
        } else {
            None
        };
        let _deleting = if matches!(request, Request::DeletePgnGame(_)) {
            Some(self.file_rows.write().await)
        } else {
            None
        };
        // A save publishes its file whole. Two saves into one file that overlap would each
        // publish it without the game of the other, so a save waits for the save before it.
        let saves = matches!(
            request,
            Request::SaveGame { .. } | Request::SaveGameAs { .. } | Request::DeletePgnGame(_)
        );
        let _one_save = if saves {
            Some(self.one_save.lock().await)
        } else {
            None
        };
        let first = self.games.lock().await.begin(request, &self.history);
        match first {
            FirstStep::Answer(response) => Ok(*response),
            FirstStep::BlockingWork(work) => {
                let last = spawn_blocking(work).await?;
                Ok(last(&mut *self.games.lock().await))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::io;

    use serde_json::json;
    use zobrist_core::pgn::{self, Entry};

    use super::*;

    fn import(source: &[u8]) -> io::Result<GameEditor> {
        let mut imported = None;
        pgn::read_games(source, |entry| {
            if let Entry::Game { game, problem, .. } = entry {
                assert_eq!(problem, None);
                imported = Some(GameEditor::new(*game));
            }
        })?;
        Ok(imported.unwrap())
    }

    #[test]
    fn recorded_values_follow_the_selected_move_and_leave_its_comment_editable() -> io::Result<()> {
        let mut editor = import(b"1. e4 {Centre. [%eval 0.34,18] [%clk 0:04:59]} e5 {[%clk 0:04:56]} (1... c5 {[%eval -0.25]}) 2. Nf3 *")?;
        for (place, evaluation, clock, comment) in [
            (
                vec![0],
                Some("+0.34"),
                json!({"side":"white","time":"0:04:59"}),
                "Centre. [%eval 0.34,18] [%clk 0:04:59]",
            ),
            (
                vec![0, 0],
                None,
                json!({"side":"black","time":"0:04:56"}),
                "[%clk 0:04:56]",
            ),
            (vec![0, 1], Some("-0.25"), json!(null), "[%eval -0.25]"),
            (vec![0, 0, 0], None, json!(null), ""),
            (vec![], None, json!(null), ""),
        ] {
            editor.go_to(Place(place)).unwrap();
            let picture =
                serde_json::to_value(GamePicture::from_editor(&editor, &GameSource::New, 0))?;
            assert_eq!(picture["evaluation"], json!(evaluation));
            assert_eq!(picture["clock"], clock);
            assert_eq!(picture["comment"], comment);
            assert_eq!(picture["notation"][0]["commentsAfter"], json!(["Centre."]));
        }
        Ok(())
    }

    #[test]
    fn the_editable_comment_follows_the_move_or_the_starting_position() -> io::Result<()> {
        let mut editor =
            import(b"{A short game} 1. e4 {Best} {by test} e5 (1... c5 {Sicilian}) *")?;
        for (place, comment) in [
            (vec![], "A short game"),
            (vec![0], "Best by test"),
            (vec![0, 1], "Sicilian"),
            (vec![0, 0], ""),
        ] {
            editor.go_to(Place(place)).unwrap();
            assert_eq!(
                GamePicture::from_editor(&editor, &GameSource::New, 0).comment,
                comment
            );
        }
        let empty = import(b"{Not} {played} *")?;
        assert_eq!(
            GamePicture::from_editor(&empty, &GameSource::New, 0).comment,
            "Not played"
        );
        Ok(())
    }

    #[test]
    fn imported_shapes_follow_their_place_and_leave_midgame_prefaces_intact() -> io::Result<()> {
        let mut editor = import(b"{Start [%cal Rd1h5]} 1. e4 {Plan [%csl Gd4]} {more} {[%cal Ge2e4]} (1. d4 {[%csl Ba1]}) {Before [%csl Yb2]} e5 *")?;
        let picture = serde_json::to_value(GamePicture::from_editor(&editor, &GameSource::New, 0))?;
        assert_eq!(
            picture["arrows"],
            json!([{ "color": "red", "from": "d1", "to": "h5" }])
        );
        assert_eq!(picture["squares"], json!([]));
        let notation = &picture["notation"];
        assert_eq!(notation[0]["commentsBefore"], json!(["Start"]));
        assert_eq!(notation[0]["commentsAfter"], json!(["Plan", "more"]));
        assert_eq!(notation[0]["variations"][0][0]["commentsAfter"], json!([]));
        assert_eq!(notation[1]["commentsBefore"], json!(["Before [%csl Yb2]"]));

        editor.go_to(Place(vec![1])).unwrap();
        let picture = serde_json::to_value(GamePicture::from_editor(&editor, &GameSource::New, 0))?;
        assert_eq!(
            picture["squares"],
            json!([{ "color": "blue", "square": "a1" }])
        );
        assert_eq!(picture["arrows"], json!([]));
        Ok(())
    }

    #[test]
    fn the_empty_game_shows_only_text_in_its_start_comments() -> io::Result<()> {
        let editor = import(b"{[%csl Gd4]} {Plan [%cal Ge2e4]} {[%clk 0:15]} *")?;
        let picture = serde_json::to_value(GamePicture::from_editor(&editor, &GameSource::New, 0))?;
        assert_eq!(picture["startComments"], json!(["Plan", "[%clk 0:15]"]));
        assert_eq!(
            picture["squares"],
            json!([{ "color": "green", "square": "d4" }])
        );
        assert_eq!(
            picture["arrows"],
            json!([{ "color": "green", "from": "e2", "to": "e4" }])
        );
        Ok(())
    }

    #[test]
    fn pictures_keep_imported_tag_order_and_read_the_result_from_the_marker() -> io::Result<()> {
        let source = b"[Annotator \"N\"]\n[BlackElo \"2100\"]\n[Result \"*\"]\n\n1. e4 0-1";
        let mut imported = None;
        pgn::read_games(source.as_slice(), |entry| {
            if let Entry::Game { game, .. } = entry {
                imported = Some(GameEditor::new(*game));
            }
        })?;
        let picture = serde_json::to_value(GamePicture::from_editor(
            &imported.unwrap(),
            &GameSource::New,
            0,
        ))?;
        assert_eq!(
            picture["tags"],
            json!([
                { "name": "Annotator", "value": "N" },
                { "name": "BlackElo", "value": "2100" },
                { "name": "Result", "value": "*" }
            ])
        );
        assert_eq!(picture["result"], "blackWin");
        Ok(())
    }

    #[test]
    fn pictures_group_imported_glyphs_and_select_the_first_of_each_kind() -> io::Result<()> {
        let source =
            b"{Start} 1. e4 $7 $10 $1 $16 $3 $13 $2 $14 $4 $15 $5 $17 $6 $18 $19 $36 {one} {two} *";
        let mut imported = None;
        pgn::read_games(source.as_slice(), |entry| {
            if let Entry::Game { game, problem, .. } = entry {
                assert_eq!(problem, None);
                imported = Some(*game);
            }
        })?;
        let mut editor = GameEditor::new(imported.unwrap());
        editor.go_to(Place(vec![0])).unwrap();
        let picture = serde_json::to_value(GamePicture::from_editor(&editor, &GameSource::New, 0))?;
        assert_eq!(picture["moveSymbol"], "!");
        assert_eq!(picture["positionSymbol"], "=");
        assert_eq!(picture["startComments"], json!([]));
        assert_eq!(
            picture["notation"],
            json!([{
                "place": [0], "number": 1, "side": "white", "text": "e4",
                "commentsBefore": ["Start"], "commentsAfter": ["one", "two"],
                "symbols": ["!", "!!", "?", "??", "!?", "?!", "=", "±", "∞", "⩲", "⩱", "∓", "+-", "-+", "$7", "$36"],
                "variations": []
            }])
        );
        Ok(())
    }
}
