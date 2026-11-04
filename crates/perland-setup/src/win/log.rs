//! A plain log at `%TEMP%\perland-setup.log`, overwritten each run. No tokens or personal data
//! exist in the setup to leak.

use std::io::Write;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::Instant;

struct Log {
    file: Option<std::fs::File>,
    start: Instant,
}

fn log() -> &'static Mutex<Log> {
    static LOG: OnceLock<Mutex<Log>> = OnceLock::new();
    LOG.get_or_init(|| {
        Mutex::new(Log {
            file: std::fs::File::create(path()).ok(),
            start: Instant::now(),
        })
    })
}

pub fn path() -> PathBuf {
    std::env::temp_dir().join("perland-setup.log")
}

pub fn line(message: &str) {
    if let Ok(mut log) = log().lock() {
        let elapsed = log.start.elapsed().as_secs_f32();
        if let Some(file) = log.file.as_mut() {
            let _ = writeln!(file, "[{elapsed:8.3}] {message}");
        }
    }
}
