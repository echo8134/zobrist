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

### `docs/adr/`

Decision records, append-only. Nothing else lives under `docs/`.

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

## Gate and CI

There is one gate, `scripts/check.sh`, and CI runs the same script. The steps live in the script, the lint levels in `Cargo.toml`, and the workflows in `.github/workflows/`.

The macOS and Windows jobs run only `cargo check`. They report a failure but do not block a merge. They become blocking once code touches the operating system.

## Documents

Each kind of document stays true in a different way.

- `docs/adr/` records decisions that are hard to reverse, in MADR form. A record describes the day it was accepted and is never edited after. A new record supersedes an old one.
- Specs are GitHub issues. An issue says what the next step builds. It closes with its PR, and after the merge the tests are the spec.
- `AGENTS.md` holds the rules for agents. It changes when a rule is added or dropped.
- This file is the map. It changes when a crate, a dependency edge, or an invariant changes, not with every PR.
- Rustdoc is the API documentation. The gate fails on `missing_docs` and runs the doc tests.
- Research notes stay outside the repository. A document that is right for an hour has no place next to code.
