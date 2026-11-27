# Architecture

This document is the map a reader opens first. It names the parts of zobrist and the rules that hold them together. It does not repeat what a file already lists. Everything marked planned does not exist yet.

## Bird's eye view

zobrist is a personal chess database desktop app written in Rust. It is meant to store the maintainer's own games, search them, and run a UCI engine for analysis. It is developed on Linux and is meant to run on macOS and Windows as well.

The program is a set of Rust crates with one direction of dependency. A pure domain model sits at the bottom. Persistence, engine management, and connections to chess sites sit on top of it. Two user interfaces, a CLI and a Tauri desktop shell with a React view, sit at the top and own no chess logic.

## Code map

### `crates/core` (zobrist-core)

The chess domain model. Pure. No IO and no framework dependencies.

### `crates/store` (zobrist-store)

Creates a game database in zobrist's own file format, verifies its checksums, and reads its games in order. It also reads one game of a PGN file by its number, adds games to a PGN file, and replaces one game of a PGN file. Adding and replacing keep every other byte of the file. It builds the row that a list shows of a game, so the CLI and the app list the same values. Search over games and positions is planned.

### `crates/engine` (zobrist-engine)

Runs a local UCI program and collects the lines it reports for each search. It uses the standard library.

### `crates/online` (zobrist-online)

Owns outgoing connections to chess sites. It resolves a public Lichess game link, fetches its PGN export, and returns one finished standard game or the reason it could not be opened.

### `crates/cli` (zobrist-cli)

The first user interface and the agents' test bed.

### `crates/app` (zobrist-app)

Tauri shell with a React view. The view lives in `crates/app/view` and reaches Rust through one command that takes a typed request and returns a typed response. The shell depends on `core` for games, `store` for PGN files, `engine` for board analysis, and `online` for games opened from links. It holds the lock on its open games only for the steps that touch them. File, network, and engine I/O run outside that lock.

### `scripts/`

The quality gate. `scripts/check.sh` is the entry point. Each script says in its first lines what it checks.

## Dependency direction

```
core  <-  store, engine, online  <-  cli, app
```

Read each arrow as "is depended on by". `store`, `engine`, and `online` may depend on `core`. `cli` and `app` may use each of these crates and `core` directly. Nothing depends on `cli` or `app`. `core` depends only on the pure chess crates that `scripts/check-deps.sh` allows, and never on IO or framework crates. No workspace crate may depend directly on `rusqlite`. Only `app` may depend directly on the `tauri` crates. Only `online` may depend directly on `reqwest`.

`scripts/check-deps.sh` holds this rule as data and enforces it. The rule covers normal and build dependencies, not dev-dependencies.

## Invariants

- Domain, persistence, engine, and application logic live only in Rust. The view renders state, forwards user intent, and keeps only UI-local state.
- External data is parsed at the boundary (PGN, UCI, database segments, IPC). Internal code trusts its types.
- Illegal states are unrepresentable. Prefer a type that cannot hold a bad value over a check that rejects one.
- The view is a pure consumer of IPC types generated from Rust.

## Decisions

A decision belongs here when a single PR cannot undo it without pain. Examples are the language, the license, the storage format, the UI toolkit, and a new dependency of `core`. Each entry gives the choice, the reason that decided it, the rejected options, the consequences that later work must respect, and the condition under which to revisit it. Edit the entry when the decision changes.

### Rust for all logic

Ownership, exhaustive matching, and the type system reject whole classes of defects before review, and clippy turns the lint set into a gate. shakmaty, pgn-reader, and shakmaty-syzygy are a maintained chess core in the same language. Rejected options: TypeScript on Node loses its types at runtime, Python has optional typing and awkward desktop packaging, and C++ with Qt costs memory safety and adds build complexity.

Revisit if a team of human developers takes over and prefers another language.

### GPL-3.0-or-later

`core` links shakmaty and pgn-reader, and will link shakmaty-syzygy. All three are GPL-3.0-or-later without a linking exception, so a distributed binary must be GPL as a whole. The MIT chess crates cozy-chess and owlchess have no PGN reader. Rejected options: no license and the PolyForm licenses are not GPL-compatible, AGPL-3.0 adds a network clause that a desktop app does not need, and MIT with Apache-2.0 cannot cover a binary that links a GPL library.

The license binds later work in two places. A piece set must have a GPL-compatible license. cburnett, merida, and chessnut qualify, and alpha and the CC BY-NC sets do not. A UCI engine runs as a separate process, which the GPL treats as an aggregate. If a release ever bundles a GPL engine, it must ship the GPL text and a pointer to the engine's source.

`deny.toml` lists the allowed licenses. Revisit if `core` stops linking a GPL chess library.

### Tauri 2 with React for the desktop UI

The app needs a board, a notation panel with nested variations, and a large game list, and it should have a modern web look. A trial build rendered correctly on Linux with Wayland. Rejected options: egui is pure Rust with headless tests but lost on look, the release notes of iced do not mention accessibility, Slint adds a third language, and the renderer of Dioxus native is in beta.

The invariants limit the cost of the second language. The TypeScript toolchain is strict TypeScript, Biome, and Vitest, and Vitest tests only UI-local state. A board component such as chessground is allowed as a view-only dependency.

ts-rs generates the IPC types, and every call from the view goes through one command that takes a typed request and returns a typed response, so the TypeScript check rejects a call that Rust does not know. Rejected option: tauri-specta also writes one function per command and checks more by itself, but its version for Tauri 2 is a release candidate. A later switch touches the type module, the command, and the one file through which the view calls Rust.

