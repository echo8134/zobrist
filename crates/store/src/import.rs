use std::cell::RefCell;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use zobrist_core::pgn::{self, Entry};

use crate::DatabaseWriter;

/// Totals for PGN entries read in source order.
#[derive(Default)]
pub struct PgnReport {
    /// Games kept, including games recovered up to a bad move.
    pub games: u64,
    /// Moves in the main lines of the games kept.
    pub main_line_moves: usize,
    /// Variations across all games.
    pub variations: usize,
    /// Comments across all games.
    pub comments: usize,
    /// Damaged games and runs of stray text.
    pub problems: usize,
}

/// An I/O error reading the PGN, or creating the database and its report.
#[derive(Debug)]
pub enum ImportError {
    /// Reading the source PGN failed.
    Read(io::Error),
    /// Creating the database or recording a diagnostic failed.
    Create(io::Error),
}

/// Reads the PGN once and publishes a new database after all games and diagnostics are written.
/// An existing destination is refused. `diagnostics` receives each problem as `Some` in source
/// order, then `None` to finish writing the report before the destination is published.
pub fn import_pgn(
    source: &Path,
    destination: &Path,
    mut diagnostics: impl FnMut(Option<String>) -> io::Result<()>,
) -> Result<PgnReport, ImportError> {
    let file = File::open(source).map_err(ImportError::Read)?;
    let mut writer = DatabaseWriter::create(destination).map_err(ImportError::Create)?;
    let mut report = PgnReport::default();
    read_pgn_entries(file, ImportError::Read, |entry| {
        if let Some(line) = report.count(&entry) {
            diagnostics(Some(line)).map_err(ImportError::Create)?;
        }
        if let Entry::Game { game, .. } = entry {
            writer.push(&game).map_err(ImportError::Create)?;
        }
        Ok(())
    })?;
    diagnostics(None).map_err(ImportError::Create)?;
    writer.finish().map_err(ImportError::Create)?;
    Ok(report)
}

/// Stops reading the PGN when its consumer fails, including when the input is a pipe.
/// `read_error` distinguishes an unreadable source from a failure in the consumer.
pub fn read_pgn_entries<E>(
    file: File,
    read_error: impl FnOnce(io::Error) -> E,
    mut each: impl FnMut(Entry) -> Result<(), E>,
) -> Result<(), E> {
    let input = PgnInput {
        file,
        failure: RefCell::new(None),
    };
    let read = pgn::read_games(&input, |entry| {
        if input.failure.borrow().is_none()
            && let Err(error) = each(entry)
        {
            input.failure.replace(Some(error));
        }
    });
    match input.failure.into_inner() {
        Some(error) => Err(error),
        None => read.map_err(read_error),
    }
}

struct PgnInput<E> {
    file: File,
    failure: RefCell<Option<E>>,
}

impl<E> Read for &PgnInput<E> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.failure.borrow().is_some() {
            Ok(0)
        } else {
            (&self.file).read(buffer)
        }
    }
}

impl PgnReport {
    /// Adds one entry to the totals and returns its diagnostic when present.
    pub fn count(&mut self, entry: &Entry) -> Option<String> {
        if let Entry::Game { game, .. } = entry {
            self.games += 1;
            let counts = game.counts();
            self.main_line_moves += counts.main_line_moves;
            self.variations += counts.variations;
            self.comments += counts.comments;
        }
        let line = problem_line(entry)?;
        self.problems += 1;
        Some(line)
    }
}

/// The diagnostic for a damaged game or stray text, including its place in the PGN.
#[must_use]
pub fn problem_line(entry: &Entry) -> Option<String> {
    match entry {
        Entry::Game { game, problem, .. } => problem.as_ref().map(|problem| {
            let kept = match game.counts().main_line_moves {
                1 => "1 move kept".to_owned(),
                moves => format!("{moves} moves kept"),
            };
            format!(
                "game {} ({} - {}): {}, {kept}",
                problem.game_number,
                game.tag("White").unwrap_or("?"),
                game.tag("Black").unwrap_or("?"),
                problem.reason,
            )
        }),
        Entry::StrayText { after_game, .. } => Some(format!(
            "after game {after_game}: text that is not part of a game"
        )),
    }
}
