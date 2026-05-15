use owo_colors::OwoColorize;

pub fn badge_ok() -> String {
    "OK   ".bold().green().to_string()
}

pub fn badge_err() -> String {
    "ERR  ".bold().red().to_string()
}

pub fn badge_warn() -> String {
    "WARN ".bold().yellow().to_string()
}

pub fn badge_info() -> String {
    "INFO ".bold().blue().to_string()
}

pub fn badge_dl() -> String {
    "DL   ".bold().bright_blue().to_string()
}

pub fn badge_run() -> String {
    "RUN  ".bold().bright_magenta().to_string()
}

pub fn badge_build() -> String {
    "BUILD".bold().bright_yellow().to_string()
}

#[macro_export]
macro_rules! log_ok {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_ok(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_err {
    ($($arg:tt)*) => {
        eprintln!("[{}] {}", $crate::log::badge_err(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_warn {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_warn(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_info {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_info(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_dl {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_dl(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_run {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_run(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_build {
    ($($arg:tt)*) => {
        println!("[{}] {}", $crate::log::badge_build(), format!($($arg)*));
    };
}

#[macro_export]
macro_rules! log_step {
    ($cur:expr, $total:expr, $($arg:tt)*) => {{
        use owo_colors::OwoColorize;
        let counter = format!("{}/{}  ", $cur, $total)
            .dimmed().to_string();
        println!("[{}] {}", counter, format!($($arg)*));
    }};
}

#[macro_export]
macro_rules! log_section {
    ($($arg:tt)*) => {{
        use owo_colors::OwoColorize;
        let title = format!("── {} ", format!($($arg)*));
        let rule  = "─".repeat(60usize.saturating_sub(title.len()));
        println!(
            "\n{}{}",
            title.bold(),
            rule.dimmed(),
        );
    }};
}
