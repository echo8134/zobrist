use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Seek, SeekFrom, Write};
use std::path::Path;

use zobrist_core::game::Game;
use zobrist_core::pgn;

use crate::StagingDirectory;

/// Adds the supplied games to a PGN file, creating the target if necessary.
/// Follows symbolic links and stages the new file beside the destination.
/// Calls `supply` with a function that writes one game at a time. An error from
/// either function discards the staged file and leaves the target as it was.
/// Returns the number of games added after syncing and publishing the file.
pub fn add_pgn_games<E: From<io::Error>>(
    target: &Path,
    supply: impl FnOnce(&mut dyn FnMut(&Game) -> io::Result<()>) -> Result<(), E>,
) -> Result<u64, E> {
    let mut target = std::path::absolute(target)?;
    let target = loop {
        match target.canonicalize() {
            Ok(destination) => break destination,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        // A link can point to a file that this add will create.
        match target.read_link() {
            Ok(destination) => target.set_file_name(destination),
            Err(error) if error.kind() == io::ErrorKind::NotFound => break target,
            Err(error) => return Err(error.into()),
        }
    };
    let staging = StagingDirectory::beside(&target)?;
    let path = staging.0.join("games.pgn");
    let mut file = File::create_new(&path)?;
    let mut separator: &[u8] = b"";
    match File::options().read(true).write(true).open(&target) {
        Ok(mut old) => {
            let length = io::copy(&mut old, &mut file)?;
            if length > 0 {
                let mut tail = [0; 3];
                let start = length.saturating_sub(3);
                file.seek(SeekFrom::Start(start))?;
                let tail = &mut tail[..usize::try_from(length - start).unwrap_or(3)];
                file.read_exact(tail)?;
                separator = if tail.ends_with(b"\n\n") || tail.ends_with(b"\n\r\n") {
                    b""
                } else if tail.ends_with(b"\n") {
                    b"\n"
                } else {
                    b"\n\n"
                };
            }
            file.set_permissions(old.metadata()?.permissions())?;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    let mut output = BufWriter::new(file);
    let mut separator = Some(separator);
    let mut games = 0;
    supply(&mut |game| {
        // A caller can ignore a failed game. Only a complete write restores the separator.
        let before = separator.take().ok_or_else(failed_write)?;
        output.write_all(before)?;
        pgn::write_game(game, &mut output)?;
        separator = Some(b"\n");
        games += 1;
        Ok(())
    })?;
    separator.ok_or_else(failed_write)?;
    output.flush()?;
    output.get_ref().sync_all()?;
    drop(output);
    fs::rename(path, target)?;
    Ok(games)
}

fn failed_write() -> io::Error {
    io::Error::other("a previous write to this PGN file failed")
}
