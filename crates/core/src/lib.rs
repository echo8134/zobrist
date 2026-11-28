//! Chess domain model for zobrist.
//!
//! This crate stays free of IO and framework dependencies. Persistence, engine
//! processes, and the user interface live in other crates that depend on this one.
//! The dependency rule is enforced by `scripts/check-deps.sh`.
//!
//! [`game`] holds the game model: tags, start position, a tree of legal moves with variations,
//! comments, and annotation symbols, and the result. [`editor`] navigates and edits those games.
//! [`pgn`] reads PGN bytes into games and never drops one: a game that cannot be read completely
//! keeps every move before its first problem and comes with a [`pgn::Problem`].
//!
//! ```
//! use zobrist_core::pgn::{self, Entry};
//!
//! let bytes = b"[White \"Alpha\"]\n[Black \"Beta\"]\n\n1. f3 e5 2. g4 Qh4# 0-1\n";
//! let mut moves = Vec::new();
//! pgn::read_games(&bytes[..], |entry| {
//!     if let Entry::Game { game, problem, .. } = entry {
//!         assert_eq!(game.tag("White"), Some("Alpha"));
//!         assert_eq!(problem, None);
//!         moves.extend(game.steps().map(|step| step.notation().to_string()));
//!     }
//! })?;
//! assert_eq!(moves, ["f3", "e5", "g4", "Qh4#"]);
//! # Ok::<(), std::io::Error>(())
//! ```

pub mod comment;
pub mod editor;
pub mod game;
pub mod pgn;
pub mod position;
pub mod shapes;
