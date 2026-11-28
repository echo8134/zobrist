use std::fs::{self, File};
use std::hash::{DefaultHasher, Hash, Hasher};
use std::io::{self, BufReader, BufWriter, Read, Seek, SeekFrom, Write};
use std::num::NonZeroU64;
use std::ops::Range;
use std::path::{Path, PathBuf};

use zobrist_core::game::Game;
use zobrist_core::pgn::{self, Entry};

use std::sync::Arc;

use crate::pgn_history::{PgnHistory, SaveTarget};
use crate::{PendingWrite, StagingDirectory};

/// A PGN change refused before publication.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PgnWriteConflict {
    /// The game in this row differs from the one the board opened.
    GameChanged,
    /// Another change still holds the file's history lock.
    HistoryLocked,
}

impl PgnWriteConflict {
    /// Identifies store refusals among the errors returned by a PGN write.
    #[must_use]
    pub fn from_io_error(error: &io::Error) -> Option<Self> {
        error.get_ref()?.downcast_ref::<Self>().copied()
    }
}

impl std::fmt::Display for PgnWriteConflict {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::GameChanged => {
                "The file changed since this game was opened. Save it as a new game."
            }
            Self::HistoryLocked => "Another change to this file is still running. Try again.",
        })
    }
}

impl std::error::Error for PgnWriteConflict {}

impl From<PgnWriteConflict> for io::Error {
    fn from(conflict: PgnWriteConflict) -> Self {
        Self::other(conflict)
    }
}

/// Complete raw PGN games staged on disk until their destination can be published.
pub struct PgnBatch {
    file: File,
    _directory: Arc<StagingDirectory>,
    write: Arc<PendingWrite>,
    games: Option<u64>,
    first_start: u64,
}

impl PgnBatch {
    /// Starts a batch without opening its eventual destination.
    pub fn new() -> io::Result<Self> {
        Self::for_task(&Arc::default())
    }

    /// Starts a batch whose unpublished files can be discarded by its owner.
    pub fn for_task(write: &Arc<PendingWrite>) -> io::Result<Self> {
        write.stage(&std::env::temp_dir().join("games.pgn"), |directory| {
            Ok(Self {
                file: File::create_new(directory.0.join("games.pgn"))?,
                _directory: directory,
                write: Arc::clone(write),
                games: Some(0),
                first_start: 0,
            })
        })
    }

    /// Stages one finished PGN, retaining its tags, annotations and variant moves verbatim.
    /// After a failed push the batch cannot be published.
    pub fn push(&mut self, pgn: &str) -> io::Result<()> {
        self.write.check()?;
        let games = self.games.take().ok_or_else(failed_write)?;
        let mut entries = 0;
        let mut start = None;
        let marker = pgn.split_ascii_whitespace().next_back();
        pgn::read_export_record(pgn.as_bytes(), |entry| {
            entries += 1;
            if let Entry::Game {
                game,
                range: Some(range),
                problem,
            } = entry
            {
                // The board reader checks standard chess moves even for a variant export.
                let variant = game.tag("Variant").is_some_and(|name| name != "Standard");
                let readable = problem.is_none_or(|problem| {
                    variant
                        && matches!(
                            problem.reason,
                            pgn::Reason::IllegalMove(_)
                                | pgn::Reason::AmbiguousMove(_)
                                | pgn::Reason::InvalidFen
                        )
                });
                let result = game.result().to_string();
                if readable
                    && !game.result().is_unknown()
                    && marker == Some(result.as_str())
                    && game.tag("Result").is_none_or(|tag| tag == result)
                {
                    start = Some(range.start);
                }
            }
        })?;
        let start = start.filter(|_| entries == 1).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "The export record does not contain one finished PGN game.",
            )
        })?;
        if games == 0 {
            self.first_start = start;
        } else {
            self.file.write_all(b"\n")?;
        }
        self.file.write_all(pgn.as_bytes())?;
        if !pgn.ends_with('\n') {
            self.file.write_all(b"\n")?;
        }
        self.games = Some(games + 1);
        Ok(())
    }

    /// Appends the batch after the current destination bytes in one publication.
    /// Returns the added count and any warning from recording history. An empty batch
    /// leaves both the destination and its history untouched.
    pub fn append_to(
        mut self,
        target: &Path,
        history: Option<PgnHistory<'_>>,
    ) -> io::Result<(u64, Option<String>)> {
        let games = self.games.ok_or_else(failed_write)?;
        if games == 0 {
            return Ok((0, None));
        }
        self.write.check()?;
        let target = SaveTarget::new(destination(target)?, history)?;
        let (mut staged, separator) = Staged::for_append(&target.path, &self.write)?;
        staged.file.write_all(separator)?;
        let first = staged.file.stream_position()? + self.first_start;
        self.file.rewind()?;
        io::copy(&mut self.file, &mut staged.file)?;
        // An open comment or tag in the destination can absorb part or all of the first game.
        staged.file.rewind()?;
        let mut found_first = false;
        let mut appended = 0;
        pgn::read_games(&staged.file, |entry| {
            if let Entry::Game {
                range: Some(range), ..
            } = entry
                && range.start >= first
            {
                found_first |= range.start == first;
                appended += 1;
            }
        })?;
        if !found_first || appended != games {
            return Err(refusal(
                "The destination would not keep every downloaded game separate.".to_owned(),
            ));
        }
        let warning = target.publish(staged)?;
        Ok((games, warning))
    }
}

