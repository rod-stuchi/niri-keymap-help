use std::{
    collections::{BTreeMap, BTreeSet, hash_map::DefaultHasher},
    fs,
    hash::{Hash, Hasher},
    path::{Path, PathBuf},
    sync::mpsc::{self, Sender},
    thread,
    time::Duration,
};

use crate::{
    config::{self, LoadReport},
    timing,
};

/// Poll contents on a worker thread: handles atomic saves, symlink changes,
/// missing directories, and creation of previously missing optional includes.
/// No GTK work or filesystem reads run in the UI event loop: reports are
/// pushed through an async channel the UI awaits, so nothing polls it.
pub struct Watcher {
    pub updates: async_channel::Receiver<LoadReport>,
    stop: Sender<()>,
}

impl Watcher {
    pub fn start(path: PathBuf) -> Self {
        let (sender, updates) = async_channel::unbounded();
        let (stop, stopped) = mpsc::channel();
        thread::spawn(move || {
            let mut files = BTreeSet::from([path.clone()]);
            let mut previous = BTreeMap::new();
            loop {
                let current = fingerprints(&files);
                if current != previous {
                    let before = current;
                    let report = config::load(&path);
                    timing::mark("config loaded (worker)");
                    if report.result.is_ok() {
                        files = report.files.clone();
                    } else {
                        files.extend(report.files.iter().cloned());
                    }
                    // Keep pre-load fingerprints for existing dependencies so a
                    // save that races with parsing is caught on the next poll.
                    previous = fingerprints(&files);
                    for (file, stamp) in before {
                        if files.contains(&file) {
                            previous.insert(file, stamp);
                        }
                    }
                    if sender.send_blocking(report).is_err() {
                        break;
                    }
                }
                match stopped.recv_timeout(Duration::from_millis(350)) {
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                    _ => break,
                }
            }
        });
        Self { updates, stop }
    }
}

impl Drop for Watcher {
    fn drop(&mut self) {
        let _ = self.stop.send(());
    }
}

fn fingerprints(files: &BTreeSet<PathBuf>) -> BTreeMap<PathBuf, u64> {
    files
        .iter()
        .map(|file| (file.clone(), fingerprint(file)))
        .collect()
}

fn fingerprint(path: &Path) -> u64 {
    let mut hash = DefaultHasher::new();
    match fs::read(path) {
        Ok(contents) => {
            0u8.hash(&mut hash);
            contents.hash(&mut hash);
        }
        Err(error) => {
            1u8.hash(&mut hash);
            error.kind().hash(&mut hash);
        }
    }
    hash.finish()
}
