//! The `zobrist` command line.

use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

use clap::{Parser, Subcommand};
use zobrist_core::game::Game;
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{Database, DatabaseWriter};

/// A personal chess database.
#[derive(Parser)]
#[command(name = "zobrist")]
struct Arguments {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Work with PGN files.
    Pgn {
        #[command(subcommand)]
        command: PgnCommand,
    },
    /// Work with game databases.
    Database {
        #[command(subcommand)]
        command: DatabaseCommand,
    },
}

#[derive(Subcommand)]
enum PgnCommand {
    /// Read every game of a PGN file and report what was read and what could not be.
    Check {
        /// The PGN file to read.
        file: PathBuf,
    },
    /// Write the games as PGN and report problems on standard error.
    Write {
        /// The PGN file to read.
        file: PathBuf,
    },
}

#[derive(Subcommand)]
enum DatabaseCommand {
    /// Store every game of a PGN file and print the import report.
    Create {
        /// The PGN file to read.
        pgn: PathBuf,
        /// The database file to create.
        database: PathBuf,
    },
    /// Write the games in a database as PGN.
    Export {
        /// The database file to read.
        database: PathBuf,
    },
}

const MAX_KEPT_PROBLEM_LINES: usize = 10_000;

#[derive(Default)]
struct Report {
    games: u64,
    main_line_moves: usize,
    variations: usize,
    comments: usize,
    problems: usize,
}

enum Failure {
    Read(io::Error),
    Create(PathBuf, io::Error),
    Write(io::Error),
}

fn main() -> ExitCode {
    // clap exits with status 2 on a usage error, which matches "the command could not run".
    match Arguments::parse().command {
        Command::Pgn { command } => match command {
            PgnCommand::Check { file } => exit_code(&file, report(&file)),
            PgnCommand::Write { file } => exit_code(&file, write_games(&file)),
        },
        Command::Database { command } => match command {
            DatabaseCommand::Create { pgn, database } => exit_code(&pgn, create(&pgn, &database)),
            DatabaseCommand::Export { database } => exit_code(&database, export(&database)),
        },
    }
}

fn exit_code(path: &Path, result: Result<usize, Failure>) -> ExitCode {
    match result {
        Ok(0) => ExitCode::SUCCESS,
        Ok(_) => ExitCode::from(1),
        Err(Failure::Read(error)) => {
            writeln!(
                io::stderr(),
                "zobrist: cannot read {}: {error}",
                path.display()
            )
            .ok();
            ExitCode::from(2)
        }
        Err(Failure::Create(database, error)) => {
            writeln!(
                io::stderr(),
                "zobrist: cannot create {}: {error}",
                database.display()
            )
            .ok();
            ExitCode::from(2)
        }
        Err(Failure::Write(error)) => {
            writeln!(io::stderr(), "zobrist: cannot write output: {error}").ok();
            ExitCode::from(2)
        }
    }
}

fn report(path: &Path) -> Result<usize, Failure> {
    let mut file = File::open(path).map_err(Failure::Read)?;
    // A pipe cannot be rewound, so keep its problem lines until the totals are known.
    let max_kept_lines = if file.metadata().map_err(Failure::Read)?.is_file() {
        MAX_KEPT_PROBLEM_LINES
    } else {
        usize::MAX
    };
    let mut report = Report::default();
    let mut lines = Vec::new();
    pgn::read_games(&file, |entry| {
        if let Some(line) = report.count(&entry)
            && lines.len() < max_kept_lines
        {
            lines.push(line);
        }
    })
    .map_err(Failure::Read)?;
    let mut out = io::stdout().lock();
    report.print_totals(&mut out).map_err(Failure::Write)?;
    if lines.len() == report.problems {
        for line in lines {
            writeln!(out, "{line}").map_err(Failure::Write)?;
        }
    } else {
        file.rewind().map_err(Failure::Read)?;
        let mut written = Ok(());
        pgn::read_games(&file, |entry| {
            if written.is_ok()
                && let Some(line) = problem_line(&entry)
            {
                written = writeln!(out, "{line}");
            }
        })
        .map_err(Failure::Read)?;
        written.map_err(Failure::Write)?;
    }
    Ok(report.problems)
}

// Once a consumer fails, stop the PGN reader at its next refill. A pipe can stay open
// indefinitely, and returning from the import lets its temporary files be removed.
struct PgnInput {
    file: File,
    failure: RefCell<Option<Failure>>,
}

impl PgnInput {
    fn for_each(
        file: File,
        mut each: impl FnMut(Entry) -> Result<(), Failure>,
    ) -> Result<(), Failure> {
        let input = Self {
            file,
            failure: RefCell::default(),
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
            None => read.map_err(Failure::Read),
        }
    }
}

impl Read for &PgnInput {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.failure.borrow().is_some() {
            Ok(0)
        } else {
            (&self.file).read(buffer)
        }
    }
}