/// Adds the supplied games to a PGN file, creating the target if necessary.
/// Follows symbolic links and stages the new file beside the destination.
/// Calls `supply` with a function that writes one game at a time. An error from
/// either function discards the staged file and leaves the target as it was.
/// Refuses an append when the end of the target takes in the added games.
/// Returns the number of games added and any warning about capturing the previous state.
pub fn add_pgn_games<E: From<io::Error>>(
    target: &Path,
    history: Option<PgnHistory<'_>>,
    supply: impl FnOnce(&mut dyn FnMut(&Game) -> io::Result<()>) -> Result<(), E>,
) -> Result<(u64, Option<String>), E> {
    let target = SaveTarget::new(destination(target)?, history)?;
    let mut added = 0;
    let mut first = None;
    let (mut staged, prefix_length) = stage_with_added(&target.path, |append| {
        supply(&mut |game| {
            append(game)?;
            // Only the first game has to start a new game after bytes we did not write.
            first.get_or_insert_with(|| game.clone());
            added += 1;
            Ok(())
        })
    })?;
    if let Some(first) = first
        && prefix_length > 0
    {
        staged.file.rewind()?;
        let mut old_games = 0;
        pgn::read_games((&staged.file).take(prefix_length), |entry| {
            if matches!(entry, Entry::Game { .. }) {
                old_games += 1;
            }
        })?;
        staged.file.rewind()?;
        let mut games = 0;
        let mut first_reads_back = false;
        pgn::read_games(&staged.file, |entry| {
            if let Entry::Game { game, .. } = entry {
                if games == old_games {
                    first_reads_back = *game == first;
                }
                games += 1;
            }
        })?;
        if !first_reads_back || games != old_games + added {
            return Err(refusal(
                "the added games would not read back as separate games after the existing games"
                    .to_owned(),
            )
            .into());
        }
    }
    let warning = target.publish(staged)?;
    Ok((added, warning))
}

/// Adds one game as the last game of a PGN file, creating the target if necessary, and returns
/// its number and any warning about capturing the previous state. Follows symbolic links
/// and stages the new file beside the destination.
///
/// Refuses, and leaves the target as it was, when the staged file does not read back with the
/// game as its last game. A comment or a tag line that is still open at the end of the target
/// would take the new game in.
pub fn add_pgn_game(
    target: &Path,
    game: &Game,
    history: Option<PgnHistory<'_>>,
) -> io::Result<(NonZeroU64, Option<String>)> {
    let target = SaveTarget::new(destination(target)?, history)?;
    let (staged, _) = stage_with_added(&target.path, |add| add(game))?;
    (&staged.file).rewind()?;
    let mut games = 0;
    let mut last_is_the_game = false;
    pgn::read_games(&staged.file, |entry| {
        if let Entry::Game {
            game: read,
            problem,
            ..
        } = entry
        {
            games += 1;
            last_is_the_game = problem.is_none() && *read == *game;
        }
    })?;
    let number = NonZeroU64::new(games)
        .filter(|_| last_is_the_game)
        .ok_or_else(|| {
            refusal("the file would not read back with the new game as its last game".to_owned())
        })?;
    let warning = target.publish(staged)?;
    Ok((number, warning))
}

