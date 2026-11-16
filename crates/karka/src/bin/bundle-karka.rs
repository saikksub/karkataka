//! macOS only: build karka.app (karka, its CEF helpers and the CEF framework).
//!
//!   cargo run -p karka --bin bundle-karka [folder] [--release]     # default folder: target/bundle
//!
//! Debug builds unless --release. scripts/install.sh uses --release.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        use cef::build_util::mac::{BundleInfo, build_bundle, bundle};
        use std::path::Path;
        let args: Vec<String> = std::env::args().skip(1).collect();
        let release = args.iter().any(|a| a == "--release");
        let folder = args.iter().find(|a| *a != "--release").map_or("target/bundle", String::as_str);
        let folder = Path::new(folder);
        let version = env!("CARGO_PKG_VERSION").parse().expect("the crate version is valid semver");
        let info = BundleInfo::new("karka", "com.xploray.karka", "karka", "English", version);
        // macOS kills a binary at launch when it was copied over one that already ran, so start
        // from no app at all.
        let _ = std::fs::remove_dir_all(folder.join("karka.app"));
        let app = if release {
            let cargo = std::env::var("CARGO").unwrap_or(String::from("cargo"));
            println!("Building karka and karka_helper (release)...");
            let built = std::process::Command::new(cargo).args(["build", "--release", "-p", "karka", "--bin", "karka", "--bin", "karka_helper"]).status()?;
            if !built.success() {
                return Err("cargo build --release failed".into());
            }
            // This binary is <target>/debug/bundle-karka, so the release binaries are in <target>/release.
            let target = std::env::current_exe()?.parent().and_then(Path::parent).ok_or("cannot find the target folder")?.join("release");
            bundle(folder, &target, "karka", "karka_helper", None, info)?
        } else {
            build_bundle(folder, "karka", info)?
        };
        println!("Built {}", app.display());
    }
    #[cfg(not(target_os = "macos"))]
    println!("No bundle needed on this platform; run `cargo run -p karka` directly.");
    Ok(())
}
