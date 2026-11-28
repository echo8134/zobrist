use std::fs::{self, File, TryLockError};
use std::io::{self, BufRead, BufReader, ErrorKind, Read, Seek, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::pgn_file::{Staged, content_version, list_changed, open, stage_without_game};
use crate::{PgnWriteConflict, StagingDirectory, copy_with_checksum, read_bytes};

const MAGIC: &[u8; 8] = b"ZPGNHST\x01";
const HEADER_BYTES: u64 = 28;
const LIMIT: usize = 20;

/// Where an app write keeps its previous bytes, and when the player made the change.
#[derive(Clone, Copy)]
pub struct PgnHistory<'a> {
    /// The application's local data directory.
    pub root: &'a Path,
    /// Seconds since the Unix epoch, in UTC.
    pub captured_at: u64,
}

/// An earlier state in the history of one PGN path.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgnState {
    /// Stable number within the file's history.
    pub number: u64,
    /// Absent when the state header cannot be read.
    pub details: Option<PgnStateDetails>,
}

/// The information shown when choosing a state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PgnStateDetails {
    /// Capture time in UTC seconds since the Unix epoch.
    pub captured_at: u64,
    /// Number of original PGN bytes.
    pub length: u64,
}

struct StateFolder {
    path: PathBuf,
    _owner_lock: File,
}

struct HistoryFailure {
    path: PathBuf,
    reason: io::Error,
}

impl HistoryFailure {
    fn contended(&self) -> bool {
        PgnWriteConflict::from_io_error(&self.reason) == Some(PgnWriteConflict::HistoryLocked)
    }

    fn listing_error(self) -> io::Error {
        if self.contended() {
            self.reason
        } else {
            list_error(&self.path)
        }
    }

    fn read_only(&self) -> bool {
        matches!(
            self.reason.kind(),
            ErrorKind::PermissionDenied | ErrorKind::ReadOnlyFilesystem
        )
    }

    fn detail(&self) -> String {
        if self.read_only() {
            "The history folder is read-only.".to_owned()
        } else {
            self.reason.to_string()
        }
    }

    fn warning(&self) -> String {
        format!(
            "Game saved. Could not keep the earlier state in {}. {}",
            self.path.display(),
            self.detail()
        )
    }

    fn change_error(self, change: ProtectedChange) -> io::Error {
        if self.contended() {
            return self.reason;
        }
        let advice = if self.read_only() {
            " Allow writing to this folder and try again."
        } else {
            " Check the history folder and try again."
        };
        io::Error::new(
            self.reason.kind(),
            format!(
                "Could not {change}. The current bytes could not be kept in {}. {}{advice}",
                self.path.display(),
                self.detail()
            ),
        )
    }
}

fn list_error(path: &Path) -> io::Error {
    io::Error::other(format!(
        "Could not read {}. Your PGN files are untouched. Check the history folder and reopen Earlier states.",
        path.display()
    ))
}

fn state_error(path: &Path) -> io::Error {
    io::Error::new(
        ErrorKind::InvalidData,
        format!(
            "Could not read {}. Your PGN files are untouched. Restore this state file from a backup or choose another state.",
            path.display()
        ),
    )
}

