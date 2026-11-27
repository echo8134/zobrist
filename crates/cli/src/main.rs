//! The `zobrist` command line.

use std::cell::RefCell;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Read, Seek, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

use clap::{Parser, Subcommand};
use zobrist_core::game::Game;
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{Database, DatabaseWriter, PgnGameRow, add_pgn_games, replace_pgn_game};

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
    /// Fetch one finished game from a public Lichess link and write it as PGN.
    Fetch {
        /// The public game link.
        link: String,
    },
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
    /// List the games in file order.
    List {
        /// The PGN file to read.
        file: PathBuf,
    },
    /// Add the games from a source PGN file to the end of a target PGN file.
    Add {
        /// The PGN file to extend or create.
        target: PathBuf,
        /// The PGN file to read.
        source: PathBuf,
    },
    /// Replace one game of a target PGN file with the one game of a source PGN file.
    Replace {
        /// The PGN file to change.
        target: PathBuf,
        /// The number of the game to replace, as `list` prints it.
        number: NonZeroU64,
        /// The PGN file that holds the new game.
        source: PathBuf,
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
    Add(PathBuf, io::Error),
    Replace(PathBuf, io::Error),
    Source(String),
    Diagnostic(io::Error),
    Write(io::Error),
}

impl From<io::Error> for Failure {
    fn from(error: io::Error) -> Self {
        Self::Write(error)
    }
}

fn main() -> ExitCode {
    // clap exits with status 2 on a usage error, which matches "the command could not run".
    match Arguments::parse().command {
        Command::Pgn { command } => match command {
            PgnCommand::Fetch { link } => fetch_game(&link),
            PgnCommand::Check { file } => exit_code(&file, report(&file)),
            PgnCommand::Write { file } => exit_code(&file, write_games(&file)),
            PgnCommand::List { file } => exit_code(&file, list_games(&file)),
            PgnCommand::Add { target, source } => exit_code(&source, add_games(&target, &source)),
            PgnCommand::Replace {
                target,
                number,
                source,
            } => exit_code(&source, replace_game(&target, number, &source)),
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
        Err(Failure::Write(error) | Failure::Diagnostic(error)) => cannot_write_output(&error),
        Err(Failure::Add(target, error)) => {
            writeln!(
                io::stderr(),
                "zobrist: cannot add games to {}: {error}",
                target.display()
            )
            .ok();
            ExitCode::from(2)
        }
        Err(Failure::Replace(target, error)) => {
            writeln!(
                io::stderr(),
                "zobrist: cannot replace a game of {}: {error}",
                target.display()
            )
            .ok();
            ExitCode::from(2)
        }
        Err(Failure::Source(reason)) => {
            writeln!(
                io::stderr(),
                "zobrist: cannot replace a game with {}: {reason}",
                path.display()
            )
            .ok();
            ExitCode::from(2)
        }
    }
}

fn cannot_write_output(error: &io::Error) -> ExitCode {
    writeln!(io::stderr(), "zobrist: cannot write output: {error}").ok();
    ExitCode::from(2)
}

fn fetch_game(link: &str) -> ExitCode {
    let game = match zobrist_online::fetch_game(link) {
        Ok(game) => game,
        Err(refusal) => {
            writeln!(io::stderr(), "zobrist: {refusal}").ok();
            return ExitCode::from(2);
        }
    };
    let mut output = PgnOutput::new(io::stdout().lock());
    match output.write(&game).and_then(|()| output.output.flush()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => cannot_write_output(&error),
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

fn for_each_game(
    file: File,
    mut each: impl FnMut(&Game) -> io::Result<()>,
) -> Result<usize, Failure> {
    let mut problems = 0;
    PgnInput::for_each(file, |entry| {
        if let Some(line) = problem_line(&entry) {
            writeln!(io::stderr().lock(), "{line}").map_err(Failure::Diagnostic)?;
            problems += 1;
        }
        if let Entry::Game { game, .. } = entry {
            each(&game)?;
        }
        Ok(())
    })?;
    Ok(problems)
}

fn write_games(path: &Path) -> Result<usize, Failure> {
    let file = File::open(path).map_err(Failure::Read)?;
    let mut output = PgnOutput::new(io::stdout().lock());
    let read = for_each_game(file, |game| output.write(game));
    output.output.flush().map_err(Failure::Write)?;
    read
}

fn list_games(path: &Path) -> Result<usize, Failure> {
    let file = File::open(path).map_err(Failure::Read)?;
    let mut output = io::stdout().lock();
    let mut number = 0_u64;
    let read = for_each_game(file, |game| {
        number += 1;
        let row = PgnGameRow::new(number, game);
        writeln!(
            output,
            "{number}\t{}\t{}\t{}\t{}\t{}",
            row.white, row.black, row.result, row.date, row.event
        )
    });
    output.flush().map_err(Failure::Write)?;
    read
}

fn add_games(target: &Path, source: &Path) -> Result<usize, Failure> {
    let file = File::open(source).map_err(Failure::Read)?;
    let mut problems = 0;
    let (added, _) = add_pgn_games(target, None, |append| {
        problems = for_each_game(file, append)?;
        Ok(())
    })
    .map_err(|failure| {
        if let Failure::Write(error) = failure {
            Failure::Add(target.to_owned(), error)
        } else {
            failure
        }
    })?;
    writeln!(io::stdout().lock(), "games added: {added}").map_err(Failure::Write)?;
    Ok(problems)
}

fn replace_game(target: &Path, number: NonZeroU64, source: &Path) -> Result<usize, Failure> {
    let file = File::open(source).map_err(Failure::Read)?;
    let mut games = 0_u64;
    let mut first = None;
    let problems = for_each_game(file, |game| {
        games += 1;
        first.get_or_insert_with(|| game.clone());
        Ok(())
    })?;
    let game = match (first, games, problems) {
        (Some(game), 1, 0) => game,
        (None, ..) => return Err(Failure::Source("it holds no game".to_owned())),
        (Some(_), 1, _) => return Err(Failure::Source("it has a problem".to_owned())),
        (Some(_), ..) => {
            return Err(Failure::Source(format!(
                "it holds {games} games, and a replacement is one game"
            )));
        }
    };
    replace_pgn_game(target, number, &game, None, None)
        .map_err(|error| Failure::Replace(target.to_owned(), error))?;
    writeln!(io::stdout().lock(), "game replaced: {number}").map_err(Failure::Write)?;
    Ok(0)
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