/// Stages the target with the supplied games and returns the length of the prefix, including the separator.
fn stage_with_added<E: From<io::Error>>(
    target: &Path,
    supply: impl FnOnce(&mut dyn FnMut(&Game) -> io::Result<()>) -> Result<(), E>,
) -> Result<(Staged, u64), E> {
    let (mut staged, separator) = Staged::for_append(target, &Arc::default())?;
    let prefix_length = staged.file.stream_position()? + separator.len() as u64;
    let mut output = BufWriter::new(&staged.file);
    let mut separator = Some(separator);
    supply(&mut |game| {
        // A caller can ignore a failed game. Only a complete write restores the separator.
        let before = separator.take().ok_or_else(failed_write)?;
        output.write_all(before)?;
        pgn::write_game(game, &mut output)?;
        separator = Some(b"\n");
        Ok(())
    })?;
    separator.ok_or_else(failed_write)?;
    output.flush()?;
    drop(output);
    Ok((staged, prefix_length))
}

/// Replaces game `number` of a PGN file, counting from 1, and keeps every other byte of the
/// file. Returns any warning about capturing the previous state. If supplied, the expected
/// game must still occupy that row. A mismatch returns [`PgnWriteConflict::GameChanged`].
/// The new game stands where the old one stood, written as [`pgn::write_game`] writes it.
/// Follows symbolic links and stages the new file beside the destination.
///
/// Refuses, and leaves the target as it was, when the file has no game with that number, when
/// the reader gives that game no range, and when the staged file does not read back with the
/// same number of games and the new game in that place.
pub fn replace_pgn_game(
    target: &Path,
    number: NonZeroU64,
    game: &Game,
    expected_game: Option<&Game>,
    history: Option<PgnHistory<'_>>,
) -> io::Result<Option<String>> {
    let mut text = Vec::new();
    pgn::write_game(game, &mut text)?;
    // The line break behind the old game stays in the file.
    text.pop();
    let target = SaveTarget::new(destination(target)?, history)?;
    // A pipe used as an external PGN must see EOF when its writer closes.
    let mut old = File::open(&target.path)?;
    let mut games = 0;
    let mut found = None;
    let mut expected_matches = expected_game.is_none();
    pgn::read_games(&old, |entry| {
        if let Entry::Game {
            range, game: held, ..
        } = entry
        {
            games += 1;
            if games == number.get() {
                found = Some(range);
                expected_matches = expected_game.is_none_or(|game| *held == *game);
            }
        }
    })?;
    if !expected_matches {
        return Err(PgnWriteConflict::GameChanged.into());
    }
    let range = match found {
        Some(Some(range)) => range,
        Some(None) => {
            return Err(refusal(format!(
                "game {number} has only tags or follows a game with only tags, \
                 so its place in the file is unknown"
            )));
        }
        None => {
            return Err(refusal(format!(
                "there is no game {number}, the file holds {games}"
            )));
        }
    };
    // Staging beside a file would otherwise bypass its write permissions.
    drop(File::options().write(true).open(&target.path)?);
    let staged = Staged::beside(&target.path, Some(&old))?;
    let mut output = BufWriter::new(&staged.file);
    old.rewind()?;
    io::copy(&mut (&old).take(range.start), &mut output)?;
    output.write_all(&text)?;
    old.seek(SeekFrom::Start(range.end))?;
    io::copy(&mut old, &mut output)?;
    output.flush()?;
    drop(output);

    // A new game without tags can merge with the game in front of it. The reader decides.
    (&staged.file).rewind()?;
    let mut staged_games = 0;
    let mut reads_back = false;
    pgn::read_games(&staged.file, |entry| {
        if let Entry::Game {
            game: read,
            problem,
            ..
        } = entry
        {
            staged_games += 1;
            if staged_games == number.get() {
                reads_back = problem.is_none() && *read == *game;
            }
        }
    })?;
    if staged_games != games || !reads_back {
        return Err(refusal(format!(
            "the file would not read back with the same number of games \
             and the new game as game {number}"
        )));
    }
    target.publish(staged)
}

