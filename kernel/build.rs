fn main() {
    let manifest_dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let arch = std::env::var("CARGO_CFG_TARGET_ARCH").unwrap();

    let ld = manifest_dir.join(format!("src/arch/{arch}/linker.ld"));

    println!("cargo:rerun-if-changed={}", ld.display());
    println!(
        "cargo:rerun-if-changed={}/limine.conf",
        manifest_dir.display()
    );
    println!("cargo:rustc-link-arg=-T{}", ld.display());
    println!("cargo:rustc-link-arg=-zmax-page-size=0x1000");
}
