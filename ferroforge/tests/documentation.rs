//! Every module in FerroForge says what it is for.
//!
//! `#![deny(missing_docs)]` in each crate root holds public items to that, but
//! it never looks at a private module, and most of FerroForge is private
//! modules: the CLI is a binary, and the macros' workings are hidden behind
//! their three entry points. So this reads every source file of the four crates
//! and asks for module documentation - `//!`, or a crate root's
//! `#![doc = include_str!(..)]` - before the first line of code.
//!
//! That the documentation renders is `cargo doc`'s to show, with warnings
//! denied; see the book's workflow chapter.

use std::{fs, path::Path};

/// The crates FerroForge publishes. The example crates are examples, and are
/// not held to this.
const CRATES: &[&str] = &[
    "ferroforge",
    "ferroforge-macros",
    "ferroforge-contracts",
    "ferroforge-cli",
];

fn rust_files(directory: &Path, found: &mut Vec<std::path::PathBuf>) {
    for entry in fs::read_dir(directory).unwrap().flatten() {
        let path = entry.path();
        if path.is_dir() {
            rust_files(&path, found);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            found.push(path);
        }
    }
}

/// Module documentation somewhere before the first line that is neither
/// blank, a comment nor an inner attribute.
fn documents_itself(text: &str) -> bool {
    for line in text.lines().map(str::trim) {
        if line.starts_with("//!") || line.starts_with("#![doc") {
            return true;
        }
        if !(line.is_empty() || line.starts_with("//") || line.starts_with("#![")) {
            return false;
        }
    }
    false
}

#[test]
fn every_module_is_documented() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap();
    let mut files = Vec::new();
    for name in CRATES {
        rust_files(&root.join(name).join("src"), &mut files);
    }
    assert!(
        files.len() >= CRATES.len(),
        "found no sources under {root:?}"
    );

    let undocumented = files
        .iter()
        .filter(|path| !documents_itself(&fs::read_to_string(path).unwrap()))
        .map(|path| path.strip_prefix(root).unwrap().display().to_string())
        .collect::<Vec<_>>();
    assert!(
        undocumented.is_empty(),
        "these modules do not say what they are for; open each with `//!` \
         documentation:\n  {}",
        undocumented.join("\n  ")
    );
}

#[test]
fn the_check_tells_documentation_from_code() {
    assert!(documents_itself("//! What this is.\nuse std::fs;\n"));
    assert!(documents_itself(
        "#![no_std]\n#![doc = include_str!(\"x\")]\n"
    ));
    assert!(documents_itself("// A note first.\n\n//! Then the docs.\n"));
    assert!(!documents_itself("use std::fs;\n//! Too late.\n"));
    assert!(!documents_itself(
        "/// An item's docs, not the module's.\nfn f() {}\n"
    ));
    assert!(!documents_itself(""));
}