impl StateFolder {
    fn open(root: &Path, destination: &Path, create: bool) -> Result<Option<Self>, HistoryFailure> {
        let parent = root.join("pgn-history");
        let owner = destination.as_os_str().as_encoded_bytes();
        let key = format!("{:08x}", crc32fast::hash(owner));
        let deadline = Instant::now() + Duration::from_secs(5);
        for collision in 0_u64.. {
            let name = if collision == 0 {
                key.clone()
            } else {
                format!("{key}-{collision}")
            };
            let path = parent.join(name);
            let failed = |reason| HistoryFailure {
                path: path.clone(),
                reason,
            };
            if !path.try_exists().map_err(failed)? {
                if !create {
                    return Ok(None);
                }
                fs::create_dir_all(&parent).map_err(failed)?;
                let staging = StagingDirectory::beside(&path).map_err(failed)?;
                let mut identity = File::create_new(staging.0.join("owner")).map_err(failed)?;
                identity.write_all(owner).map_err(failed)?;
                identity.sync_all().map_err(failed)?;
                drop(identity);
                // Racing creators publish a whole folder, including the immutable owner.
                if let Err(error) = fs::rename(&staging.0, &path)
                    && !path.is_dir()
                {
                    return Err(failed(error));
                }
            }
            let owner_path = path.join("owner");
            let unreadable_owner = |reason| HistoryFailure {
                path: owner_path.clone(),
                reason,
            };
            if !fs::symlink_metadata(&owner_path)
                .map_err(unreadable_owner)?
                .is_file()
            {
                return Err(unreadable_owner(ErrorKind::InvalidData.into()));
            }
            let mut identity = File::open(&owner_path).map_err(unreadable_owner)?;
            loop {
                match identity.try_lock() {
                    Ok(()) => break,
                    Err(TryLockError::WouldBlock) => {}
                    Err(TryLockError::Error(error)) if error.kind() == ErrorKind::Interrupted => {}
                    Err(TryLockError::Error(error)) => return Err(failed(error)),
                }
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    return Err(failed(PgnWriteConflict::HistoryLocked.into()));
                }
                std::thread::sleep(remaining.min(Duration::from_millis(25)));
            }
            let mut stored_owner = Vec::new();
            (&mut identity)
                .take(owner.len() as u64 + 1)
                .read_to_end(&mut stored_owner)
                .map_err(unreadable_owner)?;
            if stored_owner == owner {
                return Ok(Some(Self {
                    path,
                    _owner_lock: identity,
                }));
            }
        }
        Err(HistoryFailure {
            path: parent,
            reason: io::Error::other("No history folder is available."),
        })
    }

    fn numbers(&self) -> io::Result<Vec<u64>> {
        let mut numbers = Vec::new();
        for entry in fs::read_dir(&self.path)? {
            let name = entry?.file_name();
            if let Some(number) = name
                .to_str()
                .and_then(|name| name.strip_suffix(".state"))
                .filter(|number| matches!(number.as_bytes().first(), Some(b'1'..=b'9')))
                .and_then(|number| number.parse::<u64>().ok())
            {
                numbers.push(number);
            }
        }
        numbers.sort_unstable_by(|a, b| b.cmp(a));
        Ok(numbers)
    }

    fn state(&self, number: u64) -> PathBuf {
        self.path.join(format!("{number}.state"))
    }

    fn capture(&self, old: &mut File, captured_at: u64) -> io::Result<Capture> {
        if self.path.metadata()?.permissions().readonly() {
            return Err(ErrorKind::PermissionDenied.into());
        }
        let previous = self.numbers()?;
        let number = previous
            .first()
            .copied()
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| io::Error::other("No state number is available."))?;
        let path = self.state(number);
        let staging = StagingDirectory::beside(&path)?;
        let temporary = staging.0.join("state");
        let mut state = File::create_new(&temporary)?;
        state.write_all(MAGIC)?;
        state.write_all(&captured_at.to_le_bytes())?;
        state.write_all(&[0; 12])?;
        old.rewind()?;
        let (length, checksum) = copy_with_checksum(old, &state)?;
        state.seek(io::SeekFrom::Start(16))?;
        state.write_all(&length.to_le_bytes())?;
        state.write_all(&checksum.to_le_bytes())?;
        state.sync_all()?;
        drop(state);
        fs::rename(temporary, &path)?;
        let capture = Capture {
            path,
            obsolete: previous
                .into_iter()
                .skip(LIMIT - 1)
                .map(|n| self.state(n))
                .collect(),
        };
        #[cfg(unix)]
        if let Err(error) = File::open(&self.path).and_then(|folder| folder.sync_all()) {
            fs::remove_file(&capture.path)?;
            return Err(error);
        }
        Ok(capture)
    }
}

struct Capture {
    path: PathBuf,
    obsolete: Vec<PathBuf>,
}

impl Capture {
    fn finish(self, publication: io::Result<()>) -> io::Result<()> {
        if let Err(error) = publication {
            fs::remove_file(self.path)?;
            return Err(error);
        }
        // A failed prune can be retried on the next write. The PGN has been published.
        for path in self.obsolete {
            fs::remove_file(path).ok();
        }
        Ok(())
    }
}

fn open_state(path: &Path) -> io::Result<File> {
    if !fs::symlink_metadata(path)?.is_file() {
        return Err(ErrorKind::InvalidData.into());
    }
    File::open(path)
}

