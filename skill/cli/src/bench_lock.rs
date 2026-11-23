//! A lock that lets one benchmark run at a time for this user on this machine.
//!
//! It is an operating-system file lock, which the system releases when the holder exits
//! for any reason, so a crashed or killed run never leaves a stale lock behind.

use std::env;
use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use crate::failure::{Failure, Outcome, say};
use crate::proc;

/// Holds the benchmark lock until dropped.
#[derive(Debug)]
pub struct BenchLock {
    _file: File,
}

/// Takes the lock, waiting while another benchmark holds it.
pub fn acquire() -> Outcome<BenchLock> {
    let path = lock_path();
    let failed = |error: std::io::Error| {
        Failure::new(format!(
            "cannot take the benchmark lock {}: {error}",
            path.display()
        ))
    };
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(&path)
        .map_err(failed)?;
    match file.try_lock() {
        Ok(()) => {}
        Err(TryLockError::WouldBlock) => {
            say("another benchmark is running; waiting so the runs do not compete");
            file.lock().map_err(failed)?;
        }
        Err(TryLockError::Error(error)) => return Err(failed(error)),
    }
    Ok(BenchLock { _file: file })
}

/// `IMPECCABLE_BENCH_LOCK`, else a per-user file in the machine's shared temp directory.
fn lock_path() -> PathBuf {
    if let Some(path) = proc::env_nonempty("IMPECCABLE_BENCH_LOCK") {
        return PathBuf::from(path);
    }
    let user = ["USER", "LOGNAME", "USERNAME"]
        .into_iter()
        .find_map(proc::env_nonempty)
        .unwrap_or_else(|| "user".to_owned());
    let user: String = user
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    shared_temp_dir().join(format!("impeccable-bench-{user}.lock"))
}

/// `/tmp` on Unix, because `TMPDIR` can differ between a sandbox and a terminal, and two
/// runs that disagree on the lock's path would not exclude each other.
fn shared_temp_dir() -> PathBuf {
    let tmp = Path::new("/tmp");
    if cfg!(unix) && tmp.is_dir() {
        tmp.to_path_buf()
    } else {
        env::temp_dir()
    }
}