Rust owns every open game. Opening, playing, and navigating return the whole picture of the game from one editor. The view can draw the board and notation together without applying a sequence of changes or combining separate answers that might describe different positions. This sends the whole notation even after a single move. Revisit the choice if a game grows large enough for a player to notice the delay. A switch would touch every view component that reads the picture.

The view is styled with plain CSS and builds its own controls. It uses no component library and no utility-class framework. The design is hairlines, text, and a few controls, and a library brings a look of its own that zobrist would have to undo. `crates/app/view/src/design.css` holds the design values, and each component has a stylesheet of its own. The cost is that zobrist builds and tests every control itself, also the hard ones that come later, such as the long game list and the tiles. A later switch touches every component. Revisit this choice when a control costs more to build than a library costs to restyle.

Revisit if the notation panel or the large game list cannot be made acceptable, or if the webview fails on Linux, the development platform. The fallback is egui in `app`, which leaves `core`, `store`, and `engine` unchanged.

### Local engine analysis on the board

The desktop starts the registered program as a child and speaks UCI over its standard streams. This lets players choose an installed executable independently of how zobrist was built. Linking engine libraries would require a new build for each choice.

While the engine runs, the view polls through the typed request. Each answer identifies the game, its position revision and the search, so a line arriving after a move or restart cannot appear on the wrong board. Sending unsolicited updates would change the one-request, one-answer contract. Revisit polling if delivering analysis to an active board measurably delays play or navigation.

### One client for outgoing connections to chess sites

The CLI and desktop both open public Lichess links through `zobrist-online`. The crate owns the provider's HTTPS address, request limits, and cooldown. It uses `reqwest` (MIT OR Apache-2.0) with native TLS and returns game types from `zobrist-core`. Keeping this code in each interface would duplicate the request rules. Putting it in `core` would give the chess model an I/O dependency.

Future connections to chess sites, including account features, belong in this crate. Revisit the client choice if a provider requires a transport it cannot support.

### An own file format for the game store

`store` keeps a database in one file of its own format. The file is a sequence of immutable segments with checksums. A segment holds its games by column: fixed-width fields for the game list, dictionaries for names, the main lines as one compact stream of moves, and everything else a game holds beside them. A change appends a segment, and compaction rewrites the file.

A scan over this layout answers position searches, sorted lists, and filters fast enough at millions of games, with no index and no stored sort order, so one format serves every database size and no database engine is needed.

`store` owns the format and its version number, and a reader verifies the checksums before it trusts a segment. PGN stays the exchange format. After the first release, a change of the format needs a reader for the old version or a migration.

Revisit if a scan cannot answer a kind of query fast enough. The first answer is then an index part inside this format, not another store.

### Personal PGN files on the desktop

The Files page keeps references to PGN files and folders. A player can keep a repertoire in one folder and tournament games in another, close the app, and find both again on the next start. Opening a PGN keeps it in place. Saving an edit writes to that same file. The app stores the paths in files.json in its local data directory, which Tauri resolves with `app_local_data_dir()` for `io.github.echo8134.zobrist`.

When creating a PGN, the native save dialog starts in the selected folder. Without a selected folder, it suggests a folder named `zobrist` inside the personal Documents directory reported by the operating system. When the system has no Documents directory, the player chooses the destination in the dialog. The player can change the suggested location before saving.

A single managed folder would require players to move or copy existing collections. Resolving a new name against the app's launch directory made its location depend on how the player started the app. The desktop shell uses `tauri-plugin-dialog` to offer the system's file, folder and save dialogs. Revisit the storage choice if players need a collection that travels between computers.

### Releases from release-plz

release-plz turns the PR titles into releases. It keeps one release PR open that raises the version in `[workspace.package]` and adds a section to the changelog. The merge of that PR creates the tag `vX.Y.Z` and a draft GitHub release, and `.github/workflows/release.yml` attaches the command line program and the AppImage for Linux and publishes the draft. release-plz was chosen because it reads the last version from the git tags and raises a version that every crate inherits from the workspace. Rejected options: release-please cannot raise a version that crates inherit from the workspace, which is its open issue 2111. A workflow of our own around a release notes script kept hand-written notes, but zobrist would own and test a release tool.

The choice binds later work in three places. A `feat`, `fix`, or `perf` PR title is a line of the changelog, and every other type asks for no release. Tags keep the form `vX.Y.Z`, because a release tool finds the last release by its tag, so a later switch of the tool must keep that form. A release PR changes only the version fields, `Cargo.lock`, and the changelog, so a test that needs the version takes it from the manifest and never from a literal.

`release-plz.toml` holds the settings. Revisit if release-plz stops raising a version that the crates inherit from the workspace.

## Gate and CI

There is one gate, `scripts/check.sh`, and CI runs the same script. The steps live in the script, the lint levels in `Cargo.toml`, and the workflows in `.github/workflows/`. Beside the gate, the workflow `release` makes the releases that the Decisions entry describes. On a pull request that changes the release files, it builds them and publishes nothing.

The macOS and Windows jobs only build the view and run `cargo check`. zobrist ships no build for these systems yet, and the jobs keep the code portable until it does. A failure blocks a merge, because code now touches the operating system.

## Documents

Each kind of document stays true in a different way.

- The Decisions section of this file records the outcome of each decision that is hard to reverse. An entry changes when its decision changes. The research and the discussion that led to a decision stay outside the repository.
- Specs are GitHub issues. An issue says what the next step builds. It closes with its PR, and after the merge the tests are the spec.
- `AGENTS.md` holds the rules for agents. It changes when a rule is added or dropped.
- This file is the map. It changes when a crate, a dependency edge, or an invariant changes, not with every PR.
- Rustdoc is the API documentation. The gate fails on `missing_docs` and runs the doc tests.
- Research notes stay outside the repository. A document that is right for an hour has no place next to code.
