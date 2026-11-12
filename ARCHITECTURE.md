# Architecture

This document is the map a reader opens first. It names the parts of zobrist and the rules that hold them together. It does not repeat what a file already lists. Everything marked planned does not exist yet.

## Bird's eye view

zobrist is a personal chess database desktop app, in the spirit of ChessBase, written in Rust. It is meant to store the maintainer's own games, search them, and run a UCI engine for analysis. It is developed on Linux and is meant to run on macOS and Windows as well.

The program is a set of Rust crates with one direction of dependency. A pure domain model sits at the bottom. Persistence and engine management sit on top of it. Two user interfaces, a CLI and a Tauri desktop shell with a React view, sit at the top and own no chess logic.

## Code map

### `crates/core` (zobrist-core)

The chess domain model. Pure. No IO and no framework dependencies.

### `crates/store` (zobrist-store, planned)

SQLite persistence and search over games and positions.

### `crates/engine` (zobrist-engine, planned)

UCI engine process management.

### `crates/cli` (zobrist-cli, planned)

The first user interface and the agents' test bed. Everything the app can do is reachable here first.

### `crates/app` (zobrist-app, planned)

Tauri shell with a React view.

### `scripts/`

The quality gate. `scripts/check.sh` is the entry point. Each script says in its first lines what it checks.

## Dependency direction

```
core  <-  store, engine  <-  cli, app
```

Read each arrow as "is depended on by". `store` and `engine` depend on `core`. `cli` and `app` depend on `core`, `store`, and `engine`. Nothing depends on `cli` or `app`. `core` depends only on the pure chess crates that `scripts/check-deps.sh` allows, and never on IO or framework crates. Only `store` may depend directly on `rusqlite`, and only `app` may depend directly on the `tauri` crates.

`scripts/check-deps.sh` holds this rule as data and enforces it. The rule covers normal and build dependencies, not dev-dependencies.

## Invariants

- Domain, persistence, engine, and application logic live only in Rust. The view renders state, forwards user intent, and keeps only UI-local state.
- External data is parsed at the boundary (PGN, UCI, SQLite rows, IPC). Internal code trusts its types.
- Illegal states are unrepresentable. Prefer a type that cannot hold a bad value over a check that rejects one.
- The view is a pure consumer of IPC types generated from Rust (planned, the generator is chosen in the app step).

## Decisions

A decision belongs here when a single PR cannot undo it without pain. Examples are the language, the license, the storage format, the UI toolkit, and a new dependency of `core`. Each entry gives the choice, the reason that decided it, the rejected options, the consequences that later work must respect, and the condition under which to revisit it. Edit the entry when the decision changes.

### Rust for all logic

Ownership, exhaustive matching, and the type system reject whole classes of defects before review, and clippy turns the lint set into a gate. shakmaty, pgn-reader, and shakmaty-syzygy are a maintained chess core in the same language. Rejected options: TypeScript on Node loses its types at runtime, Python has optional typing and awkward desktop packaging, and C++ with Qt costs memory safety and adds build complexity.

Revisit if a team of human developers takes over and prefers another language.

### GPL-3.0-or-later

`core` will link shakmaty, pgn-reader, and shakmaty-syzygy. All three are GPL-3.0-or-later without a linking exception, so a distributed binary must be GPL as a whole. The MIT chess crates cozy-chess and owlchess have no PGN reader. Rejected options: no license and the PolyForm licenses are not GPL-compatible, AGPL-3.0 adds a network clause that a desktop app does not need, and MIT with Apache-2.0 cannot cover a binary that links a GPL library.

The license binds later work in two places. A piece set must have a GPL-compatible license. cburnett, merida, and chessnut qualify, and alpha and the CC BY-NC sets do not. A UCI engine such as Stockfish runs as a separate process, which the GPL treats as an aggregate. If a release ever bundles Stockfish, it must ship the GPL text and a pointer to the Stockfish source.

`deny.toml` lists the allowed licenses. Revisit if `core` stops linking a GPL chess library.

### Tauri 2 with React for the desktop UI

The app needs a board, a notation panel with nested variations, and a large game list, and it should have a modern web look. En Croissant shows that this stack carries a chess database with a large game list, and a trial build rendered correctly on Linux with Wayland. Rejected options: egui is pure Rust with headless tests but lost on look, the release notes of iced do not mention accessibility, Slint adds a third language, and the renderer of Dioxus native is in beta.

The invariants limit the cost of the second language. The TypeScript toolchain is strict TypeScript, Biome, and Vitest, and Vitest tests only UI-local state. A board component such as chessground is allowed as a view-only dependency. The app step chooses the IPC type generator from tauri-specta and ts-rs.

Revisit if the notation panel or the large game list cannot be made acceptable, or if the webview fails on Linux, the development platform. The fallback is egui in `app`, which leaves `core`, `store`, and `engine` unchanged.

## Gate and CI

There is one gate, `scripts/check.sh`, and CI runs the same script. The steps live in the script, the lint levels in `Cargo.toml`, and the workflows in `.github/workflows/`.

The macOS and Windows jobs run only `cargo check`. They report a failure but do not block a merge. They become blocking once code touches the operating system.

## Documents

Each kind of document stays true in a different way.

- The Decisions section of this file records the outcome of each decision that is hard to reverse. An entry changes when its decision changes. The research and the discussion that led to a decision stay outside the repository.
- Specs are GitHub issues. An issue says what the next step builds. It closes with its PR, and after the merge the tests are the spec.
- `AGENTS.md` holds the rules for agents. It changes when a rule is added or dropped.
- This file is the map. It changes when a crate, a dependency edge, or an invariant changes, not with every PR.
- Rustdoc is the API documentation. The gate fails on `missing_docs` and runs the doc tests.
- Research notes stay outside the repository. A document that is right for an hour has no place next to code.