fn read_header(file: &mut File) -> io::Result<(PgnStateDetails, u32)> {
    if &read_bytes::<8>(file)? != MAGIC {
        return Err(ErrorKind::InvalidData.into());
    }
    let captured_at = u64::from_le_bytes(read_bytes(file)?);
    let length = u64::from_le_bytes(read_bytes(file)?);
    let checksum = u32::from_le_bytes(read_bytes(file)?);
    // The displayed UTC date has four year digits.
    if captured_at > 253_402_300_799
        || file.metadata()?.len().checked_sub(HEADER_BYTES) != Some(length)
    {
        return Err(ErrorKind::InvalidData.into());
    }
    Ok((
        PgnStateDetails {
            captured_at,
            length,
        },
        checksum,
    ))
}

fn read_state(path: &Path, mut target: impl Write) -> io::Result<()> {
    let invalid = |_| state_error(path);
    let mut file = open_state(path).map_err(invalid)?;
    let (details, checksum) = read_header(&mut file).map_err(invalid)?;
    let mut payload = BufReader::new(file);
    let mut hash = crc32fast::Hasher::new();
    let mut length = 0;
    loop {
        let bytes = payload.fill_buf().map_err(invalid)?;
        let count = bytes.len();
        if count == 0 {
            break;
        }
        target.write_all(bytes)?;
        hash.update(bytes);
        length += count as u64;
        payload.consume(count);
    }
    if (length, hash.finalize()) != (details.length, checksum) {
        return Err(state_error(path));
    }
    Ok(())
}

pub(super) struct SaveTarget {
    pub(super) path: PathBuf,
    history: Option<(u64, Result<StateFolder, HistoryFailure>)>,
}

impl SaveTarget {
    pub(super) fn new(path: PathBuf, history: Option<PgnHistory<'_>>) -> io::Result<Self> {
        let history = history
            .map(|history| {
                let folder = StateFolder::open(history.root, &path, true).and_then(|folder| {
                    folder.ok_or_else(|| HistoryFailure {
                        path: history.root.join("pgn-history"),
                        reason: io::Error::from(ErrorKind::NotFound),
                    })
                });
                match folder {
                    Err(failure) if failure.contended() => Err(failure.reason),
                    folder => Ok((history.captured_at, folder)),
                }
            })
            .transpose()?;
        Ok(Self { path, history })
    }

    pub(super) fn publish(self, staged: Staged) -> io::Result<Option<String>> {
        let Some((time, folder)) = self.history else {
            staged.publish()?;
            return Ok(None);
        };
        let old = File::open(&self.path);
        if old
            .as_ref()
            .is_err_and(|error| error.kind() == ErrorKind::NotFound)
        {
            staged.publish()?;
            return Ok(None);
        }
        if let Ok(file) = &old
            && equal_bytes(file, &staged.file).unwrap_or(false)
        {
            return Ok(None);
        }
        let captured = match &folder {
            Ok(folder) => old
                .and_then(|mut file| folder.capture(&mut file, time))
                .map_err(|reason| {
                    HistoryFailure {
                        path: folder.path.clone(),
                        reason,
                    }
                    .warning()
                }),
            Err(failure) => Err(failure.warning()),
        };
        match captured {
            Ok(capture) => {
                capture.finish(staged.publish())?;
                Ok(None)
            }
            Err(warning) => {
                staged.publish()?;
                Ok(Some(warning))
            }
        }
    }
}

fn equal_bytes(mut left: &File, mut right: &File) -> io::Result<bool> {
    let mut remaining = left.metadata()?.len();
    if remaining != right.metadata()?.len() {
        return Ok(false);
    }
    left.rewind()?;
    right.rewind()?;
    let mut first = [0; 8192];
    let mut second = [0; 8192];
    while remaining > 0 {
        let size = usize::try_from(remaining)
            .unwrap_or(first.len())
            .min(first.len());
        left.read_exact(&mut first[..size])?;
        right.read_exact(&mut second[..size])?;
        if first[..size] != second[..size] {
            return Ok(false);
        }
        remaining -= size as u64;
    }
    Ok(true)
}

/// Lists a PGN's earlier states, newest first. Opening the list never creates a history.
pub fn pgn_states(target: &Path, root: &Path) -> io::Result<Vec<PgnState>> {
    let target = target.canonicalize()?;
    let Some(folder) =
        StateFolder::open(root, &target, false).map_err(HistoryFailure::listing_error)?
    else {
        return Ok(Vec::new());
    };
    Ok(folder
        .numbers()
        .map_err(|_| list_error(&folder.path))?
        .into_iter()
        .take(LIMIT)
        .map(|number| {
            let details = open_state(&folder.state(number))
                .and_then(|mut file| read_header(&mut file))
                .ok()
                .map(|(details, _)| details);
            PgnState { number, details }
        })
        .collect())
}

