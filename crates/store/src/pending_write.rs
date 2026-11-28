//! Publication and cleanup of a file being prepared by a desktop task.

use std::io;
use std::path::Path;
use std::sync::{Arc, Condvar, Mutex, PoisonError, Weak};

use crate::StagingDirectory;

enum Phase {
    Preparing(Vec<Weak<StagingDirectory>>),
    Publishing,
    Finished,
    Abandoned,
}

/// The right to publish a staged write. Its owner can abandon the work before publication.
pub struct PendingWrite {
    phase: Mutex<Phase>,
    finished: Condvar,
}

impl Default for PendingWrite {
    fn default() -> Self {
        Self {
            phase: Mutex::new(Phase::Preparing(Vec::new())),
            finished: Condvar::new(),
        }
    }
}

impl PendingWrite {
    pub(super) fn stage<T>(
        &self,
        target: &Path,
        create: impl FnOnce(Arc<StagingDirectory>) -> io::Result<T>,
    ) -> io::Result<T> {
        let mut phase = self.phase.lock().unwrap_or_else(PoisonError::into_inner);
        let Phase::Preparing(directories) = &mut *phase else {
            return Err(io::ErrorKind::Interrupted.into());
        };
        let directory = Arc::new(StagingDirectory::beside(target)?);
        directories.push(Arc::downgrade(&directory));
        create(directory)
    }

    pub(super) fn begin_publication(&self) -> io::Result<()> {
        let mut phase = self.phase.lock().unwrap_or_else(PoisonError::into_inner);
        if !matches!(*phase, Phase::Preparing(_)) {
            return Err(io::ErrorKind::Interrupted.into());
        }
        *phase = Phase::Publishing;
        Ok(())
    }

    /// Discards staged files, even if the worker is blocked on input. If publication has
    /// already begun, waits until the owner has completed all parts of the transaction.
    pub fn abandon(&self) {
        let phase = self.phase.lock().unwrap_or_else(PoisonError::into_inner);
        let mut phase = self
            .finished
            .wait_while(phase, |phase| matches!(phase, Phase::Publishing))
            .unwrap_or_else(PoisonError::into_inner);
        if let Phase::Preparing(directories) = &*phase {
            for directory in directories.iter().filter_map(Weak::upgrade) {
                directory.remove();
            }
            *phase = Phase::Abandoned;
        }
    }

    /// Called by the owner after its last write, including any catalog update.
    pub fn finish(&self) {
        *self.phase.lock().unwrap_or_else(PoisonError::into_inner) = Phase::Finished;
        self.finished.notify_all();
    }
}
