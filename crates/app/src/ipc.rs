//! Everything that crosses between the view and Rust.
//!
//! The view sends a [`Request`] and gets back the [`Response`] case with the same name.

use serde::{Deserialize, Serialize};

/// What the view can ask for.
#[derive(Clone, Copy, Deserialize)]
#[serde(tag = "type")]
pub enum Request {
    /// The version of zobrist.
    Version,
}

/// What Rust answers. Each case has the name of its request.
#[derive(Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type")]
pub enum Response {
    /// The version of zobrist.
    Version {
        /// The version of this crate.
        version: String,
    },
}

/// Answers a request of the view.
#[must_use]
pub fn answer(request: Request) -> Response {
    match request {
        Request::Version => Response::Version {
            version: env!("CARGO_PKG_VERSION").to_owned(),
        },
    }
}
