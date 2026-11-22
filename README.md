# zobrist

A chess database for my own games, written in Rust. A personal, for-fun project.

If you stumbled across this, you were probably looking for
[En Croissant](https://github.com/franciscoBSalgueiro/en-croissant).

## Why public

So that GitHub Actions is free. There is no support, and nothing here is meant for other users.

## Name

After [Zobrist hashing](https://chessprogramming.org/Zobrist_Hashing), the trick chess programs use to give every position a number.

## Release notes

End each PR description with `## Release notes`. Write one `- ` bullet per change a chess player will notice, or use `- N/A` for an internal change. The squash commit keeps this text.

For the first release, run `scripts/release-notes.py TAG`. For later releases, run `scripts/release-notes.py PREVIOUS_TAG TAG`. Copy the Markdown from standard output into the GitHub release. Review any commits listed on standard error for changes that need a handwritten entry.

## License

GPL-3.0-or-later, see `LICENSE`.
