//! The answers of the app, asked for the way the view's one command asks.

use zobrist_app::ipc::{self, Request, Response};

#[test]
fn answers_the_version_request_with_the_version_of_the_crate() {
    assert_eq!(
        ipc::answer(Request::Version),
        Response::Version {
            version: "0.0.0".to_owned()
        }
    );
}
