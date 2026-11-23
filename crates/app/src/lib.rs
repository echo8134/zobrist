//! The zobrist desktop app: a Tauri shell with a React view.
//!
//! The view owns no logic. [`ipc`] holds everything that crosses between the view and Rust.

pub mod ipc;