/// Reads game `number` of a PGN file, counting from 1. `None` when the file holds fewer games.
/// A game with a problem comes as the part that the reader kept.
pub fn read_pgn_game(source: &Path, number: NonZeroU64) -> io::Result<Option<Game>> {
    let mut games = 0;
    let mut found = None;
    pgn::read_games(File::open(source)?, |entry| {
        if let Entry::Game { game, .. } = entry {
            games += 1;
            if games == number.get() {
                found = Some(*game);
            }
        }
    })?;
    Ok(found)
}

/// Reads the rows, problem count and version of the very same PGN byte stream.
pub fn list_pgn_games(path: &Path) -> io::Result<(Vec<PgnGameRow>, usize, String)> {
    let destination = path.canonicalize()?;
    let mut source = VersionReader::new(&destination, File::open(&destination)?);
    let mut rows = Vec::new();
    let mut problems = 0;
    pgn::read_games(&mut source, |entry| {
        if let Entry::Game { game, problem, .. } = entry {
            rows.push(PgnGameRow::new(rows.len() as u64 + 1, &game));
            problems += usize::from(problem.is_some());
        }
    })?;
    Ok((rows, problems, source.version()))
}

// Feed the hash fixed-size blocks so a short pipe read and a disk read have the same version.
struct VersionReader<R> {
    source: R,
    hash: DefaultHasher,
    checksum: crc32fast::Hasher,
    block: [u8; 256],
    filled: usize,
    length: u64,
}

impl<R> VersionReader<R> {
    fn new(path: &Path, source: R) -> Self {
        let mut hash = DefaultHasher::new();
        path.hash(&mut hash);
        Self {
            source,
            hash,
            checksum: crc32fast::Hasher::new(),
            block: [0; 256],
            filled: 0,
            length: 0,
        }
    }

    fn version(mut self) -> String {
        self.hash.write(&self.block[..self.filled]);
        format!(
            "{}-{:08x}-{:016x}",
            self.length,
            self.checksum.finalize(),
            self.hash.finish()
        )
    }
}

impl<R: Read> Read for VersionReader<R> {
    fn read(&mut self, output: &mut [u8]) -> io::Result<usize> {
        let count = self.source.read(output)?;
        let mut bytes = &output[..count];
        self.checksum.update(bytes);
        self.length += count as u64;
        while !bytes.is_empty() {
            let take = bytes.len().min(self.block.len() - self.filled);
            self.block[self.filled..self.filled + take].copy_from_slice(&bytes[..take]);
            self.filled += take;
            bytes = &bytes[take..];
            if self.filled == self.block.len() {
                self.hash.write(&self.block);
                self.filled = 0;
            }
        }
        Ok(count)
    }
}

pub(super) fn content_version(path: &Path, file: &mut File) -> io::Result<String> {
    file.rewind()?;
    let mut source = VersionReader::new(path, file);
    io::copy(&mut source, &mut io::sink())?;
    Ok(source.version())
}

pub(super) fn list_changed() -> io::Error {
    io::Error::other(
        "This file changed since the list was read. Reopen the file and choose the game again.",
    )
}