fn stamp(path: &Path, file: &mut File) -> io::Result<String> {
    file.rewind()?;
    let (length, checksum) = copy_with_checksum(&mut *file, io::sink())?;
    Ok(format!(
        "{length}-{checksum}-{:?}-{}",
        file.metadata()?.modified()?,
        path.display()
    ))
}

/// Verifies the chosen state and returns an opaque stamp of the PGN shown for confirmation.
pub fn prepare_pgn_restore(target: &Path, root: &Path, number: u64) -> io::Result<String> {
    let target = target.canonicalize()?;
    let folder = StateFolder::open(root, &target, false)
        .map_err(HistoryFailure::listing_error)?
        .ok_or_else(|| list_error(&root.join("pgn-history")))?;
    read_state(&folder.state(number), io::sink())?;
    stamp(&target, &mut File::open(&target)?)
}

#[derive(Clone, Copy)]
enum ProtectedChange {
    Restore(u64),
    Delete(NonZeroU64),
}

impl std::fmt::Display for ProtectedChange {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Restore(number) => write!(formatter, "restore State {number}"),
            Self::Delete(number) => write!(formatter, "delete game {number}"),
        }
    }
}

impl ProtectedChange {
    fn changed(self) -> io::Error {
        match self {
            Self::Restore(_) => io::Error::other("This file changed. Choose the state again."),
            Self::Delete(_) => list_changed(),
        }
    }

    fn version(self, path: &Path, file: &mut File) -> io::Result<String> {
        match self {
            Self::Restore(_) => stamp(path, file),
            Self::Delete(_) => content_version(path, file),
        }
    }

    fn apply(self, path: &Path, history: PgnHistory<'_>, expected: &str) -> io::Result<PathBuf> {
        let deleting = matches!(self, Self::Delete(_));
        let target = path.canonicalize().map_err(|error| {
            if deleting && error.kind() == ErrorKind::NotFound {
                self.changed()
            } else {
                error
            }
        })?;
        let folder = StateFolder::open(history.root, &target, deleting)
            .map_err(|error| error.change_error(self))?
            .ok_or_else(|| list_error(&history.root.join("pgn-history")))?;
        let mut current = open(&target).map_err(|error| {
            if deleting && error.kind() == ErrorKind::NotFound {
                self.changed()
            } else {
                error
            }
        })?;
        if self.version(&target, &mut current)? != expected {
            return Err(self.changed());
        }
        let staged = Staged::beside(target.clone(), Some(&current))?;
        if let Self::Restore(number) = self {
            read_state(&folder.state(number), &staged.file)?;
            if equal_bytes(&current, &staged.file)? {
                return Ok(target);
            }
        }
        let capture = folder
            .capture(&mut current, history.captured_at)
            .map_err(|reason| {
                HistoryFailure {
                    path: folder.path.clone(),
                    reason,
                }
                .change_error(self)
            })?;
        let publication = (|| {
            if let Self::Delete(number) = self {
                // Read from the captured file so the list, range and surviving bytes describe one snapshot.
                let mut snapshot = open_state(&capture.path)?;
                read_header(&mut snapshot)?;
                stage_without_game(&target, number, expected, snapshot, &staged.file)?;
            }
            staged.publish_if(|| {
                let version = path
                    .canonicalize()
                    .and_then(|resolved| self.version(&resolved, &mut File::open(&resolved)?));
                if version.is_ok_and(|version| version == expected) {
                    Ok(())
                } else {
                    Err(self.changed())
                }
            })
        })();
        capture.finish(publication)?;
        Ok(target)
    }
}

/// Publishes the chosen bytes only after keeping the current PGN and checking the confirmation.
pub fn restore_pgn_state(
    target: &Path,
    history: PgnHistory<'_>,
    number: u64,
    expected: &str,
) -> io::Result<()> {
    ProtectedChange::Restore(number)
        .apply(target, history, expected)
        .map(drop)
}

/// Deletes one listed game after keeping the full PGN. Returns the resolved path that was changed.
pub fn delete_pgn_game(
    target: &Path,
    number: NonZeroU64,
    expected: &str,
    history: PgnHistory<'_>,
) -> io::Result<PathBuf> {
    ProtectedChange::Delete(number).apply(target, history, expected)
}
