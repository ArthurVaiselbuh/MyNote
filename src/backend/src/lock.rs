use std::fs::{File, OpenOptions};
use std::path::Path;
#[cfg(unix)]
use std::time::{Duration, Instant};

pub const LOCK_FILE_NAME: &str = ".mynote.lock";

// Held for the lifetime of the Store that owns it; the OS releases the
// underlying handle (and with it the exclusive lock) on process exit even
// after a crash or force-kill, so a lock can never outlive its owner and
// there is no "stale lock" state to detect or clean up.
pub struct NotebookLock {
    #[allow(dead_code)]
    file: File,
}

pub enum LockError {
    HeldElsewhere,
    Io(String),
}

impl NotebookLock {
    pub fn acquire(root: &Path) -> Result<NotebookLock, LockError> {
        let path = root.join(LOCK_FILE_NAME);
        acquire_exclusive(&path).map(|file| NotebookLock { file })
    }
}

#[cfg(windows)]
fn acquire_exclusive(path: &Path) -> Result<File, LockError> {
    use std::os::windows::fs::OpenOptionsExt;
    const ERROR_SHARING_VIOLATION: i32 = 32;
    OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .share_mode(0)
        .open(path)
        .map_err(|e| {
            if e.raw_os_error() == Some(ERROR_SHARING_VIOLATION) {
                LockError::HeldElsewhere
            } else {
                LockError::Io(e.to_string())
            }
        })
}

#[cfg(unix)]
fn acquire_exclusive(path: &Path) -> Result<File, LockError> {
    use std::io::ErrorKind;
    use std::os::unix::io::AsRawFd;
    // Declared by hand rather than pulling in the `libc` crate: flock(2) is
    // part of the platform C library every unix target already links.
    extern "C" {
        fn flock(fd: i32, operation: i32) -> i32;
    }
    const LOCK_EX: i32 = 2;
    const LOCK_NB: i32 = 4;
    let file = OpenOptions::new()
        .create(true)
        .write(true)
        .open(path)
        .map_err(|e| LockError::Io(e.to_string()))?;
    let deadline = Instant::now() + FORK_RACE_BUDGET;
    loop {
        if unsafe { flock(file.as_raw_fd(), LOCK_EX | LOCK_NB) } == 0 {
            return Ok(file);
        }

        let e = std::io::Error::last_os_error();
        if Instant::now() >= deadline {
            return match e.kind() {
                ErrorKind::WouldBlock => Err(LockError::HeldElsewhere),
                _ => Err(LockError::Io(format!("flock: {e}"))),
            };
        }
        match e.kind() {
            ErrorKind::Interrupted => continue,
            ErrorKind::WouldBlock => std::thread::sleep(RETRY_PAUSE),
            _ => return Err(LockError::Io(format!("flock: {e}"))),
        }
    }
}
#[cfg(unix)]
const FORK_RACE_BUDGET: Duration = Duration::from_millis(500);
#[cfg(unix)]
const RETRY_PAUSE: Duration = Duration::from_millis(2);
