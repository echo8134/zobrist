//! Reads and creates game databases, reads one game of a PGN file, adds games to PGN files, and
//! replaces one game of a PGN file.
//!
//! # File format, version 1
//!
//! All integers are little-endian. A file starts with the eight bytes `ZOBRIST\0`
//! (the eighth byte is zero) followed by a `u32` version. Exactly one segment follows.
//! The segment starts at byte 12 with its `u64` length, including that length field.
//! Seven parts follow immediately in kind order, without padding.
//!
//! The last 172 bytes are the part table (168 bytes) and its CRC-32 (4 bytes).
//! Each row below gives the byte range of a field within a 24-byte entry.
//!
//! | Bytes | Field |
//! |---|---|
//! | 0–3 | Part kind (`u32`), numbered 1 through 7 in the order of the game parts below |
//! | 4–11 | Absolute file offset of the first byte, a `u64` |
//! | 12–19 | Number of bytes in the part, a `u64` |
//! | 20–23 | CRC-32 of exactly those bytes, a `u32` |
//!
//! All checksums are IEEE CRC-32. The number of games follows from the length of the games
//! part and its fixed record width.
//!
//! ## Game parts
//!
//! A string is a `u64` byte count followed by that many UTF-8 bytes. All indexes count from zero.
//!
//! | Part | Fields in file order |
//! | --- | --- |
//! | Games | One 18-byte record per game. Bytes 0–7 give its first move index, 8–15 its main-line move count, 16 its result (0 unknown, 1 White wins, 2 Black wins, 3 draw), and 17 its flags (bit 0 start position, bit 1 annotations). |
//! | Moves | One `u16` per main-line move. Bits 0–5 give the start square and 6–11 the target square (a1 = 0, h1 = 7, h8 = 63). Bits 12–15 give the promotion (0 none, 2 knight, 3 bishop, 4 rook, 5 queen). Castling targets the rook's square. The whole word `0xffff` is a null move. |
//! | Start positions | For each game with flag bit 0 set, one byte for the castling mode (0 standard, 1 Chess960), then a FEN string including en passant and both clocks. Other games use the standard opening position. |
//! | Tag names | Strings in order of first appearance. A name's index in this part is its ID. |
//! | Tag values | Strings in order of first appearance. A value's index in this part is its ID. |
//! | Tags | For each game, a `u64` tag count followed by that many (`u64` name ID, `u64` value ID) pairs in tag order. |
//! | Annotations | For each game with flag bit 1 set, a sequence of the instructions below ending with byte 0. |
//!
//! Each annotation instruction starts with one byte. Operands follow immediately.
//!
//! | Byte | Instruction |
//! | --- | --- |
//! | 0 | End game. |
//! | 1 | Play the next move from this game's main line in Moves. |
//! | 2 | Play the following `u16` move inside a variation, encoded as in Moves. |
//! | 3 | Begin a variation of the preceding move. |
//! | 4 | End the current variation. |
//! | 5 | Add the following string as a comment. |
//! | 6 | Add the following `u8` annotation symbol to the preceding move. |

mod codec;
mod import;
mod pgn_file;
mod pgn_history;

pub use import::{ImportError, PgnReport, import_pgn, problem_line, read_pgn_entries};
pub use pgn_file::{
    PgnBatch, PgnGameRow, PgnWriteConflict, add_pgn_game, add_pgn_games, read_pgn_game,
    replace_pgn_game,
};

pub use pgn_history::{
    PgnHistory, PgnState, PgnStateDetails, pgn_states, prepare_pgn_restore, restore_pgn_state,
};

use std::error::Error;
use std::fmt;
use std::fs::{self, DirBuilder, File};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::iter;
use std::path::{Path, PathBuf};
use std::process;
use std::sync::atomic::{AtomicU64, Ordering};

use crc32fast::Hasher;
use zobrist_core::game::Game;

const MAGIC: &[u8; 8] = b"ZOBRIST\0";
const VERSION: u32 = 1;
const FILE_HEADER: u64 = 12;
const FIRST_PART: u64 = FILE_HEADER + 8;
const PARTS: usize = 7;
const TABLE_BYTES: usize = PARTS * 24;
static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

/// Why a database could not be opened or a stored game could not be read.
#[derive(Debug)]
pub enum OpenError {
    /// Reading the file failed.
    Read(io::Error),
    /// The file does not identify itself as a zobrist database.
    NotDatabase,
    /// The header names another format version.
    Version {
        /// The version in the file.
        found: u32,
        /// The version this reader supports.
        expected: u32,
    },
    /// A length, checksum, or game record is invalid.
    Damaged,
}

impl fmt::Display for OpenError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read(error) => error.fmt(formatter),
            Self::NotDatabase => formatter.write_str("not a zobrist database"),
            Self::Version { found, expected } => {
                write!(formatter, "database version {found}, expected {expected}")
            }
            Self::Damaged => formatter.write_str("damaged zobrist database"),
        }
    }
}

