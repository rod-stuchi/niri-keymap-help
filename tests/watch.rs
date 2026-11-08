use std::{
    fs, thread,
    time::{Duration, Instant},
};

use niri_keymap_help::{config::LoadReport, watch::Watcher};
use tempfile::TempDir;

fn next_matching(watcher: &Watcher, predicate: impl Fn(&LoadReport) -> bool) -> LoadReport {
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        match watcher.updates.try_recv() {
            Ok(report) if predicate(&report) => return report,
            Ok(_) => {}
            Err(_) => {
                assert!(
                    Instant::now() < deadline,
                    "watcher should notice config changes"
                );
                thread::sleep(Duration::from_millis(10));
            }
        }
    }
}

#[test]
fn detects_atomic_saves_invalid_edits_recovery_and_optional_creation() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.kdl");
    fs::write(&path, "include optional=true \"extra.kdl\"\n").unwrap();
    let watcher = Watcher::start(path.clone());
    next_matching(&watcher, |report| report.result.is_ok());

    fs::write(
        dir.path().join("extra.kdl"),
        "binds { Mod+T { spawn \"kitty\"; }; }",
    )
    .unwrap();
    next_matching(&watcher, |report| {
        report.result.as_ref().is_ok_and(|s| s.bindings.len() == 1)
    });

    fs::write(&path, "binds {\n").unwrap();
    next_matching(&watcher, |report| report.result.is_err());

    let replacement = dir.path().join("replacement.kdl");
    fs::write(&replacement, "binds { Mod+Q { close-window; }; }").unwrap();
    fs::rename(replacement, &path).unwrap();
    let report = next_matching(&watcher, |report| {
        report.result.as_ref().is_ok_and(|s| {
            s.bindings
                .first()
                .is_some_and(|binding| binding.keys == "Mod+Q")
        })
    });
    assert_eq!(report.files.len(), 1);
}

#[test]
fn recovers_from_missing_initial_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("config.kdl");
    let watcher = Watcher::start(path.clone());
    next_matching(&watcher, |report| report.result.is_err());
    fs::write(path, "binds { Mod+Q { close-window; }; }").unwrap();
    next_matching(&watcher, |report| report.result.is_ok());
}