// The PGN payload of the captured state begins at the current file position.
pub(super) fn stage_without_game(
    path: &Path,
    number: NonZeroU64,
    expected: &str,
    mut snapshot: File,
    output: &File,
) -> io::Result<()> {
    let payload = snapshot.stream_position()?;
    let mut reader = VersionReader::new(path, &mut snapshot);
    let mut games = 0;
    let (mut previous, mut selected, mut following) = (None, None, None);
    pgn::read_games(&mut reader, |entry| {
        let range = match entry {
            Entry::Game { range, .. } => {
                games += 1;
                if games == number.get() {
                    selected = range;
                    return;
                }
                range
            }
            Entry::StrayText { range, .. } => Some(range),
        };
        if games < number.get() {
            previous = Some(range.map(|range| range.end));
        } else if following.is_none() {
            following = Some(range.map(|range| range.start));
        }
    })?;
    let length = reader.length;
    if reader.version() != expected {
        return Err(list_changed());
    }
    let unknown = || {
        io::Error::other(format!(
            "Could not locate game {number} safely in {}. Your PGN file is untouched.",
            path.display()
        ))
    };
    let game = selected.ok_or_else(unknown)?;
    let next = following.unwrap_or(Some(length)).ok_or_else(unknown)?;
    // Only a final entry owns the blank separator before it. Keep the previous entry's line ending.
    let prefix = if following.is_none() {
        previous.unwrap_or(Some(game.start)).ok_or_else(unknown)?
    } else {
        game.start
    };
    if prefix > game.start || game.start >= game.end || game.end > next || next > length {
        return Err(unknown());
    }
    snapshot.seek(SeekFrom::Start(payload))?;
    let mut staged = BufWriter::new(output);
    io::copy(&mut (&mut snapshot).take(prefix), &mut staged)?;
    copy_separator(
        &mut snapshot,
        &mut staged,
        prefix..game.start,
        true,
        unknown,
    )?;
    snapshot.seek(SeekFrom::Start(payload + game.end))?;
    copy_separator(&mut snapshot, &mut staged, game.end..next, false, unknown)?;
    io::copy(&mut snapshot, &mut staged)?;
    staged.flush()?;
    drop(staged);
    snapshot.seek(SeekFrom::Start(payload))?;
    if !surviving_games_match(snapshot, number, output)? {
        return Err(unknown());
    }
    Ok(())
}

fn surviving_games_match(
    original: File,
    removed: NonZeroU64,
    mut staged: &File,
) -> io::Result<bool> {
    staged.rewind()?;
    // Compare the two readers in order without holding a whole PGN's games in memory.
    std::thread::scope(|scope| {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let comparison = std::thread::Builder::new().spawn_scoped(scope, move || {
            let mut equal = true;
            pgn::read_games(staged, |entry| {
                if let Entry::Game { game, problem, .. } = entry {
                    equal &=
                        receive.recv().ok() == Some((game, problem.map(|problem| problem.reason)));
                }
            })?;
            Ok(equal && receive.recv().is_err())
        })?;
        let mut row = 0;
        let read = pgn::read_games(original, |entry| {
            if let Entry::Game { game, problem, .. } = entry {
                row += 1;
                if row != removed.get() {
                    let _sent = send.send((game, problem.map(|problem| problem.reason)));
                }
            }
        });
        drop(send);
        read?;
        comparison.join().unwrap_or(Ok(false))
    })
}

fn copy_separator(
    file: &mut File,
    output: &mut BufWriter<&File>,
    gap: Range<u64>,
    before_game: bool,
    unknown: impl Fn() -> io::Error,
) -> io::Result<()> {
    let length = gap.end - gap.start;
    if length == 0 {
        return Ok(());
    }
    let start = file.stream_position()?;
    let mut bytes = BufReader::new(file.take(length)).bytes().peekable();
    let mut discard = 0..length;
    let mut offset = 0;
    let mut line_start = 0;
    let mut has_text = false;
    while let Some(byte) = bytes.next().transpose()? {
        offset += 1;
        match byte {
            b'\n' => {}
            b'\r' if before_game && line_start == 0 && !has_text => {
                // The preceding entry keeps its own line ending, including a lone CR.
                offset += u64::from(bytes.next_if(|next| matches!(next, Ok(b'\n'))).is_some());
            }
            b' ' | b'\t' | b'\r' => continue,
            _ if before_game => {
                has_text = true;
                continue;
            }
            _ => {
                if byte == b'%' && output.stream_position()? != 0 {
                    output.seek(SeekFrom::Current(-1))?;
                    let mut previous = [0];
                    output.get_mut().read_exact(&mut previous)?;
                    if previous != *b"\n" {
                        return Err(unknown());
                    }
                }
                discard.end = line_start;
                break;
            }
        }
        if before_game && (line_start == 0 || has_text) {
            discard.start = offset;
        }
        line_start = offset;
        has_text = false;
    }
    if before_game && (line_start == 0 || has_text) {
        // The gap can end with a byte order mark directly before the selected game's first tag.
        discard.end = line_start;
    }
    file.seek(SeekFrom::Start(start))?;
    io::copy(&mut file.take(discard.start), output)?;
    file.seek(SeekFrom::Start(start + discard.end))?;
    io::copy(&mut file.take(length - discard.end), output)?;
    Ok(())
}