impl Error for OpenError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Read(error) => Some(error),
            Self::NotDatabase | Self::Version { .. } | Self::Damaged => None,
        }
    }
}

impl From<io::Error> for OpenError {
    fn from(error: io::Error) -> Self {
        if matches!(
            error.kind(),
            io::ErrorKind::InvalidData | io::ErrorKind::UnexpectedEof
        ) {
            Self::Damaged
        } else {
            Self::Read(error)
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct Part {
    offset: u64,
    length: u64,
}

/// An open database whose header, part table and part checksums have been verified.
#[derive(Debug)]
pub struct Database {
    file: File,
    parts: [Part; PARTS],
}

impl Database {
    /// Opens a version 1 database and verifies every part before returning.
    pub fn open(path: &Path) -> Result<Self, OpenError> {
        let mut file = File::open(path).map_err(OpenError::Read)?;
        match read_bytes(&mut file) {
            Ok(bytes) if &bytes == MAGIC => {}
            Ok(_) => return Err(OpenError::NotDatabase),
            Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => {
                return Err(OpenError::NotDatabase);
            }
            Err(error) => return Err(OpenError::Read(error)),
        }
        let version = u32::from_le_bytes(read_bytes(&mut file)?);
        if version != VERSION {
            return Err(OpenError::Version {
                found: version,
                expected: VERSION,
            });
        }
        let length = file.metadata()?.len();
        let segment_length = u64::from_le_bytes(read_bytes(&mut file)?);
        if segment_length.checked_add(FILE_HEADER) != Some(length) {
            return Err(OpenError::Damaged);
        }
        let table_offset = length
            .checked_sub(TABLE_BYTES as u64 + 4)
            .filter(|offset| *offset >= FIRST_PART)
            .ok_or(OpenError::Damaged)?;
        file.seek(SeekFrom::Start(table_offset))?;
        let table = read_bytes::<TABLE_BYTES>(&mut file)?;
        if crc32fast::hash(&table) != u32::from_le_bytes(read_bytes(&mut file)?) {
            return Err(OpenError::Damaged);
        }

        let mut parts = [Part::default(); PARTS];
        let mut entries = table.as_slice();
        let mut next = FIRST_PART;
        file.seek(SeekFrom::Start(next))?;
        for (kind, part) in (1_u32..).zip(&mut parts) {
            let stored_kind = u32::from_le_bytes(read_bytes(&mut entries)?);
            let offset = u64::from_le_bytes(read_bytes(&mut entries)?);
            let length = u64::from_le_bytes(read_bytes(&mut entries)?);
            let checksum = u32::from_le_bytes(read_bytes(&mut entries)?);
            if stored_kind != kind || offset != next || length > table_offset - next {
                return Err(OpenError::Damaged);
            }
            let measured = copy_with_checksum((&mut file).take(length), io::sink())?;
            if measured != (length, checksum) {
                return Err(OpenError::Damaged);
            }
            *part = Part { offset, length };
            next += length;
        }
        if next != table_offset || !parts[0].length.is_multiple_of(codec::GAME_WIDTH) {
            return Err(OpenError::Damaged);
        }
        Ok(Self { file, parts })
    }

    /// The number of games in the segment.
    #[must_use]
    pub fn game_count(&self) -> u64 {
        self.parts[0].length / codec::GAME_WIDTH
    }

    /// Reads games in stored order. Each call starts again at the first game.
    /// The iterator stops after the first read or decoding error.
    pub fn games(
        &mut self,
    ) -> Result<impl Iterator<Item = Result<Game, OpenError>> + '_, OpenError> {
        let readers = self.parts.map(|part| {
            BufReader::new(PartReader {
                file: &self.file,
                next: part.offset,
                end: part.offset + part.length,
            })
        });
        let mut decoder = codec::Decoder::new(readers, self.game_count())?;
        let mut finished = false;
        Ok(iter::from_fn(move || {
            if finished {
                return None;
            }
            let next = decoder.next_game();
            finished = !matches!(next, Ok(Some(_)));
            next.map_err(OpenError::from).transpose()
        }))
    }
}

struct PartReader<'file> {
    file: &'file File,
    next: u64,
    end: u64,
}

impl Read for PartReader<'_> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let remaining = usize::try_from(self.end - self.next).unwrap_or(usize::MAX);
        let limit = remaining.min(buffer.len());
        if limit == 0 {
            return Ok(0);
        }
        // Buffered readers share a file handle, so each refill sets its own position.
        self.file.seek(SeekFrom::Start(self.next))?;
        let count = self.file.read(&mut buffer[..limit])?;
        self.next += count as u64;
        Ok(count)
    }
}

fn read_bytes<const N: usize>(source: &mut impl Read) -> io::Result<[u8; N]> {
    let mut bytes = [0; N];
    source.read_exact(&mut bytes)?;
    Ok(bytes)
}

