//! A stand-in Tool for tests/cancel_native.rs, in Rust so that it also runs on Windows, where
//! the other tests' shell-script fakes cannot. Copied into place as `yt-dlp`: it answers
//! `--version`, otherwise it starts a child that hangs, writes both process ids to `pids.txt`
//! beside itself, and hangs too, like a yt-dlp that has started aria2c.

use std::process::Command;
use std::time::Duration;
use std::{env, fs, thread};

fn hang() -> ! {
    loop {
        thread::sleep(Duration::from_secs(60));
    }
}

// Never waits on its child: both hang until the test's cancel kills them.
#[allow(clippy::zombie_processes)]
fn main() {
    let exe = env::current_exe().unwrap();
    match env::args().nth(1).as_deref() {
        Some("--version") => println!("2025.01.01"),
        Some("--child") => hang(),
        _ => {
            let child = Command::new(&exe).arg("--child").spawn().unwrap();
            // Written whole, then renamed, so the test never reads half of it.
            let tmp = exe.with_file_name("pids.tmp");
            fs::write(&tmp, format!("{}\n{}\n", std::process::id(), child.id())).unwrap();
            fs::rename(tmp, exe.with_file_name("pids.txt")).unwrap();
            hang()
        }
    }
}
