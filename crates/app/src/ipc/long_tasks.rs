//! Background file writes considered when the player closes the window.

use std::io;
use std::sync::{Arc, PoisonError, Weak};

use zobrist_store::PendingWrite;

use super::{DatabaseRequest, DesktopRequest, Response, SharedGames};

#[derive(Clone, Copy)]
enum Task {
    Import,
}

impl Task {
    fn close_message(self) -> &'static str {
        match self {
            Self::Import => {
                "An import is still running. zobrist closes when it finishes. Close again to stop it and quit."
            }
        }
    }
}

#[derive(Default)]
pub(super) struct LongTasks {
    abandoning: bool,
    running: Vec<(Task, Weak<PendingWrite>)>,
}

/// A blocking worker holds this until the last part of its file operation has finished.
#[derive(Default)]
pub struct RunningWrite(Arc<PendingWrite>);

impl RunningWrite {
    /// The worker must obtain permission before it publishes its staged file.
    #[must_use]
    pub fn permission(&self) -> &PendingWrite {
        &self.0
    }
}

impl Drop for RunningWrite {
    fn drop(&mut self) {
        self.0.finish();
    }
}

impl SharedGames {
    /// Registers a long request before it enters the blocking worker queue.
    pub fn start_write(&self, request: &DesktopRequest) -> io::Result<Option<RunningWrite>> {
        if !matches!(
            request,
            DesktopRequest::Databases(DatabaseRequest::ImportDatabase { .. })
        ) {
            return Ok(None);
        }
        let kind = Task::Import;
        let mut tasks = self
            .long_tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        if tasks.abandoning {
            return Err(io::ErrorKind::Interrupted.into());
        }
        tasks.running.retain(|(_, write)| write.strong_count() > 0);
        let write = RunningWrite::default();
        tasks.running.push((kind, Arc::downgrade(&write.0)));
        Ok(Some(write))
    }

    pub(super) fn prepare_to_close(&self, abandon: bool) -> Response {
        let mut tasks = self
            .long_tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        tasks.running.retain(|(_, write)| write.strong_count() > 0);
        let message = if abandon {
            tasks.abandoning = true;
            let writes: Vec<_> = tasks
                .running
                .drain(..)
                .filter_map(|(_, write)| write.upgrade())
                .collect();
            // Stop every unpublished task before waiting for any task already committing.
            for write in &writes {
                write.abandon();
            }
            for write in writes {
                write.finish_abandoning();
            }
            None
        } else {
            tasks.running.first().map(|(kind, _)| kind.close_message())
        };
        Response::PrepareToClose { message }
    }

    /// Allows new background writes when saving the boards or closing the window failed.
    pub fn resume_writes(&self) {
        self.long_tasks
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .abandoning = false;
    }
}