fn copy_with_checksum(mut source: impl Read, mut target: impl Write) -> io::Result<(u64, u32)> {
    let mut checksum = Hasher::new();
    let mut length = 0;
    let mut buffer = [0; 8192];
    loop {
        let count = match source.read(&mut buffer) {
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            result => result?,
        };
        if count == 0 {
            return Ok((length, checksum.finalize()));
        }
        target.write_all(&buffer[..count])?;
        checksum.update(&buffer[..count]);
        length += count as u64;
    }
}

struct StagingDirectory(PathBuf);

impl StagingDirectory {
    fn beside(target: &Path) -> io::Result<Self> {
        let name = target.file_name().ok_or(io::ErrorKind::InvalidInput)?;
        let parent = target.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let directory = DirBuilder::new();
        #[cfg(unix)]
        let directory = {
            use std::os::unix::fs::DirBuilderExt as _;
            let mut directory = directory;
            directory.mode(0o700);
            directory
        };
        loop {
            let number = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
            let temporary = format!(".zobrist-{}-{number}", process::id());
            if name.eq_ignore_ascii_case(temporary.as_str()) {
                continue;
            }
            let path = parent.join(temporary);
            match directory.create(&path) {
                Ok(()) => return Ok(Self(path)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
                Err(error) => return Err(error),
            }
        }
    }
}

impl Drop for StagingDirectory {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).ok();
    }
}

/// Accepts one game at a time. Finishing the writer publishes the complete database.
pub struct DatabaseWriter {
    target: PathBuf,
    parts: [BufWriter<File>; PARTS],
    staging: StagingDirectory,
    encoder: Option<codec::Encoder>,
}

impl DatabaseWriter {
    /// Starts a database in the target's directory. An existing target is refused.
    pub fn create(target: &Path) -> io::Result<Self> {
        let target = std::path::absolute(target)?;
        match fs::symlink_metadata(&target) {
            Ok(_) => return Err(io::ErrorKind::AlreadyExists.into()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        let staging = StagingDirectory::beside(&target)?;
        let create = |name: &str| File::create_new(staging.0.join(name)).map(BufWriter::new);
        let parts = [
            create("games")?,
            create("moves")?,
            create("starts")?,
            create("names")?,
            create("values")?,
            create("tags")?,
            create("annotations")?,
        ];
        Ok(Self {
            target,
            parts,
            staging,
            encoder: Some(codec::Encoder::default()),
        })
    }

    /// Adds one game. After a write error the writer can only be discarded.
    pub fn push(&mut self, game: &Game) -> io::Result<()> {
        let encoder = self.encoder.as_mut().ok_or_else(failed_push)?;
        let result = encoder.push(game, &mut self.parts);
        if result.is_err() {
            self.encoder = None;
        }
        result
    }

    /// Syncs the segment, publishes it without replacing any file, and syncs the directory.
    pub fn finish(mut self) -> io::Result<()> {
        self.encoder.take().ok_or_else(failed_push)?;
        let path = self.staging.0.join("database");
        let mut file = BufWriter::new(File::create_new(&path)?);
        file.write_all(MAGIC)?;
        file.write_all(&VERSION.to_le_bytes())?;
        file.write_all(&0_u64.to_le_bytes())?;
        let mut table = Vec::with_capacity(TABLE_BYTES);
        let mut next = FIRST_PART;
        for (kind, part) in (1_u32..).zip(&mut self.parts) {
            part.flush()?;
            let source = part.get_mut();
            source.rewind()?;
            let (length, checksum) = copy_with_checksum(source, &mut file)?;
            table.extend_from_slice(&kind.to_le_bytes());
            table.extend_from_slice(&next.to_le_bytes());
            table.extend_from_slice(&length.to_le_bytes());
            table.extend_from_slice(&checksum.to_le_bytes());
            next += length;
        }
        file.write_all(&table)?;
        file.write_all(&crc32fast::hash(&table).to_le_bytes())?;
        let segment_length = next + TABLE_BYTES as u64 + 4 - FILE_HEADER;
        file.seek(SeekFrom::Start(FILE_HEADER))?;
        file.write_all(&segment_length.to_le_bytes())?;
        file.flush()?;
        file.get_ref().sync_all()?;

        let parent = self.staging.0.parent().ok_or(io::ErrorKind::InvalidInput)?;
        let mut options = File::options();
        options.read(true);
        #[cfg(windows)]
        {
            use std::os::windows::fs::OpenOptionsExt as _;
            // Opening a directory requires FILE_FLAG_BACKUP_SEMANTICS.
            options.write(true).custom_flags(0x0200_0000);
        }
        let directory = options.open(parent)?;
        fs::hard_link(path, &self.target)?;
        drop(file);
        drop(self);
        directory.sync_all()
    }
}

fn failed_push() -> io::Error {
    io::Error::other("a previous write to this database failed")
}
