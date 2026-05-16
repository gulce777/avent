mod build;
mod fetch;
mod iso;
pub mod log;
mod run;

use anyhow::Result;
use clap::{Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "xtask",
    about = "Eira build system",
    version,
    propagate_version = true
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Download Limine and edk2-OVMF (skips if already present)
    Fetch,

    /// Compile the kernel
    Build {
        #[arg(long, default_value = "x86_64")]
        arch: Arch,
        #[arg(long)]
        release: bool,
    },

    /// Build a bootable ISO image
    Iso {
        #[arg(long, default_value = "x86_64")]
        arch: Arch,
        #[arg(long)]
        release: bool,
    },

    /// Run the kernel in QEMU
    Run {
        #[arg(long, default_value = "x86_64")]
        arch: Arch,
        #[arg(long)]
        release: bool,
        /// Boot in BIOS mode (x86_64 only; default: UEFI)
        #[arg(long)]
        bios: bool,
    },

    /// Run kernel tests in QEMU and report pass/fail
    Test {
        #[arg(long, default_value = "x86_64")]
        arch: Arch,
        #[arg(long)]
        release: bool,
        /// Boot in BIOS mode (x86_64 only; default: UEFI)
        #[arg(long)]
        bios: bool,
    },

    /// Remove the build/ directory
    Clean,
}

#[derive(Clone, ValueEnum, Debug)]
pub enum Arch {
    #[value(name = "x86_64")]
    X86_64,
    Aarch64,
}

impl Arch {
    pub fn as_str(&self) -> &'static str {
        match self {
            Arch::X86_64 => "x86_64",
            Arch::Aarch64 => "aarch64",
        }
    }

    pub fn target_json(&self) -> String {
        format!("{}-eira.json", self.as_str())
    }

    pub fn iso(&self) -> String {
        format!("eira-{}.iso", self.as_str())
    }

    pub fn linker_script(&self) -> String {
        format!("eira/src/arch/{}/linker.ld", self.as_str())
    }

    pub fn ovmf_code(&self) -> String {
        format!("ovmf-code-{}.fd", self.as_str())
    }

    pub fn ovmf_vars(&self) -> String {
        format!("ovmf-vars-{}.fd", self.as_str())
    }

    pub fn qemu_bin(&self) -> &'static str {
        match self {
            Arch::X86_64 => "qemu-system-x86_64",
            Arch::Aarch64 => "qemu-system-aarch64",
        }
    }
}

pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("xtask/ has no parent directory")
        .to_path_buf()
}

pub fn build_dir() -> PathBuf {
    workspace_root().join("build")
}

fn run_cli() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fetch => {
            log_section!("FETCH");
            fetch::fetch_limine().and_then(|_| fetch::fetch_ovmf())
        }
        Command::Build { arch, release } => {
            log_section!("BUILD");
            build::build_kernel(&arch, release)
        }
        Command::Iso { arch, release } => {
            log_section!("BUILD");
            build::build_kernel(&arch, release)?;
            log_section!("ISO");
            iso::create_iso(&arch, release)
        }
        Command::Run {
            arch,
            release,
            bios,
        } => {
            log_section!("BUILD");
            build::build_kernel(&arch, release)?;
            log_section!("ISO");
            iso::create_iso(&arch, release)?;
            log_section!("RUN");
            run::run_qemu(&arch, !bios)
        }
        Command::Test {
            arch,
            release,
            bios,
        } => {
            log_section!("BUILD");
            build::build_kernel_with_tests(&arch, release)?;
            log_section!("ISO");
            iso::create_iso(&arch, release)?;
            log_section!("TEST");
            run::run_qemu_tests(&arch, !bios)
        }
        Command::Clean => {
            log_section!("CLEAN");
            let dir = build_dir();
            if dir.exists() {
                std::fs::remove_dir_all(&dir)?;
                log_ok!("build/ removed");
            } else {
                log_info!("build/ does not exist, nothing to clean");
            }
            Ok(())
        }
    }
}

fn main() {
    match run_cli() {
        Ok(_) => {
            println!();
        }
        Err(e) => {
            log_err!("{e:#}");
            std::process::exit(1);
        }
    };
}
