use std::{
    env,
    sync::OnceLock,
    time::{Duration, Instant},
};

static START: OnceLock<Instant> = OnceLock::new();
static ENABLED: OnceLock<bool> = OnceLock::new();

/// Record the process start. Call first thing in `main`.
pub fn start() {
    START.get_or_init(Instant::now);
}

pub fn elapsed() -> Duration {
    START.get_or_init(Instant::now).elapsed()
}

/// Print a startup milestone when `NIRI_KEYMAP_HELP_TIMING=1`.
pub fn mark(label: &str) {
    let enabled = *ENABLED
        .get_or_init(|| env::var_os("NIRI_KEYMAP_HELP_TIMING").is_some_and(|value| value == "1"));
    if enabled {
        eprintln!("[+{:>4}ms] {label}", elapsed().as_millis());
    }
}
