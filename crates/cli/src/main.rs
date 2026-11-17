//! The `zobrist` command line.

use std::fs::File;
use std::io::{self, Seek, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use zobrist_core::pgn::{self, Entry};

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
}

#[derive(Subcommand)]
enum PgnCommand {
    /// Read every game of a PGN file and report what was read and what could not be.
    Check {
        /// The PGN file to read.
        file: PathBuf,
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
    problem_lines: Vec<String>,
}

enum Failure {
    Read(io::Error),
    Write,
}

fn main() -> ExitCode {
    // clap exits with status 2 on a usage error, which matches "the command could not run".
    let Arguments {
        command: Command::Pgn {
            command: PgnCommand::Check { file },
        },
    } = Arguments::parse();
    check(&file)
}

fn check(path: &Path) -> ExitCode {
    match report(path) {
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
        Err(Failure::Write) => ExitCode::from(2),
    }
}

fn report(path: &Path) -> Result<usize, Failure> {
    let mut file = File::open(path).map_err(Failure::Read)?;
    let report = count(&file).map_err(Failure::Read)?;
    let mut out = io::stdout().lock();
    print_totals(&report, &mut out).map_err(|_| Failure::Write)?;
    if report.problem_lines.len() == report.problems {
        for line in &report.problem_lines {
            writeln!(out, "{line}").map_err(|_| Failure::Write)?;
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
        written.map_err(|_| Failure::Write)?;
    }
    Ok(report.problems)
}

fn count(file: &File) -> io::Result<Report> {
    // Only a regular file can be read a second time. A pipe, for one, is empty after the first
    // pass, so its problem lines must all wait in memory.
    let max_kept_lines = if file.metadata()?.is_file() {
        MAX_KEPT_PROBLEM_LINES
    } else {
        usize::MAX
    };
    let mut report = Report::default();
    pgn::read_games(file, |entry| {
        if let Entry::Game { game, .. } = &entry {
            report.games += 1;
            let counts = game.counts();
            report.main_line_moves += counts.main_line_moves;
            report.variations += counts.variations;
            report.comments += counts.comments;
        }
        if let Some(line) = problem_line(&entry) {
            report.problems += 1;
            if report.problem_lines.len() < max_kept_lines {
                report.problem_lines.push(line);
            }
        }
    })?;
    Ok(report)
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

fn print_totals(report: &Report, mut out: impl Write) -> io::Result<()> {
    writeln!(out, "games: {}", report.games)?;
    writeln!(out, "main line moves: {}", report.main_line_moves)?;
    writeln!(out, "variations: {}", report.variations)?;
    writeln!(out, "comments: {}", report.comments)?;
    writeln!(out, "problems: {}", report.problems)
}
