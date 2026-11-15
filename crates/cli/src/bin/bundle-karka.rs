//! macOS only: build target/bundle/karka.app, which holds karka, its CEF helpers and the CEF framework.

fn main() -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(target_os = "macos")]
    {
        use cef::build_util::mac::{BundleInfo, build_bundle};
        let version = env!("CARGO_PKG_VERSION").parse()?;
        let info = BundleInfo::new("karka", "com.xploray.karka", "karka", "English", version);
        let app = build_bundle(std::path::Path::new("target/bundle"), "karka", info)?;
        println!("Built {}", app.display());
    }
    #[cfg(not(target_os = "macos"))]
    println!("No bundle needed on this platform; run `cargo run -p karka` directly.");
    Ok(())
}