struct PgnOutput<W> {
    output: W,
    encoded: Vec<u8>,
    first_game: bool,
}

impl<W: Write> PgnOutput<W> {
    fn new(output: W) -> Self {
        Self {
            output,
            encoded: Vec::new(),
            first_game: true,
        }
    }

    fn write(&mut self, game: &Game) -> io::Result<()> {
        self.encoded.clear();
        if !self.first_game {
            self.encoded.push(b'\n');
        }
        pgn::write_game(game, &mut self.encoded)?;
        self.output.write_all(&self.encoded)?;
        self.first_game = false;
        Ok(())
    }
}

fn write_games(path: &Path) -> Result<usize, Failure> {
    let file = File::open(path).map_err(Failure::Read)?;
    let mut output = PgnOutput::new(io::stdout().lock());
    let mut problems = 0;
    let read = PgnInput::for_each(file, |entry| {
        if let Some(line) = problem_line(&entry) {
            writeln!(io::stderr().lock(), "{line}").map_err(Failure::Write)?;
            problems += 1;
        }
        if let Entry::Game { game, .. } = entry {
            output.write(&game).map_err(Failure::Write)?;
        }
        Ok(())
    });
    output.output.flush().map_err(Failure::Write)?;
    read?;
    Ok(problems)
}

fn create(source: &Path, database: &Path) -> Result<usize, Failure> {
    let file = File::open(source).map_err(Failure::Read)?;
    let cannot_create = |error| Failure::Create(database.to_owned(), error);
    let mut writer = DatabaseWriter::create(database).map_err(cannot_create)?;
    let mut report = Report::default();
    let mut problems = None;
    PgnInput::for_each(file, |entry| {
        if let Some(line) = report.count(&entry) {
            let log = match &mut problems {
                Some(log) => log,
                None => problems.insert(ProblemLog::create(database).map_err(cannot_create)?),
            };
            writeln!(log.file, "{line}").map_err(cannot_create)?;
        }
        if let Entry::Game { game, .. } = entry {
            writer.push(&game).map_err(cannot_create)?;
        }
        Ok(())
    })?;
    if let Some(log) = &mut problems {
        log.file.rewind().map_err(cannot_create)?;
    }
    writer.finish().map_err(cannot_create)?;
    let mut output = io::stdout().lock();
    report.print_totals(&mut output).map_err(Failure::Write)?;
    if let Some(log) = &mut problems {
        io::copy(log.file.get_mut(), &mut output).map_err(Failure::Write)?;
    }
    output.flush().map_err(Failure::Write)?;
    Ok(report.problems)
}

// The report starts with totals, so even a non-seekable source needs somewhere to keep
// every problem line until the last game has been read.
struct ProblemLog {
    file: BufWriter<File>,
    path: PathBuf,
}

impl ProblemLog {
    fn create(database: &Path) -> io::Result<Self> {
        let mut options = OpenOptions::new();
        options.read(true).write(true).create_new(true);
        #[cfg(unix)]
        std::os::unix::fs::OpenOptionsExt::mode(&mut options, 0o600);
        for sequence in 0_u64.. {
            let name = format!(".zobrist-problems-{}-{sequence}", process::id());
            if database
                .file_name()
                .is_some_and(|target| target.eq_ignore_ascii_case(name.as_str()))
            {
                continue;
            }
            let path = database.with_file_name(name);
            match options.open(&path) {
                Ok(file) => {
                    return Ok(Self {
                        file: BufWriter::new(file),
                        path,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => (),
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::other("no temporary file name available"))
    }
}

impl Drop for ProblemLog {
    fn drop(&mut self) {
        fs::remove_file(&self.path).ok();
    }
}

fn export(path: &Path) -> Result<usize, Failure> {
    let cannot_read = |error| Failure::Read(io::Error::other(error));
    let mut database = Database::open(path).map_err(cannot_read)?;
    let mut output = PgnOutput::new(io::stdout().lock());
    for game in database.games().map_err(cannot_read)? {
        let game = game.map_err(cannot_read)?;
        output.write(&game).map_err(Failure::Write)?;
    }
    output.output.flush().map_err(Failure::Write)?;
    Ok(0)
}

impl Report {
    fn count(&mut self, entry: &Entry) -> Option<String> {
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

    fn print_totals(&self, mut out: impl Write) -> io::Result<()> {
        writeln!(out, "games: {}", self.games)?;
        writeln!(out, "main line moves: {}", self.main_line_moves)?;
        writeln!(out, "variations: {}", self.variations)?;
        writeln!(out, "comments: {}", self.comments)?;
        writeln!(out, "problems: {}", self.problems)
    }
}

fn problem_line(entry: &Entry) -> Option<String> {
    match entry {
        Entry::Game { game, problem } => problem.as_ref().map(|problem| {
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
        Entry::StrayText { after_game } => Some(format!(
            "after game {after_game}: text that is not part of a game"
        )),
    }
}
