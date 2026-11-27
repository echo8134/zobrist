//! A local UCI engine and the lines it reports for the current board position.

use std::io::{self, BufRead as _, BufReader, Read as _, Write as _};
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc::{self, Receiver, Sender, SyncSender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

/// The evaluation in the root side-to-move's perspective.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Score {
    /// Hundredths of a pawn.
    Centipawns(i32),
    /// Moves to mate. A negative value means the opponent delivers mate.
    Mate(i32),
}

/// A scored principal variation. The app replays the moves on its board.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    /// The engine's evaluation of this line.
    pub score: Score,
    /// Moves written in UCI coordinates, beginning at the search position.
    pub moves: Vec<String>,
}

/// The editor's UCI position and the castling rules used for its moves.
#[derive(Clone, PartialEq, Eq)]
pub struct Position {
    /// Full UCI position command, including the history to the current node.
    pub command: String,
    /// Nonstandard castling rights require the program's `UCI_Chess960` option.
    pub chess960: bool,
}

struct Search {
    revision: usize,
    position: Position,
}

struct Analysis {
    desired: Option<Search>,
    lines: Result<[Option<Line>; 2], String>,
}

enum Event {
    PositionChanged,
    Output(String),
    Disconnected,
}

/// One child process, from its UCI handshake until it is reaped.
pub struct Engine {
    analysis: Arc<Mutex<Analysis>>,
    events: SyncSender<Event>,
    worker: Option<JoinHandle<()>>,
}

impl Engine {
    /// Start an executable directly and begin the handshake in the background.
    /// `position` is a complete UCI position command from the editor.
    pub fn start(path: &Path, revision: usize, position: Position) -> io::Result<Self> {
        let mut child = Command::new(path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let (Some(output), Some(mut input)) = (child.stdout.take(), child.stdin.take()) else {
            reap(&mut child);
            return Err(io::Error::other("Could not open the engine pipes."));
        };
        let analysis = Arc::new(Mutex::new(Analysis {
            desired: Some(Search { revision, position }),
            lines: Ok([None, None]),
        }));
        let (events, receive) = mpsc::sync_channel(16);
        let reader = events.clone();
        thread::spawn(move || {
            let mut output = BufReader::new(output);
            loop {
                // An executable without a newline must not grow a buffer without a limit.
                let mut line = String::new();
                let limit = 64 * 1024;
                match output.by_ref().take(limit).read_line(&mut line) {
                    Ok(1..) if line.ends_with('\n') => {
                        if reader.send(Event::Output(line)).is_err() {
                            return;
                        }
                    }
                    _ => break,
                }
            }
            drop(reader.send(Event::Disconnected));
        });
        let (write, commands) = mpsc::channel::<String>();
        let writer = events.clone();
        thread::spawn(move || {
            for command in commands {
                if writeln!(input, "{command}").is_err() {
                    drop(writer.send(Event::Disconnected));
                    break;
                }
            }
        });
        let shared = Arc::clone(&analysis);
        let worker = thread::spawn(move || {
            if let Err(message) = run(&shared, &receive, &write) {
                lock(&shared).lines = Err(message.to_owned());
            }
            drop(write.send("quit".to_owned()));
            reap(&mut child);
        });
        Ok(Self {
            analysis,
            events,
            worker: Some(worker),
        })
    }

    /// Replace the search and immediately invalidate lines for the previous revision.
    pub fn set_position(&mut self, revision: usize, position: Position) {
        let mut analysis = lock(&self.analysis);
        if analysis
            .desired
            .as_ref()
            .is_some_and(|current| current.revision >= revision)
        {
            return;
        }
        analysis.desired = Some(Search { revision, position });
        if let Ok(lines) = &mut analysis.lines {
            *lines = [None, None];
        }
        drop(self.events.try_send(Event::PositionChanged));
    }

    /// Read the latest scored lines, in rank order, and their position revision.
    pub fn snapshot(&self) -> Result<(usize, Vec<Line>), String> {
        let analysis = lock(&self.analysis);
        let search = analysis.desired.as_ref().ok_or("Engine stopped.")?;
        Ok((
            search.revision,
            analysis
                .lines
                .as_ref()
                .map_err(Clone::clone)?
                .iter()
                .flatten()
                .cloned()
                .collect(),
        ))
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        lock(&self.analysis).desired = None;
        // A full queue already wakes the worker, which reads the newest request.
        drop(self.events.try_send(Event::PositionChanged));
        if let Some(worker) = self.worker.take() {
            drop(worker.join());
        }
    }
}

fn lock(analysis: &Mutex<Analysis>) -> MutexGuard<'_, Analysis> {
    analysis.lock().unwrap_or_else(PoisonError::into_inner)
}

