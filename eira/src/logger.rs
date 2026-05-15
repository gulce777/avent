//! System-wide logging framework.
//!
//! Hooks into the `log` crate and routes all log messages through
//! serial port driver with ANSI color formatting.

use log::{Level, LevelFilter, Metadata, Record};

struct EiraLogger;

impl log::Log for EiraLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if self.enabled(record.metadata()) {
            let color_code = match record.level() {
                Level::Error => "\x1B[1;31m",
                Level::Warn => "\x1B[1;33m",
                Level::Info => "\x1B[1;32m",
                Level::Debug => "\x1B[1;34m",
                Level::Trace => "\x1B[1;90m",
            };

            let dim_color = "\x1B[0;90m";
            let reset_color = "\x1B[0m";

            crate::println!(
                "{}[{}{:<5}{}] {} | {}{}",
                dim_color,
                color_code,
                record.level(),
                dim_color,
                record.target(),
                reset_color,
                record.args()
            );
        }
    }

    fn flush(&self) {}
}

static LOGGER: EiraLogger = EiraLogger;

pub fn init() {
    unsafe {
        let _ = log::set_logger(&LOGGER);

        log::set_max_level(LevelFilter::Trace);
    }
}