/// What a list shows of one game of a PGN file.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgnGameRow {
    /// The number of the game in its file, counting from 1.
    pub number: u64,
    /// The `White` tag.
    pub white: String,
    /// The `Black` tag.
    pub black: String,
    /// The result of the game, as its result marker gives it.
    pub result: String,
    /// The `Date` tag.
    pub date: String,
    /// The `Event` tag.
    pub event: String,
}

impl PgnGameRow {
    /// Builds the row of a game. A missing tag is `?`. A line break or a tab in a tag becomes a
    /// space, so that every value stays in one field.
    #[must_use]
    pub fn new(number: u64, game: &Game) -> Self {
        let tag = |name| {
            game.tag(name)
                .unwrap_or("?")
                .replace("\r\n", " ")
                .replace(['\r', '\n', '\t'], " ")
        };
        Self {
            number,
            white: tag("White"),
            black: tag("Black"),
            result: game.result().to_string(),
            date: tag("Date"),
            event: tag("Event"),
        }
    }
}

/// Follows symbolic links to the file that a target names.
fn destination(target: &Path) -> io::Result<PathBuf> {
    let mut target = std::path::absolute(target)?;
    loop {
        match target.canonicalize() {
            Ok(destination) => return Ok(destination),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        // A link can point to a file that does not exist yet.
        match target.read_link() {
            Ok(destination) => target.set_file_name(destination),
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(target),
            Err(error) => return Err(error),
        }
    }
}

// Write access makes a target that must not be written fail here. A rename over it would pass.
pub(super) fn open(destination: &Path) -> io::Result<File> {
    File::options().read(true).write(true).open(destination)
}

/// The next version of a PGN file, written beside its destination and published whole.
pub(super) struct Staged {
    pub(super) write: Arc<PendingWrite>,
    pub(super) file: File,
    path: PathBuf,
    destination: PathBuf,
    _directory: Arc<StagingDirectory>,
}

impl Staged {
    fn for_append(target: &Path, write: &Arc<PendingWrite>) -> io::Result<(Self, &'static [u8])> {
        let old = match open(target) {
            Ok(old) => Some(old),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error),
        };
        let mut staged = Staged::beside_for_task(target, old.as_ref(), write)?;
        let mut separator: &[u8] = b"";
        if let Some(mut old) = old {
            let length = io::copy(&mut old, &mut staged.file)?;
            if length > 0 {
                let mut tail = [0; 3];
                let start = length.saturating_sub(3);
                staged.file.seek(SeekFrom::Start(start))?;
                let tail = &mut tail[..usize::try_from(length - start).unwrap_or(3)];
                staged.file.read_exact(tail)?;
                separator = if tail.ends_with(b"\n\n") || tail.ends_with(b"\n\r\n") {
                    b""
                } else if tail.ends_with(b"\n") {
                    b"\n"
                } else {
                    b"\n\n"
                };
            }
        }
        Ok((staged, separator))
    }

    pub(super) fn beside(destination: &Path, old: Option<&File>) -> io::Result<Staged> {
        Self::beside_for_task(destination, old, &Arc::default())
    }

    fn beside_for_task(
        destination: &Path,
        old: Option<&File>,
        write: &Arc<PendingWrite>,
    ) -> io::Result<Staged> {
        write.stage(destination, |directory| {
            let path = directory.0.join("games.pgn");
            let file = File::create_new(&path)?;
            if let Some(old) = old {
                file.set_permissions(old.metadata()?.permissions())?;
            }
            Ok(Staged {
                write: Arc::clone(write),
                file,
                path,
                destination: destination.to_owned(),
                _directory: directory,
            })
        })
    }

    pub(super) fn publish(self) -> io::Result<()> {
        self.publish_if(|| Ok(()))
    }

    pub(super) fn publish_if(
        self,
        check_destination: impl FnOnce() -> io::Result<()>,
    ) -> io::Result<()> {
        drop(self.file);
        check_destination()?;
        self.write.begin_publication()?;
        fs::rename(self.path, self.destination)
    }
}

fn failed_write() -> io::Error {
    io::Error::other("a previous write to this PGN file failed")
}

fn refusal(reason: String) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, reason)
}