fn reap(child: &mut Child) {
    let deadline = Instant::now() + Duration::from_millis(150);
    while Instant::now() < deadline {
        if matches!(child.try_wait(), Ok(Some(_))) {
            return;
        }
        thread::sleep(Duration::from_millis(5));
    }
    drop(child.kill());
    drop(child.wait());
}

#[derive(Clone, Copy)]
enum Phase {
    Start,
    Identify,
    Ready,
    Search(usize),
    Stop,
    Finished(usize),
}

fn run(
    analysis: &Mutex<Analysis>,
    receive: &Receiver<Event>,
    write: &Sender<String>,
) -> Result<(), &'static str> {
    const DISCONNECTED: &str = "The engine disconnected.";
    const TIMED_OUT: &str = "The engine did not answer in time.";
    let send = |command: &str| write.send(command.to_owned()).map_err(|_| DISCONNECTED);
    let deadline = || Instant::now() + Duration::from_secs(5);
    let mut phase = Phase::Identify;
    let mut until = Some(deadline());
    let mut line_count = 1;
    let mut supports_chess960 = false;
    let mut chess960 = false;
    send("uci")?;
    loop {
        {
            let analysis = lock(analysis);
            let Some(search) = &analysis.desired else {
                return Ok(());
            };
            if let Phase::Search(revision) = phase
                && revision != search.revision
            {
                send("stop")?;
                phase = Phase::Stop;
                until = Some(deadline());
            } else if let Phase::Finished(revision) = phase
                && revision != search.revision
            {
                phase = Phase::Start;
            }
            if matches!(phase, Phase::Start) {
                if chess960 == search.position.chess960 {
                    send(&search.position.command)?;
                    send("go infinite")?;
                    phase = Phase::Search(search.revision);
                    until = None;
                } else {
                    if !supports_chess960 {
                        return Err("This engine does not support Chess960 castling.");
                    }
                    chess960 = search.position.chess960;
                    send(&format!("setoption name UCI_Chess960 value {chess960}"))?;
                    send("isready")?;
                    phase = Phase::Ready;
                    until = Some(deadline());
                }
            }
        }
        let event = match until {
            Some(end) => {
                let time = end
                    .checked_duration_since(Instant::now())
                    .ok_or(TIMED_OUT)?;
                receive.recv_timeout(time).map_err(|_| TIMED_OUT)?
            }
            None => receive.recv().map_err(|_| DISCONNECTED)?,
        };
        let line = match event {
            Event::Output(line) => line,
            Event::PositionChanged => continue,
            Event::Disconnected => return Err(DISCONNECTED),
        };
        let line = line.trim();
        match (phase, line.split_ascii_whitespace().next()) {
            (Phase::Identify, Some("uciok")) => {
                if line_count == 2 {
                    send("setoption name MultiPV value 2")?;
                }
                send("isready")?;
                phase = Phase::Ready;
                until = Some(deadline());
            }
            (Phase::Identify, Some("option")) => {
                if supports_two_lines(line) {
                    line_count = 2;
                } else if option_values(line, "UCI_Chess960", "check").is_some() {
                    supports_chess960 = true;
                }
            }
            (Phase::Ready, Some("readyok")) | (Phase::Stop, Some("bestmove")) => {
                phase = Phase::Start;
            }
            (Phase::Search(revision), Some("bestmove")) => phase = Phase::Finished(revision),
            (Phase::Search(revision), Some("info")) => {
                if let Some((rank, line)) = principal_variation(line, line_count) {
                    let mut analysis = lock(analysis);
                    if analysis
                        .desired
                        .as_ref()
                        .is_some_and(|search| search.revision == revision)
                        && let Ok(lines) = &mut analysis.lines
                    {
                        lines[rank - 1] = Some(line);
                    }
                }
            }
            _ => {}
        }
    }
}

fn option_values<'line>(
    line: &'line str,
    name: &str,
    kind: &str,
) -> Option<std::str::SplitAsciiWhitespace<'line>> {
    let mut words = line.split_ascii_whitespace();
    for field in ["option", "name", name, "type", kind] {
        if !words.next()?.eq_ignore_ascii_case(field) {
            return None;
        }
    }
    Some(words)
}

fn supports_two_lines(option: &str) -> bool {
    let Some(mut words) = option_values(option, "MultiPV", "spin") else {
        return false;
    };
    let mut minimum = 1;
    let mut maximum = 1;
    while let (Some(field), Some(value)) = (words.next(), words.next()) {
        match field {
            "min" => minimum = value.parse().unwrap_or(usize::MAX),
            "max" => maximum = value.parse().unwrap_or(0),
            _ => {}
        }
    }
    minimum <= 2 && maximum >= 2
}

