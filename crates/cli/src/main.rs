//! The `zobrist` command line.

use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Seek, Write};
use std::num::NonZeroU64;
use std::path::{Path, PathBuf};
use std::process::{self, ExitCode};

use clap::{Parser, Subcommand};
use zobrist_core::game::Game;
use zobrist_core::pgn::{self, Entry};
use zobrist_store::{
    Database, ImportError, PgnGameRow, PgnReport, add_pgn_games, import_pgn, problem_line,
    read_pgn_entries, replace_pgn_game,
};

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
    /// Append a player's public Lichess games from an inclusive UTC date range to a PGN.
    DownloadPlayer {
        /// The public Lichess username.
        username: String,
        /// First UTC calendar date to include.
        #[arg(long, value_name = "YYYY-MM-DD")]
        from: String,
        /// Last UTC calendar date to include.
        #[arg(long, value_name = "YYYY-MM-DD")]
        to: String,
        /// The PGN file to extend or create.
        destination: PathBuf,
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
    /// Print how many games a database holds.
    Info {
        /// The database file to inspect.
        database: PathBuf,
    },
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
            PgnCommand::DownloadPlayer {
                username,
                from,
                to,
                destination,
            } => match zobrist_online::download_player_games(&username, &from, &to, &destination) {
                Ok(count) => report_added(count)
                    .map_or_else(|error| cannot_write_output(&error), |()| ExitCode::SUCCESS),
                Err(refusal) => online_refusal(refusal),
            },
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
            DatabaseCommand::Info { database } => exit_code(&database, info(&database)),
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

fn online_refusal(refusal: impl std::fmt::Display) -> ExitCode {
    writeln!(io::stderr(), "zobrist: {refusal}").ok();
    ExitCode::from(2)
}

fn fetch_game(link: &str) -> ExitCode {
    let game = match zobrist_online::fetch_game(link) {
        Ok(game) => game,
        Err(refusal) => return online_refusal(refusal),
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
    let mut report = PgnReport::default();
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
    print_totals(&report, &mut out).map_err(Failure::Write)?;
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
    read_pgn_entries(file, Failure::Read, |entry| {
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
    report_added(added).map_err(Failure::Write)?;
    Ok(problems)
}

fn report_added(added: u64) -> io::Result<()> {
    writeln!(io::stdout().lock(), "games added: {added}")
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
    let mut problems = None;
    let report = import_pgn(source, database, |line| {
        if let Some(line) = line {
            let log = match &mut problems {
                Some(log) => log,
                None => problems.insert(ProblemLog::create(database)?),
            };
            writeln!(log.file, "{line}")?;
        } else if let Some(log) = &mut problems {
            log.file.rewind()?;
        }
        Ok(())
    })
    .map_err(|error| match error {
        ImportError::Read(error) => Failure::Read(error),
        ImportError::Create(error) => Failure::Create(database.to_owned(), error),
    })?;
    let mut output = io::stdout().lock();
    print_totals(&report, &mut output).map_err(Failure::Write)?;
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

fn cannot_read(error: zobrist_store::OpenError) -> Failure {
    Failure::Read(io::Error::other(error))
}

fn info(path: &Path) -> Result<usize, Failure> {
    let database = Database::open(path).map_err(cannot_read)?;
    writeln!(io::stdout().lock(), "games: {}", database.game_count())?;
    Ok(0)
}

fn export(path: &Path) -> Result<usize, Failure> {
    let mut database = Database::open(path).map_err(cannot_read)?;
    let mut output = PgnOutput::new(io::stdout().lock());
    for game in database.games().map_err(cannot_read)? {
        let game = game.map_err(cannot_read)?;
        output.write(&game).map_err(Failure::Write)?;
    }
    output.output.flush().map_err(Failure::Write)?;
    Ok(0)
}

fn print_totals(report: &PgnReport, mut out: impl Write) -> io::Result<()> {
    writeln!(out, "games: {}", report.games)?;
    writeln!(out, "main line moves: {}", report.main_line_moves)?;
    writeln!(out, "variations: {}", report.variations)?;
    writeln!(out, "comments: {}", report.comments)?;
    writeln!(out, "problems: {}", report.problems)
}
