# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0](https://github.com/echo8134/zobrist/compare/v0.1.0...v0.2.0) - 2026-11-28

### Added

- *(databases)* find and open games in your collections ([#141](https://github.com/echo8134/zobrist/pull/141))
- *(databases)* import a PGN as a new local collection ([#131](https://github.com/echo8134/zobrist/pull/131))
- download a player's public Lichess games into a PGN file ([#114](https://github.com/echo8134/zobrist/pull/114))
- *(app)* restore an earlier state of a PGN from Files ([#115](https://github.com/echo8134/zobrist/pull/115))
- *(databases)* keep local chess collections on this device ([#101](https://github.com/echo8134/zobrist/pull/101))
- open a finished Lichess game from its link ([#96](https://github.com/echo8134/zobrist/pull/96))
- *(app)* open a PGN file on the page Files and list its games ([#71](https://github.com/echo8134/zobrist/pull/71))
- *(pgn)* replace one game in a PGN file ([#67](https://github.com/echo8134/zobrist/pull/67))
- *(pgn)* list games and add them to a PGN file ([#42](https://github.com/echo8134/zobrist/pull/42))
- show the recorded evaluation and clock for the selected move ([#124](https://github.com/echo8134/zobrist/pull/124))
- *(app)* add an engine line to the game as a variation ([#129](https://github.com/echo8134/zobrist/pull/129))
- *(app)* start and stop the default engine on the board ([#113](https://github.com/echo8134/zobrist/pull/113))
- *(app)* set up positions and copy and paste chess games ([#86](https://github.com/echo8134/zobrist/pull/86))
- *(app)* set up a position and exchange games as PGN text ([#74](https://github.com/echo8134/zobrist/pull/74))
- *(app)* keep arrows and colored squares at each position ([#72](https://github.com/echo8134/zobrist/pull/72))
- *(app)* edit game details and results ([#70](https://github.com/echo8134/zobrist/pull/70))
- *(app)* undo and redo changes to an open game ([#68](https://github.com/echo8134/zobrist/pull/68))
- *(app)* set comments and annotation symbols in open games ([#66](https://github.com/echo8134/zobrist/pull/66))
- *(app)* keep each open game's position and notation together ([#63](https://github.com/echo8134/zobrist/pull/63))
- *(core)* read and write arrows and colored squares in PGN comments ([#61](https://github.com/echo8134/zobrist/pull/61))
- *(core)* promote variations and delete moves from a game ([#49](https://github.com/echo8134/zobrist/pull/49))
- *(core)* play moves and variations anywhere in a game ([#41](https://github.com/echo8134/zobrist/pull/41))
- *(app)* close again to stop a game import or download ([#145](https://github.com/echo8134/zobrist/pull/145))
- *(app)* delete a game from a PGN on Files ([#136](https://github.com/echo8134/zobrist/pull/136))
- *(app)* open a game of a PGN file and save it back ([#77](https://github.com/echo8134/zobrist/pull/77))
- check endgame positions and moves with the Lichess tablebase ([#140](https://github.com/echo8134/zobrist/pull/140))
- *(app)* reopen your boards after an unexpected shutdown ([#143](https://github.com/echo8134/zobrist/pull/143))
- *(app)* add your public Lichess games to a PGN from User ([#139](https://github.com/echo8134/zobrist/pull/139))
- *(app)* drop a PGN file on the window to open its games ([#137](https://github.com/echo8134/zobrist/pull/137))
- *(app)* keep one copy of zobrist open per data folder ([#134](https://github.com/echo8134/zobrist/pull/134))
- *(app)* reopen a recent PGN file from Home ([#126](https://github.com/echo8134/zobrist/pull/126))
- *(app)* reopen your boards where you left them ([#104](https://github.com/echo8134/zobrist/pull/104))
- *(app)* recover your file list from its last good copy ([#122](https://github.com/echo8134/zobrist/pull/122))
- *(app)* choose your pieces, board colours, and coordinates ([#109](https://github.com/echo8134/zobrist/pull/109))
- *(app)* remember your Lichess name on this device ([#103](https://github.com/echo8134/zobrist/pull/103))
- *(app)* remember installed engines and choose the default ([#102](https://github.com/echo8134/zobrist/pull/102))
- *(app)* keep your PGN files and folders together on the Files page ([#88](https://github.com/echo8134/zobrist/pull/88))
- *(app)* show Lichess and Chess.com on the User page ([#95](https://github.com/echo8134/zobrist/pull/95))
- *(app)* open the game data form on the first save ([#85](https://github.com/echo8134/zobrist/pull/85))
- *(app)* open a game of a file with a click and save it from its tab ([#82](https://github.com/echo8134/zobrist/pull/82))
- *(app)* write comments and set symbols below the notation ([#81](https://github.com/echo8134/zobrist/pull/81))
- *(app)* fill in the data of a game in a form above the notation ([#80](https://github.com/echo8134/zobrist/pull/80))
- *(app)* undo, redo, promote, and delete with buttons above the notation ([#79](https://github.com/echo8134/zobrist/pull/79))
- *(app)* walk through the game and show its variations ([#75](https://github.com/echo8134/zobrist/pull/75))
- *(app)* draw arrows and mark squares on the board ([#76](https://github.com/echo8134/zobrist/pull/76))
- *(app)* make moves on the board ([#73](https://github.com/echo8134/zobrist/pull/73))
- *(app)* show a board in a tab ([#69](https://github.com/echo8134/zobrist/pull/69))
- *(app)* open and close tabs in the top bar ([#44](https://github.com/echo8134/zobrist/pull/44))
- *(app)* add Events and User to the icon bar ([#40](https://github.com/echo8134/zobrist/pull/40))
- *(app)* collapse the icon bar to a strip of icons ([#31](https://github.com/echo8134/zobrist/pull/31))
- *(app)* give the window a top bar, an icon bar, and pages ([#29](https://github.com/echo8134/zobrist/pull/29))

### Fixed

- *(pgn)* keep the original file when added games would be lost ([#108](https://github.com/echo8134/zobrist/pull/108))
- *(app)* show how to recover damaged engine and account lists ([#123](https://github.com/echo8134/zobrist/pull/123))
- *(app)* save one copy of a game on a double-click ([#105](https://github.com/echo8134/zobrist/pull/105))

## [0.1.0](https://github.com/echo8134/zobrist/releases/tag/v0.1.0) - 2026-11-24

### Added

- *(store)* create a database from PGN and export its games ([#20](https://github.com/echo8134/zobrist/pull/20))
- *(pgn)* write games as PGN ([#11](https://github.com/echo8134/zobrist/pull/11))
- read games from a PGN file ([#7](https://github.com/echo8134/zobrist/pull/7))
