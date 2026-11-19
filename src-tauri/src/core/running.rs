//! The Tool Runs in flight (downloads, video lookups) and the Tool Change in progress (install,
//! delete, yt-dlp update). Runs share the Tools; a change needs them to itself, so whichever
//! comes second is refused at once, never made to wait. A download can be cancelled by id, and
//! quitting cancels everything and waits until the Tool processes are gone.

use std::collections::{HashMap, HashSet};
use std::fmt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::download::{download, DownloadError, DownloadOptions};
use super::process::CancelHandle;
use super::progress::Progress;
use super::tools::ToolPaths;

/// How long quitting waits for cancelled Tool processes to be gone. Core kills them
/// outright after 5 seconds, so this only runs out if something is badly stuck, or if an
/// install is in the middle of one source's download (it only checks cancel between sources).
const SHUTDOWN_WAIT: Duration = Duration::from_secs(8);

/// Why a Tool Run or a Tool Change was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Busy {
    /// A Tool Change, refused because Tool Runs are in flight.
    Runs,
    /// A Tool Run, or a second Tool Change, refused because a Tool Change is in progress.
    Change,
}

impl fmt::Display for Busy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Runs => "Wait for the downloads and lookups to finish.",
            Self::Change => "Wait until the Tools finish changing.",
        })
    }
}

impl std::error::Error for Busy {}

#[derive(Clone, Default)]
pub struct Running(Arc<Shared>);

#[derive(Default)]
struct Shared {
    next_id: AtomicU64,
    state: Mutex<State>,
    emptied: Condvar,
}

/// Everything in flight, behind one lock so that checking the gate and registering are one step.
#[derive(Default)]
struct State {
    /// Every Tool Run and the Tool Change, by id.
    handles: HashMap<u64, CancelHandle>,
    /// The id of the Tool Change in progress, if any.
    changing: Option<u64>,
    /// The ids in `handles` that are downloads, as opposed to lookups and checks.
    downloads: HashSet<u64>,
}

impl State {
    /// May a Tool Run start (or, with `change`, a Tool Change)?
    fn gate(&self, change: bool) -> Result<(), Busy> {
        if self.changing.is_some() {
            Err(Busy::Change)
        } else if change && !self.handles.is_empty() {
            Err(Busy::Runs)
        } else {
            Ok(())
        }
    }
}

/// Takes its job out of the running set when dropped, so also when the job panics.
struct Registered<'a>(&'a Running, u64);

impl Drop for Registered<'_> {
    fn drop(&mut self) {
        self.0.remove(self.1);
    }
}

impl Running {
    /// Start a download, like [`download`], and return its id. `on_progress` and `on_done` get
    /// that id; `on_done` runs once, with the result, after the download has left the running
    /// set. Refused with [`Busy::Change`] (as its text) while a Tool Change is in progress.
    pub fn download(
        &self,
        paths: &ToolPaths,
        options: DownloadOptions,
        on_progress: impl Fn(u64, Progress) + Send + 'static,
        on_done: impl FnOnce(u64, Result<PathBuf, DownloadError>) + Send + 'static,
    ) -> Result<u64, String> {
        let id = self.next_id();
        // Held until the download is registered, so that no Tool Change can start in between.
        let mut state = self.0.state.lock().unwrap();
        state.gate(false).map_err(|busy| busy.to_string())?;
        let task = download(paths, options, move |progress| on_progress(id, progress))?;
        state.handles.insert(id, task.canceller());
        state.downloads.insert(id);
        drop(state);
        let this = self.clone();
        thread::spawn(move || {
            let result = task.wait();
            this.remove(id);
            on_done(id, result);
        });
        Ok(id)
    }

    /// Run `job` (a video lookup) as a Tool Run, so that quitting cancels it: it gets the
    /// handle to pass on to the core call it makes. Refused while a Tool Change is in progress.
    pub fn run<T>(&self, job: impl FnOnce(&CancelHandle) -> T) -> Result<T, Busy> {
        let (_registered, cancel) = self.enter(false)?;
        Ok(job(&cancel))
    }

    /// Run `job` (an install, a delete, a yt-dlp update) as the Tool Change, with the Tools to
    /// itself, and with a handle that quitting cancels. Refused while any Tool Run or another
    /// Tool Change is in flight.
    pub fn change<T>(&self, job: impl FnOnce(&CancelHandle) -> T) -> Result<T, Busy> {
        let (_registered, cancel) = self.enter(true)?;
        Ok(job(&cancel))
    }

    /// Would quitting now throw work away: a download, or a Tool Change such as an install?
    /// Quitting asks first if so. A lookup does not count: it is quick to run again.
    pub fn quitting_loses_work(&self) -> bool {
        let state = self.0.state.lock().unwrap();
        !state.downloads.is_empty() || state.changing.is_some()
    }

    /// Cancel the download with this id. Does nothing if it has ended.
    pub fn cancel(&self, id: u64) {
        if let Some(handle) = self.0.state.lock().unwrap().handles.get(&id) {
            handle.cancel();
        }
    }

    /// Cancel everything that is running and wait until its Tool processes are gone, so that
    /// quitting leaves nothing running in the background.
    pub fn cancel_all_and_wait(&self) {
        let deadline = Instant::now() + SHUTDOWN_WAIT;
        let mut state = self.0.state.lock().unwrap();
        state.handles.values().for_each(CancelHandle::cancel);
        while !state.handles.is_empty() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            state = self.0.emptied.wait_timeout(state, left).unwrap().0;
        }
    }

    fn enter(&self, change: bool) -> Result<(Registered<'_>, CancelHandle), Busy> {
        let (id, cancel) = (self.next_id(), CancelHandle::default());
        let mut state = self.0.state.lock().unwrap();
        state.gate(change)?;
        state.handles.insert(id, cancel.clone());
        if change {
            state.changing = Some(id);
        }
        Ok((Registered(self, id), cancel))
    }

    fn next_id(&self) -> u64 {
        self.0.next_id.fetch_add(1, Ordering::Relaxed) + 1
    }

    fn remove(&self, id: u64) {
        let mut state = self.0.state.lock().unwrap();
        state.handles.remove(&id);
        state.downloads.remove(&id);
        if state.changing == Some(id) {
            state.changing = None;
        }
        drop(state);
        self.0.emptied.notify_all();
    }
}
