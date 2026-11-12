---
status: accepted
date: 2026-10-28
---

# Use Rust for all logic

## Context and Problem Statement

Nearly all code is written by agents and reviewed by one maintainer. Which language catches the most defects in agent-written code before review, and has a maintained chess core?

## Considered Options

* Rust
* TypeScript on Node
* Python
* C++ with Qt

## Decision Outcome

Chosen option: "Rust", because ownership, exhaustive matching, and the type system reject whole classes of defects before review, clippy with `-D warnings` turns the lint set into a gate, and shakmaty, pgn-reader, and shakmaty-syzygy give a maintained chess core in the same language. TypeScript loses its types at runtime, Python has optional typing and awkward desktop packaging, and C++ with Qt works against agent-written code on memory safety and build complexity. The view (ADR 0003) renders state and keeps only UI-local state; everything else is Rust.

### Consequences

* Good, because the compiler does part of the review.
* Bad, because compile times are longer than for a scripting language.
* Bad, because the desktop UI choice is less settled than on the web. ADR 0003 covers it.

### Confirmation

`rust-toolchain.toml` pins the toolchain. `scripts/check.sh` runs the gate locally through the Stop hook and in CI.

## More Information

Revisit if a human team takes over and prefers another language.

* Rust releases (1.99.1 current on 2026-10-28): https://blog.rust-lang.org/releases/
* shakmaty 0.30.2 (2026-10-07): https://crates.io/crates/shakmaty
