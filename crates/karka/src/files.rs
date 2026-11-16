//! File access whose errors say which file and what failed, in words an agent can act on.

use std::path::Path;

pub fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

pub fn write(path: &Path, contents: &[u8]) -> Result<(), String> {
    std::fs::write(path, contents).map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// A folder and its parents.
pub fn create_dir(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("cannot create {}: {e}", path.display()))
}

pub fn rename(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::rename(from, to).map_err(|e| format!("cannot replace {}: {e}", to.display()))
}

pub fn read_dir(path: &Path) -> Result<std::fs::ReadDir, String> {
    std::fs::read_dir(path).map_err(|e| format!("cannot read {}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_name_the_file() {
        let dir = std::env::temp_dir().join(format!("karka-files-{}", std::process::id()));
        create_dir(&dir).unwrap();
        let file = dir.join("a.txt");
        write(&file, b"hi").unwrap();
        assert_eq!(read(&file).unwrap(), "hi");
        rename(&file, &dir.join("b.txt")).unwrap();
        assert_eq!(read_dir(&dir).unwrap().count(), 1);
        let missing = dir.join("missing/x");
        assert!(read(&missing).unwrap_err().starts_with(&format!("cannot read {}", missing.display())));
        assert!(write(&missing, b"").unwrap_err().starts_with("cannot write"));
        assert!(create_dir(&dir.join("b.txt/sub")).unwrap_err().starts_with("cannot create"));
        assert!(rename(&missing, &file).unwrap_err().starts_with("cannot replace"));
        assert!(read_dir(&missing).unwrap_err().starts_with("cannot read"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