fn principal_variation(info: &str, line_count: usize) -> Option<(usize, Line)> {
    let mut words = info.split_ascii_whitespace();
    if words.next()? != "info" {
        return None;
    }
    let mut score = None;
    let mut rank = 1;
    while let Some(field) = words.next() {
        match field {
            "score" => {
                let kind = words.next()?;
                let value = words.next()?.parse().ok()?;
                score = Some(match kind {
                    "cp" => Score::Centipawns(value),
                    "mate" => Score::Mate(value),
                    _ => return None,
                });
            }
            "multipv" => rank = words.next()?.parse().ok()?,
            "pv" if (1..=line_count).contains(&rank) => {
                let moves: Vec<_> = words.map(str::to_owned).collect();
                return (!moves.is_empty()).then_some((
                    rank,
                    Line {
                        score: score?,
                        moves,
                    },
                ));
            }
            "lowerbound" | "upperbound" | "string" | "pv" => return None,
            _ => {}
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_scored_lines_and_discards_bounds_and_unrequested_ranks() {
        assert_eq!(
            principal_variation("info score cp 34 pv e2e4 e7e5", 1),
            Some((
                1,
                Line {
                    score: Score::Centipawns(34),
                    moves: vec!["e2e4".into(), "e7e5".into()]
                }
            ))
        );
        assert_eq!(
            principal_variation("info depth 8 multipv 2 score mate -3 pv g3g2", 2),
            Some((
                2,
                Line {
                    score: Score::Mate(-3),
                    moves: vec!["g3g2".into()]
                }
            ))
        );
        assert_eq!(
            principal_variation("info score cp 27 upperbound pv e2e4", 2),
            None
        );
        assert_eq!(
            principal_variation("info string score cp 27 pv e2e4", 2),
            None
        );
        assert_eq!(
            principal_variation("info multipv 2 score cp 27 pv e2e4", 1),
            None
        );
    }

    #[test]
    fn enables_nonstandard_castling_before_sending_an_imported_position() {
        let fen = "position fen rbbqnknr/pppppppp/8/8/8/8/PPPPPPPP/RBBQNKNR w KQkq - 0 1";
        let analysis = Mutex::new(Analysis {
            desired: Some(Search {
                revision: 1,
                position: Position {
                    command: fen.into(),
                    chess960: true,
                },
            }),
            lines: Ok([None, None]),
        });
        let (events, receive) = mpsc::channel();
        let (write, commands) = mpsc::channel();
        for answer in [
            "option name UCI_Chess960 type check default false",
            "uciok",
            "readyok",
            "readyok",
        ] {
            events.send(Event::Output(answer.into())).unwrap();
        }
        events.send(Event::Disconnected).unwrap();
        assert_eq!(
            run(&analysis, &receive, &write),
            Err("The engine disconnected.")
        );
        assert_eq!(
            commands.try_iter().collect::<Vec<_>>(),
            [
                "uci",
                "isready",
                "setoption name UCI_Chess960 value true",
                "isready",
                fen,
                "go infinite",
            ]
        );
    }

    #[cfg(unix)]
    #[test]
    fn reports_a_stalled_handshake_and_reaps_the_unresponsive_program() -> io::Result<()> {
        use std::{env, fs, os::unix::fs::PermissionsExt as _, process};

        let directory = env::temp_dir().join(format!("zobrist-uci-timeout-{}", process::id()));
        fs::create_dir_all(&directory)?;
        let program = directory.join("unresponsive");
        fs::write(
            &program,
            "#!/bin/sh\necho \"$$\" > \"$0.pid\"\nwhile read -r command; do :; done\n",
        )?;
        fs::set_permissions(&program, fs::Permissions::from_mode(0o700))?;
        let start = Position {
            command: "position startpos".into(),
            chess960: false,
        };
        let mut engine = Engine::start(&program, 1, start.clone())?;
        engine.set_position(
            2,
            Position {
                command: "position startpos moves e2e4".into(),
                chess960: false,
            },
        );
        engine.set_position(1, start);
        assert_eq!(engine.snapshot(), Ok((2, vec![])));
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Err(message) = engine.snapshot() {
                assert_eq!(message, "The engine did not answer in time.");
                break;
            }
            assert!(Instant::now() < deadline);
            thread::sleep(Duration::from_millis(10));
        }
        let pid = fs::read_to_string(program.with_extension("pid"))?;
        drop(engine);
        let alive = Command::new("kill")
            .args(["-0", pid.trim()])
            .stderr(Stdio::null())
            .status()?;
        assert!(!alive.success());
        fs::remove_dir_all(directory)
    }
}
