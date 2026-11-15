//! macOS only: build karka.app (karka, its CEF helpers and the CEF framework, as debug builds).
//!
//!   cargo run -p karka --bin bundle-karka [folder]     # default folder: target/bundle

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        use cef::build_util::mac::{BundleInfo, build_bundle};
        let folder = std::env::args().nth(1).unwrap_or(String::from("target/bundle"));
        let version = env!("CARGO_PKG_VERSION").parse().expect("the crate version is valid semver");
        let info = BundleInfo::new("karka", "com.xploray.karka", "karka", "English", version);
        let app = build_bundle(std::path::Path::new(&folder), "karka", info)?;
        println!("Built {}", app.display());
    }
    #[cfg(not(target_os = "macos"))]
    println!("No bundle needed on this platform; run `cargo run -p karka` directly.");
    Ok(())
}
