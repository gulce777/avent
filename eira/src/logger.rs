//! System-wide logging framework.
//!
//! Routes all `log` crate messages to the serial port.
//! Call [`init`] once during early boot before any `log::` macro is used.

use log::{Level, LevelFilter, Metadata, Record};

struct EiraLogger;

impl log::Log for EiraLogger {
    fn enabled(&self, metadata: &Metadata) -> bool {
        metadata.level() <= Level::Trace
    }

    fn log(&self, record: &Record) {
        if !self.enabled(record.metadata()) {
            return;
        }

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
            "{dim}[{color}{:<5}{dim}] {target} | {reset}{args}",
            record.level(),
            dim = dim_color,
            color = color_code,
            reset = reset_color,
            target = record.target(),
            args = record.args(),
        );
    }

    fn flush(&self) {}
}

static LOGGER: EiraLogger = EiraLogger;

/// Install the kernel logger.
///
/// Must be called exactly once, early in the boot. All `log::` macros silently
/// drop their output until this returns.
///
/// # Panics
///
/// Panics if called more than once.
pub fn init() {
    log::set_logger(&LOGGER).expect("logger already initialised");

    log::set_max_level(LevelFilter::Trace);
}
