//! Architectural guard: the `core` engine must stay domain-agnostic.
//!
//! `src/core/**` is the future `scrip-core` crate. If any file in it references the
//! finance domain, the seam is broken. This test fails loudly when that happens.

use std::fs;
use std::path::{Path, PathBuf};

fn rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        if path.is_dir() {
            rust_files(&path, out);
        } else if path.extension().is_some_and(|e| e == "rs") {
            out.push(path);
        }
    }
}

#[test]
fn core_does_not_depend_on_the_domain() {
    let core = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/core");
    let mut files = Vec::new();
    rust_files(&core, &mut files);
    assert!(!files.is_empty(), "expected to find modules under src/core");

    let banned = [
        "crate::config",
        "crate::models",
        "crate::generators",
        "crate::fixtures",
        "crate::reference",
    ];

    for file in files {
        let src = fs::read_to_string(&file).unwrap();
        for needle in banned {
            assert!(
                !src.contains(needle),
                "{} references the domain via `{needle}` — core must stay domain-agnostic",
                file.display()
            );
        }
    }
}
