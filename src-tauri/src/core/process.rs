//! Running a Tool: reading its output line by line, and stopping it, and everything it
//! started (yt-dlp starts aria2c and ffmpeg), when a download is cancelled.

use std::io::{self, Read};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

/// How often a running Tool is checked for having exited or been cancelled.
const POLL: Duration = Duration::from_millis(20);
/// How long a terminated Tool gets to exit before it is killed outright.
const KILL_GRACE: Duration = Duration::from_secs(5);
/// How long output is still read after the Tool has exited. Reading normally ends sooner,
/// when the pipes close; this only caps a child that outlived the Tool and keeps them open for
/// good. Generous, because on a busy machine the last of the output (a final line without `\n`
/// is only sent once the pipe closes) can arrive well after the exit.
const DRAIN: Duration = Duration::from_secs(5);

/// Cancels a download from any thread. Every Tool process a download runs, now or in a
/// later stage, honours it: one that starts after the cancel is stopped straight away.
#[derive(Debug, Clone, Default)]
pub struct CancelHandle {
    cancelled: Arc<AtomicBool>,
    /// Past this, this copy (and its clones) counts as cancelled too.
    deadline: Option<Instant>,
}

impl CancelHandle {
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::SeqCst);
    }

    pub fn is_cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst) || self.timed_out()
    }

    /// A copy that is also cancelled once `wait` has passed, or at the deadline it already
    /// had if that is sooner. Cancelling either copy still cancels both.
    pub fn with_deadline(&self, wait: Duration) -> Self {
        let at = Instant::now() + wait;
        CancelHandle {
            cancelled: self.cancelled.clone(),
            deadline: Some(self.deadline.map_or(at, |d| d.min(at))),
        }
    }

    /// Has the deadline passed?
    pub fn timed_out(&self) -> bool {
        self.deadline.is_some_and(|d| Instant::now() >= d)
    }
}

/// The "cancelled" outcome of a video lookup or a yt-dlp update (a download has its own error).
pub(super) const CANCELLED: &str = "Cancelled";

#[derive(Clone, Copy)]
enum Stream {
    Stdout,
    Stderr,
}

/// Run `cmd` to the end, calling `on_line` for every line of stdout and stderr.
/// Lines end at `\n`, `\r` or `\r\n`, so aria2c's `\r` readouts count as lines.
/// Once `cancel` is set, the Tool and its children are stopped: SIGTERM first (taskkill on
/// Windows), then SIGKILL if it is still running after [`KILL_GRACE`].
pub(super) fn run_streaming(
    cmd: Command,
    cancel: &CancelHandle,
    mut on_line: impl FnMut(&str),
) -> io::Result<ExitStatus> {
    run_lines(cmd, cancel, |_, line| on_line(line))
}

/// What a finished Tool run printed. Each stream's lines are joined with `\n`.
pub(super) struct Captured {
    pub status: ExitStatus,
    pub stdout: String,
    pub stderr: String,
}

/// Run `cmd` to the end, like [`run_streaming`], and return all of its output.
pub(super) fn run_captured(cmd: Command, cancel: &CancelHandle) -> io::Result<Captured> {
    let (mut stdout, mut stderr) = (String::new(), String::new());
    let status = run_lines(cmd, cancel, |stream, line| {
        let out = match stream {
            Stream::Stdout => &mut stdout,
            Stream::Stderr => &mut stderr,
        };
        if !out.is_empty() {
            out.push('\n');
        }
        out.push_str(line);
    })?;
    Ok(Captured {
        status,
        stdout,
        stderr,
    })
}

/// Starts a Tool, retrying briefly on "Text file busy". On Linux a Tool written moments
/// ago (just installed, or a test's fake) cannot be run while a process another thread is
/// forking still holds the write handle, until that child execs.
#[cfg(unix)]
pub(crate) fn spawn(cmd: &mut Command) -> io::Result<Child> {
    for attempt in 1..=20 {
        match cmd.spawn() {
            Err(err) if err.raw_os_error() == Some(libc::ETXTBSY) => {
                thread::sleep(Duration::from_millis(10 * attempt))
            }
            other => return other,
        }
    }
    cmd.spawn()
}

