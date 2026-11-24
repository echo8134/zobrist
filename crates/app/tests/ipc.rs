//! The answers of the app, asked for the way the view's one command asks.

use zobrist_app::ipc::{self, Request, Response};

#[test]
fn answers_the_version_request_with_the_version_of_the_crate() {
    // A release changes only the version line in `[workspace.package]` of the root manifest, so
    // the expected value comes from there and not from a literal.
    let manifest = include_str!("../../../Cargo.toml");
    let workspace_package = manifest
        .split_once("[workspace.package]\n")
        .expect("the root manifest has a [workspace.package] section")
        .1
        .split("\n[")
        .next()
        .unwrap_or_default();
    let version = workspace_package
        .lines()
        .find_map(|line| line.strip_prefix("version = "))
        .expect("[workspace.package] sets the version")
        .trim_matches('"');

    assert_eq!(
        ipc::answer(Request::Version),
        Response::Version {
            version: version.to_owned()
        }
    );
}
