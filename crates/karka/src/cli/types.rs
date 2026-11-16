//! Types for code editors: a tsconfig.json and the declarations of solid-js and the karka runtime
//! in `.karka/types/`, so scenes get completion and checks in any editor that speaks TypeScript.
//! No Node or npm install needed.

use std::path::Path;

/// Built by `pnpm assets`: declaration file path → text.
const TYPES: &str = include_str!("../../assets/types.json");

const TSCONFIG: &str = r#"{
  "compilerOptions": {
    "target": "ES2022",
    "module": "ESNext",
    "moduleResolution": "Bundler",
    "lib": ["ES2022", "DOM", "DOM.Iterable"],
    "jsx": "preserve",
    "jsxImportSource": "solid-js",
    "strict": true,
    "noEmit": true,
    "skipLibCheck": true,
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "paths": {
      "solid-js": ["./.karka/types/solid-js/types/index.d.ts"],
      "solid-js/web": ["./.karka/types/solid-js/web/types/index.d.ts"],
      "solid-js/store": ["./.karka/types/solid-js/store/types/index.d.ts"],
      "solid-js/jsx-runtime": ["./.karka/types/solid-js/types/jsx.d.ts"],
      "@karkataka/runtime": ["./.karka/types/runtime/index.d.ts"],
      "csstype": ["./.karka/types/csstype/index.d.ts"]
    }
  },
  "include": ["**/*.ts", "**/*.tsx"],
  "exclude": ["out", ".karka", "media"]
}
"#;

/// Write the declarations (always, to match this karka) and a tsconfig.json (only if there is
/// none: it is the user's to change).
pub fn write(dir: &Path) -> Result<(), String> {
    let types: serde_json::Map<String, serde_json::Value> = serde_json::from_str(TYPES).expect("assets/types.json is a JSON object");
    let root = dir.join(".karka/types");
    for (path, text) in &types {
        let file = root.join(path);
        crate::files::create_dir(file.parent().expect("type files are in folders"))?;
        crate::files::write(&file, text.as_str().unwrap_or("").as_bytes())?;
    }
    let tsconfig = dir.join("tsconfig.json");
    if !tsconfig.exists() {
        crate::files::write(&tsconfig, TSCONFIG.as_bytes())?;
    }
    Ok(())
}

/// Write the declarations unless they are there (a project cloned without its .karka folder).
pub fn ensure(dir: &Path) -> Result<(), String> {
    if dir.join(".karka/types/runtime/index.d.ts").exists() { Ok(()) } else { write(dir) }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_declarations_and_keeps_the_users_tsconfig() {
        let dir = std::env::temp_dir().join(format!("karka-types-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        ensure(&dir).unwrap();
        assert!(dir.join(".karka/types/solid-js/web/types/index.d.ts").exists());
        assert!(std::fs::read_to_string(dir.join(".karka/types/runtime/index.d.ts")).unwrap().contains("export declare function useFrame"));
        assert!(std::fs::read_to_string(dir.join("tsconfig.json")).unwrap().contains("@karkataka/runtime"));
        std::fs::write(dir.join("tsconfig.json"), "{}").unwrap();
        std::fs::remove_file(dir.join(".karka/types/runtime/index.d.ts")).unwrap();
        ensure(&dir).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("tsconfig.json")).unwrap(), "{}");
        assert!(dir.join(".karka/types/runtime/index.d.ts").exists());
        // Already there: nothing to do (a read-only folder would make writing fail).
        ensure(&dir).unwrap();
        let blocked = dir.join("file");
        std::fs::write(&blocked, "").unwrap();
        assert!(write(&blocked).unwrap_err().starts_with("cannot create"));
        std::fs::remove_dir_all(dir).unwrap();
    }
}
