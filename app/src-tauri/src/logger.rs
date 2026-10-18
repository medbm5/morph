//! Minimal logger: stderr plus `<data dir>/morph.log` (truncated at start).

use log::{Level, LevelFilter, Metadata, Record};
use std::io::Write;
use std::sync::Mutex;
use std::time::Instant;

struct Logger {
    file: Mutex<Option<std::fs::File>>,
    start: Instant,
    level: Level,
}

impl log::Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        m.level() <= self.level
            && (m.target().starts_with("metamorphosis") || m.target().starts_with("morph"))
    }

    fn log(&self, r: &Record) {
        if !self.enabled(r.metadata()) {
            return;
        }
        let line = format!(
            "{:>9.3} {:<5} {}: {}\n",
            self.start.elapsed().as_secs_f64(),
            r.level(),
            r.target(),
            r.args()
        );
        eprint!("{line}");
        if let Some(f) = self.file.lock().unwrap().as_mut() {
            let _ = f.write_all(line.as_bytes());
        }
    }

    fn flush(&self) {}
}

pub fn init() {
    let dir = morph_core::components::data_dir();
    let _ = std::fs::create_dir_all(&dir);
    let file = std::fs::File::create(dir.join("morph.log")).ok();
    let level = match std::env::var("MORPH_LOG").as_deref() {
        Ok("debug") => Level::Debug,
        Ok("trace") => Level::Trace,
        _ => Level::Info,
    };
    let logger = Logger {
        file: Mutex::new(file),
        start: Instant::now(),
        level,
    };
    if log::set_boxed_logger(Box::new(logger)).is_ok() {
        log::set_max_level(LevelFilter::Trace);
    }
}
