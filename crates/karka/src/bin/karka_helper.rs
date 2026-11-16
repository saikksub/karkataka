//! macOS only: CEF's GPU, renderer and utility processes run from this binary inside karka.app.

fn main() {
    #[cfg(target_os = "macos")]
    {
        let exe = std::env::current_exe().expect("cannot locate helper executable");
        let loader = cef::library_loader::LibraryLoader::new(&exe, true);
        assert!(loader.load(), "cannot load the CEF framework");
        let code = karka::engine::run_subprocess().unwrap_or(0);
        drop(loader);
        std::process::exit(code);
    }
}
