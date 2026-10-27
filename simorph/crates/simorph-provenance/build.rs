//! Records the compiler that built this crate, so provenance does not depend
//! on whichever `rustc` happens to be on the PATH at run time.
use std::process::Command;

fn main() {
    let rustc = std::env::var("RUSTC").unwrap_or_else(|_| "rustc".into());
    let v = Command::new(rustc)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default();
    println!("cargo:rustc-env=SIMORPH_BUILD_RUSTC={v}");
    println!(
        "cargo:rustc-env=SIMORPH_BUILD_PROFILE={}",
        std::env::var("PROFILE").unwrap_or_default()
    );
    println!("cargo:rerun-if-env-changed=RUSTC");
}