/// Windows: no console window flashing up for every Tool, version checks included.
#[cfg(windows)]
pub(crate) fn spawn(cmd: &mut Command) -> io::Result<Child> {
    use std::os::windows::process::CommandExt;
    cmd.creation_flags(CREATE_NO_WINDOW).spawn()
}

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

fn run_lines(
    mut cmd: Command,
    cancel: &CancelHandle,
    mut on_line: impl FnMut(Stream, &str),
) -> io::Result<ExitStatus> {
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    in_own_process_group(&mut cmd);
    let mut child = spawn(&mut cmd)?;

    let (tx, rx) = mpsc::channel();
    read_lines(
        child.stdout.take().expect("stdout is piped"),
        Stream::Stdout,
        tx.clone(),
    );
    read_lines(
        child.stderr.take().expect("stderr is piped"),
        Stream::Stderr,
        tx,
    );

    let mut reading = true;
    let mut terminated_at: Option<Instant> = None;
    let mut exited: Option<(ExitStatus, Instant)> = None;
    loop {
        if reading {
            match rx.recv_timeout(POLL) {
                Ok((stream, line)) => on_line(stream, &line),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => reading = false,
            }
        } else if exited.is_none() {
            thread::sleep(POLL);
        }

        if exited.is_none() {
            if let Some(status) = child.try_wait()? {
                // Sweep up anything that ignored the terminate and is still in the group. Unix
                // only: a group outlives its leader, but on Windows taskkill already forced the
                // whole tree, and the pid may by now belong to an unrelated process.
                if cfg!(unix) && cancel.is_cancelled() {
                    kill_tree(child.id(), true);
                }
                exited = Some((status, Instant::now()));
            } else if cancel.is_cancelled() {
                match terminated_at {
                    None => {
                        kill_tree(child.id(), false);
                        terminated_at = Some(Instant::now());
                    }
                    // Unix only: taskkill already forced the tree, and running it again every
                    // poll would start a stream of taskkill processes for one that is stuck.
                    Some(at) if cfg!(unix) && at.elapsed() >= KILL_GRACE => {
                        kill_tree(child.id(), true)
                    }
                    Some(_) => {}
                }
            }
        }

        if let Some((status, at)) = exited {
            if !reading || at.elapsed() >= DRAIN {
                return Ok(status);
            }
        }
    }
}

/// Why a Tool that did not succeed ended, e.g. `yt-dlp exited with code 1`.
pub(super) fn describe_failure(name: &str, status: ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(signal) = status.signal() {
            return format!("{name} terminated by signal {signal}");
        }
    }
    match status.code() {
        Some(code) => format!("{name} exited with code {code}"),
        None => format!("{name} did not finish"),
    }
}

fn read_lines(mut pipe: impl Read + Send + 'static, stream: Stream, tx: Sender<(Stream, String)>) {
    thread::spawn(move || {
        let mut line = Vec::new();
        let mut chunk = [0u8; 8192];
        let flush = |line: &mut Vec<u8>| {
            if !line.is_empty() {
                let _ = tx.send((stream, String::from_utf8_lossy(line).into_owned()));
                line.clear();
            }
        };
        loop {
            match pipe.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => {
                    for &byte in &chunk[..n] {
                        if byte == b'\n' || byte == b'\r' {
                            flush(&mut line);
                        } else {
                            line.push(byte);
                        }
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::Interrupted => {}
                Err(_) => break,
            }
        }
        flush(&mut line);
    });
}

/// Unix: the Tool leads its own process group, so the whole tree can be signalled at once.
#[cfg(unix)]
fn in_own_process_group(cmd: &mut Command) {
    use std::os::unix::process::CommandExt;
    cmd.process_group(0);
}

/// Stop the Tool with process id `pid` and everything it started. `force` is SIGKILL
/// rather than SIGTERM; taskkill always forces.
#[cfg(unix)]
fn kill_tree(pid: u32, force: bool) {
    let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
    // A negative pid is a process group, and the Tool's group id is its pid.
    // SAFETY: kill(2) takes plain integers and has no memory-safety preconditions.
    unsafe { libc::kill(-(pid as libc::pid_t), signal) };
}

#[cfg(windows)]
fn kill_tree(pid: u32, _force: bool) {
    use std::os::windows::process::CommandExt;
    let _ = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
}
