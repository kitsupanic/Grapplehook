//! Helpers shared by the tests: a local HTTP server standing in for the
//! GitHub / martin-riedl.de release hosts, fake Tool scripts, and fakes that hang.
#![allow(dead_code)] // each test file uses a different subset

use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::TcpListener;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

/// What the fake server answers for a path.
pub enum Reply {
    Ok(Vec<u8>),
    Redirect(&'static str),
    /// Promises the whole body but closes the connection after half of it.
    Truncated(Vec<u8>),
    /// A host that answers HEAD with 405 (some do) and GET normally.
    NoHead(Vec<u8>),
    /// The first body once, then the second: a release published between two requests.
    FirstThen(Vec<u8>, Vec<u8>),
}

/// Requests the server has seen, as `"GET /path"`.
pub type Log = Arc<Mutex<Vec<String>>>;

/// Serve fixed replies on 127.0.0.1 (anything else is 404). Returns the base URL.
pub fn serve(routes: Vec<(&'static str, Reply)>) -> String {
    serve_with_log(routes).0
}

pub fn serve_with_log(routes: Vec<(&'static str, Reply)>) -> (String, Log) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let routes: Arc<Mutex<HashMap<_, _>>> = Arc::new(Mutex::new(routes.into_iter().collect()));
    let log = Log::default();
    let seen = log.clone();
    thread::spawn(move || {
        for mut stream in listener.incoming().flatten() {
            let mut buf = [0u8; 4096];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]);
            let mut words = request.split_whitespace();
            let method = words.next().unwrap_or("GET").to_string();
            let path = words.next().unwrap_or("/").to_string();
            seen.lock().unwrap().push(format!("{method} {path}"));
            let head = method == "HEAD";
            let ok = |len: usize, body: &[u8]| {
                let mut r = format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
                )
                .into_bytes();
                if !head {
                    r.extend_from_slice(body);
                }
                r
            };
            let mut routes = routes.lock().unwrap();
            if let Some(reply @ Reply::FirstThen(..)) = routes.get_mut(path.as_str()) {
                if let Reply::FirstThen(first, then) = std::mem::replace(reply, Reply::Ok(vec![])) {
                    *reply = Reply::Ok(then);
                    let _ = stream.write_all(&ok(first.len(), &first));
                    continue;
                }
            }
            let response = match routes.get(path.as_str()) {
                Some(Reply::Ok(body)) => ok(body.len(), body),
                Some(Reply::Truncated(body)) => ok(body.len(), &body[..body.len() / 2]),
                Some(Reply::NoHead(_)) if head => {
                    b"HTTP/1.1 405 Method Not Allowed\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                        .to_vec()
                }
                Some(Reply::NoHead(body)) => ok(body.len(), body),
                Some(Reply::FirstThen(..)) => unreachable!("replaced above"),
                Some(Reply::Redirect(to)) => format!(
                    "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                )
                .into_bytes(),
                None => b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    .to_vec(),
            };
            let _ = stream.write_all(&response);
        }
    });
    (base, log)
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

pub fn close(actual: Option<f64>, expected: f64) -> bool {
    actual.is_some_and(|a| (a - expected).abs() < 0.01)
}

pub fn dir_entries(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .map(|rd| {
            rd.flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

/// Shell commands for a Tool that hangs: it starts a child, writes `<child pid> <own pid>` to
/// `pids_file`, and then waits forever. A single line, so it also fits in a `case` arm.
pub fn hang_recording_pids(pids_file: &Path) -> String {
    format!(
        "sleep 300 & echo \"$! $$\" > '{}'; while :; do wait; done",
        pids_file.display()
    )
}

/// The pids a hanging fake wrote to `pids_file`, once it is running.
pub fn wait_for_pids(pids_file: &Path) -> Vec<String> {
    // Generous: on macOS each freshly written fake Tool is scanned before it first runs.
    let deadline = Instant::now() + Duration::from_secs(60);
    loop {
        match fs::read_to_string(pids_file) {
            Ok(text) if text.trim().contains(' ') => {
                return text.split_whitespace().map(String::from).collect()
            }
            _ => {
                assert!(Instant::now() < deadline, "a fake never started");
                thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

pub fn is_alive(pid: &str) -> bool {
    Command::new("kill")
        .args(["-0", pid])
        .stderr(Stdio::null())
        .status()
        .unwrap()
        .success()
}

/// Fails unless every one of `pids` is gone within a few seconds.
pub fn assert_all_dead(pids: &[String]) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while pids.iter().any(|pid| is_alive(pid)) {
        assert!(Instant::now() < deadline, "still running: {pids:?}");
        thread::sleep(Duration::from_millis(50));
    }
}

/// Write an executable shell script standing in for a Tool.
pub fn fake_tool(dir: &Path, name: &str, body: &str) -> PathBuf {
    let path = dir.join(name);
    fs::write(&path, format!("#!/bin/sh\n{body}\n")).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o755)).unwrap();
    path
}

/// The Tools `check_tools` reports Missing, as the UI hands them to `install_size`.
pub fn missing(paths: &grapplehook_lib::core::ToolPaths) -> Vec<grapplehook_lib::core::Tool> {
    grapplehook_lib::core::check_tools(paths, &grapplehook_lib::core::CancelHandle::default())
        .tools
        .into_iter()
        .filter(|t| t.state == grapplehook_lib::core::ToolState::Missing)
        .map(|t| t.tool)
        .collect()
}
