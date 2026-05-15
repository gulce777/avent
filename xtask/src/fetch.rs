use crate::{build_dir, log_dl, log_info, log_ok, log_step};
use anyhow::{Context, Result, bail};
use std::process::Command;

const LIMINE_REPO: &str = "https://github.com/limine-bootloader/limine.git";
const LIMINE_BRANCH: &str = "v11.x-binary";

pub fn fetch_limine() -> Result<()> {
    let dest = build_dir().join("limine");

    log_step!(1, 2, "Fetching Limine...");

    if dest.join(".git").exists() {
        log_info!("Limine already present, pulling latest.");

        let status = Command::new("git")
            .args(["-C", dest.to_str().unwrap(), "pull", "--ff-only"])
            .status()
            .context("git pull failed, is git installed?")?;

        if !status.success() {
            bail!("git pull failed");
        }

        log_ok!("Limine up to date at:   {}", dest.display());
    } else {
        log_dl!("Cloning Limine ({LIMINE_BRANCH} branch)...");
        std::fs::create_dir_all(&dest)?;

        let status = Command::new("git")
            .args([
                "clone",
                "--branch",
                LIMINE_BRANCH,
                "--depth",
                "1",
                LIMINE_REPO,
                dest.to_str().unwrap(),
            ])
            .status()
            .context("git clone failed, is git installed?")?;

        if !status.success() {
            bail!("Limine clone failed");
        }

        log_ok!("Limine ready at:   {}", dest.display());
    }

    Ok(())
}

const EDK2_OVMF_REPO: &str = "https://github.com/gulce777/edk2-ovmf.git";

pub fn fetch_ovmf() -> Result<()> {
    let dest = build_dir().join("edk2-ovmf");

    log_step!(2, 2, "Fetching OVMF firmware...");

    if dest.join(".git").exists() {
        log_info!("edk2-ovmf already present, pulling latest.");

        let status = Command::new("git")
            .args(["-C", dest.to_str().unwrap(), "pull"])
            .status()
            .context("git pull failed, is git installed?")?;

        if !status.success() {
            bail!("git pull failed");
        }

        log_ok!("edk2-ovmf up to date at {}", dest.display());
    } else {
        log_dl!("Cloning edk2-ovmf...");
        std::fs::create_dir_all(&dest)?;

        let status = Command::new("git")
            .args([
                "clone",
                "--depth",
                "1",
                EDK2_OVMF_REPO,
                dest.to_str().unwrap(),
            ])
            .status()
            .context("git clone failed, is git installed?")?;

        if !status.success() {
            bail!("edk2-ovmf clone failed");
        }

        log_ok!("OVMF firmware ready at {}", dest.display());
    }

    Ok(())
}
