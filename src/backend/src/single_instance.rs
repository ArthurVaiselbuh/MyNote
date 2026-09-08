use crate::file_watcher::{watch_directory_and_scan, DirectoryWatcher};
use crate::lock::{LockError, NotebookLock};
use crate::{err, settings, tray};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use tauri::{AppHandle, Manager};

pub type InstanceState = Mutex<Option<InstanceOwner>>;

pub struct InstanceOwner {
    listener: Option<DirectoryWatcher>,
    _lock: NotebookLock,
    requests: PathBuf,
}

impl InstanceOwner {
    fn acquire(requests: &Path) -> Result<Self, LockError> {
        fs::create_dir_all(requests).map_err(|e| LockError::Io(err(e)))?;
        let lock = NotebookLock::acquire(requests)?;
        fs::write(requests.join("owner"), std::process::id().to_string())
            .map_err(|e| LockError::Io(err(e)))?;
        Ok(Self {
            _lock: lock,
            requests: requests.to_path_buf(),
            listener: None,
        })
    }

    fn listen(&mut self, app: AppHandle) -> Result<(), String> {
        if self.listener.is_some() {
            return Ok(());
        }
        let requests = self.requests.clone();
        self.listener = Some(watch_directory_and_scan(&self.requests, move || {
            if take_reveal_requests(&requests) {
                let handle = app.clone();
                let _ = app.run_on_main_thread(move || tray::reveal_window(&handle));
            }
        })?);
        Ok(())
    }
}

pub fn claim_or_reveal() -> Result<Option<InstanceOwner>, String> {
    let requests = settings::app_dir().join("instance");
    match InstanceOwner::acquire(&requests) {
        Ok(owner) => Ok(Some(owner)),
        Err(LockError::HeldElsewhere) => {
            request_reveal(&requests)?;
            Ok(None)
        }
        Err(LockError::Io(error)) => Err(error),
    }
}

pub fn sync(app: &AppHandle, enabled: bool) -> Result<(), String> {
    let state = app.state::<InstanceState>();
    let mut owner = state.lock().map_err(err)?;
    if !enabled {
        *owner = None;
        return Ok(());
    }
    if owner.is_none() {
        *owner = Some(
            InstanceOwner::acquire(&settings::app_dir().join("instance")).map_err(|error| {
                match error {
                    LockError::HeldElsewhere =>
                        "Single instance is already enabled in another MyNote window. Close that window first.".into(),
                    LockError::Io(error) => error,
                }
            })?,
        );
    }
    if let Err(error) = owner.as_mut().unwrap().listen(app.clone()) {
        *owner = None;
        return Err(error);
    }
    Ok(())
}

fn request_reveal(requests: &Path) -> Result<(), String> {
    #[cfg(windows)]
    allow_owner_to_take_focus(requests);
    fs::write(
        requests.join(format!("reveal-{}", uuid::Uuid::new_v4())),
        [],
    )
    .map_err(err)
}

#[cfg(windows)]
fn allow_owner_to_take_focus(requests: &Path) {
    #[link(name = "user32")]
    extern "system" {
        fn AllowSetForegroundWindow(process_id: u32) -> i32;
    }
    if let Some(process_id) = fs::read_to_string(requests.join("owner"))
        .ok()
        .and_then(|pid| pid.parse().ok())
    {
        unsafe { AllowSetForegroundWindow(process_id) };
    }
}

fn take_reveal_requests(requests: &Path) -> bool {
    let Ok(entries) = fs::read_dir(requests) else {
        return false;
    };
    let mut requested = false;
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name
            .strip_prefix("reveal-")
            .is_some_and(|id| uuid::Uuid::parse_str(id).is_ok())
            && fs::remove_file(entry.path()).is_ok()
        {
            requested = true;
        }
    }
    requested
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::Duration;

    #[test]
    fn watcher_consumes_pending_and_new_reveal_requests() {
        let root = tempfile::tempdir().unwrap();
        let mut owner = InstanceOwner::acquire(root.path()).ok().unwrap();
        request_reveal(root.path()).unwrap();
        let requests = root.path().to_path_buf();
        let (revealed_tx, revealed_rx) = mpsc::channel();
        owner.listener = Some(
            watch_directory_and_scan(root.path(), move || {
                if take_reveal_requests(&requests) {
                    revealed_tx.send(()).unwrap();
                }
            })
            .unwrap(),
        );
        revealed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        for _ in 0..3 {
            request_reveal(root.path()).unwrap();
            revealed_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        }
        assert!(!take_reveal_requests(root.path()));
        drop(owner);
        assert_eq!(
            revealed_rx.recv_timeout(Duration::from_secs(5)),
            Err(mpsc::RecvTimeoutError::Disconnected),
        );
        assert!(InstanceOwner::acquire(root.path()).is_ok());
    }

    #[test]
    fn ownership_is_exclusive_and_released_on_drop() {
        let root = tempfile::tempdir().unwrap();
        let owner = InstanceOwner::acquire(root.path()).ok().unwrap();
        assert!(matches!(
            InstanceOwner::acquire(root.path()),
            Err(LockError::HeldElsewhere)
        ));
        drop(owner);
        assert!(InstanceOwner::acquire(root.path()).is_ok());
    }

    #[test]
    fn reveal_requests_survive_startup_and_are_consumed_once() {
        let root = tempfile::tempdir().unwrap();
        let _owner = InstanceOwner::acquire(root.path()).ok().unwrap();
        request_reveal(root.path()).unwrap();
        request_reveal(root.path()).unwrap();
        assert!(take_reveal_requests(root.path()));
        assert!(!take_reveal_requests(root.path()));
        assert!(root.path().join(crate::lock::LOCK_FILE_NAME).exists());
        request_reveal(root.path()).unwrap();
        assert!(take_reveal_requests(root.path()));
    }
}
