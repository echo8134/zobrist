---
status: accepted
date: 2026-10-28
---

# Use Tauri 2 with React for the desktop UI

## Context and Problem Statement

zobrist needs a desktop UI with a board, a notation panel with nested variations, and a large game list. The maintainer runs Hyprland on native Wayland with an AMD GPU and values a modern web look. Which toolkit should the `app` crate use, given that all logic stays in Rust?

## Considered Options

* Tauri 2 with React
* egui/eframe 0.37.0
* iced 0.14.0
* Slint 1.18.1
* Dioxus native

## Decision Outcome

Chosen option: "Tauri 2 with React", because the maintainer values a modern web look, En Croissant 0.15.1 (GPL-3.0) proves the stack for a chess database with a large game list, and the platform risk did not reproduce: a minimal Tauri 2.12.5 app built and run on the maintainer's machine (Hyprland, native Wayland, AMD GPU, webkit2gtk 2.54.1) rendered correctly with no Wayland or EGL errors. egui is pure Rust with AccessKit and headless tests through egui_kittest but lost on look. iced's release notes do not mention accessibility. Slint adds a third language. Dioxus native's renderer Blitz is still beta.

Rules that bound the cost of the second language: domain, persistence, engine, and application logic live in Rust; the view renders state, forwards user intent, and keeps only UI-local state; IPC types are generated from Rust (tauri-specta or ts-rs, chosen in the app step); the TypeScript toolchain is strict TypeScript, Biome, and Vitest for UI-local state only; a board component such as chessground is allowed as a view-only dependency.

### Consequences

* Good, because the view can use mature web component libraries.
* Bad, because the UI is split across two languages and a JS build.
* Bad, because webview and platform integration are where most UI bugs in En Croissant's tracker come from.

### Confirmation

`scripts/check-deps.sh` keeps the `tauri` crates out of the normal and build dependencies of every crate but `app`. AGENTS.md binds the view to hold no chess logic.

## More Information

Revisit if the notation panel or the large game list cannot be made acceptable, or if the webview fails on the maintainer's platform. The fallback is egui in the `app` crate; `core`, `store`, and `engine` are unaffected.

* egui releases: https://github.com/emilk/egui/releases (0.37.0, 2026-10-15)
* Tauri: https://crates.io/crates/tauri (2.12.5, 2026-10-21)
* En Croissant: https://github.com/franciscoBSalgueiro/en-croissant (0.15.1, 2026-09-05), Wayland issue https://github.com/franciscoBSalgueiro/en-croissant/issues/929
* egui_extras renders only visible table rows: https://docs.rs/egui_extras/latest/egui_extras/struct.TableBody.html
