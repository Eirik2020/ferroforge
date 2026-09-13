use std::{
    fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::source::{DefinitionId, ModuleId, discover_task_package, discover_tasks};
use quote::ToTokens;

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sw/src/lib.rs")
}

fn fixture_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sw/Cargo.toml")
}

fn id(root: &ModuleId, module: &[&str], name: &str) -> DefinitionId {
    DefinitionId {
        module: ModuleId {
            crate_root: root.crate_root.clone(),
            path: module.iter().map(|s| (*s).to_owned()).collect(),
        },
        name: name.to_owned(),
    }
}

#[test]
fn discovers_root_inline_file_and_mod_rs_sources_without_compiling_them() {
    let sources = discover_tasks(&fixture()).unwrap();
    assert_eq!(sources.modules.len(), 7);
    assert_eq!(sources.files.len(), 6);
    let selected = sources
        .select(&[
            ("root", id(&sources.root, &[], "root_task")),
            ("status", id(&sources.root, &["indicators"], "blink")),
            ("inline_report", id(&sources.root, &["inline"], "report")),
        ])
        .unwrap();
    assert_eq!(selected[1].definition.contract.arguments.local.len(), 2);
    assert_eq!(
        selected[1].definition.contract.arguments.spawn[0].name,
        "report"
    );
    assert_eq!(
        selected[1].definition.contract.arguments.config[0].name,
        "period_ms"
    );
    assert!(
        selected[1]
            .definition
            .function
            .to_token_stream()
            .to_string()
            .contains("CONFIG . PERIOD_MS")
    );
    assert!(selected[2].support_module.support.iter().any(|i| {
        i.to_token_stream()
            .to_string()
            .contains("must remain literal text")
    }));
}

#[test]
fn resolves_the_task_package_and_library_root_through_cargo() {
    let package = discover_task_package(&fixture_manifest()).unwrap();
    assert_eq!(package.name, "ferroforge-sw-task-fixture");
    assert_eq!(package.library_target, "ferroforge_sw_task_fixture");
    assert_eq!(package.manifest, fixture_manifest().canonicalize().unwrap());
    assert_eq!(package.library_root, fixture().canonicalize().unwrap());
    assert_eq!(
        package.root,
        fixture_manifest().parent().unwrap().canonicalize().unwrap()
    );
    assert!(package.package_id.contains("ferroforge-sw-task-fixture"));
    assert_eq!(
        package
            .dependencies
            .iter()
            .map(|dependency| dependency.name.as_str())
            .collect::<Vec<_>>(),
        ["defmt", "embedded-hal", "fugit", "heapless", "rtt-target"]
    );
    assert_eq!(
        package.check_only_dependencies.iter().collect::<Vec<_>>(),
        ["ferroforge"]
    );
    assert_eq!(package.sources.modules.len(), 7);
}

#[test]
fn selects_tasks_individually_and_shares_module_support_between_instances() {
    let sources = discover_tasks(&fixture()).unwrap();
    let blink = id(&sources.root, &["indicators"], "blink");
    let selected = sources
        .select(&[("status", blink.clone()), ("alarm", blink)])
        .unwrap();
    assert_eq!(selected.len(), 2);
    assert!(selected.iter().all(|s| s.definition.id.name == "blink"));
    assert!(std::ptr::eq(selected[0].definition, selected[1].definition));
    assert!(std::ptr::eq(
        selected[0].support_module,
        selected[1].support_module
    ));
    let support = &selected[0].support_module.support;
    assert_eq!(
        support
            .iter()
            .filter(|i| matches!(i, syn::Item::Use(_)))
            .count(),
        3
    );
    assert!(support.iter().any(|i| matches!(i, syn::Item::Struct(_))));
    assert!(support.iter().any(|i| matches!(i, syn::Item::Impl(_))));
    assert!(support.iter().any(|i| matches!(i, syn::Item::Fn(_))));
    assert!(
        !support
            .iter()
            .any(|i| matches!(i, syn::Item::Fn(f) if f.sig.ident == "report"))
    );
}

#[test]
fn selection_rejects_unknown_definitions_and_duplicate_or_invalid_instance_names() {
    let sources = discover_tasks(&fixture()).unwrap();
    let report = id(&sources.root, &["inline"], "report");
    assert!(
        sources
            .select(&[("repeat", report.clone()), ("repeat", report.clone())])
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    assert!(
        sources
            .select(&[("repeat", report.clone()), ("r#repeat", report.clone())])
            .unwrap_err()
            .to_string()
            .contains("duplicate")
    );
    assert!(sources.select(&[("not a name", report)]).is_err());
    assert!(
        sources
            .select(&[("missing", id(&sources.root, &["inline"], "absent"))])
            .unwrap_err()
            .to_string()
            .contains("unknown task")
    );
    assert!(
        sources
            .select(&[("missing", id(&sources.root, &["absent"], "report"))])
            .unwrap_err()
            .to_string()
            .contains("unknown source module")
    );
}

// Temporary source trees are isolated test inputs, not source-crate manifests.
// Their contents deliberately include invalid Rust contracts for diagnostics.
struct TempSource(PathBuf);
impl TempSource {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "ferroforge-discovery-{}-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn write(&self, path: &str, text: &str) {
        let path = self.0.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }
    fn root(&self) -> PathBuf {
        self.0.join("lib.rs")
    }
    fn manifest(&self) -> PathBuf {
        self.0.join("Cargo.toml")
    }
}
impl Drop for TempSource {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed, never a source root.
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn rejects_conditional_or_custom_source_without_following_it() {
    for (source, message) in [
        ("#[cfg(any())] mod nonexistent;", "conditional source"),
        (
            "#[path = \"outside.rs\"] mod nonexistent;",
            "custom module paths",
        ),
        ("include!(\"nonexistent.rs\");", "item macro expansion"),
        (
            "use ferroforge::task as job; #[job] async fn run(cx: run::Context) {}",
            "aliases",
        ),
        (
            "use ferroforge::{self as ff}; #[ff::task] async fn run(cx: run::Context) {}",
            "aliases",
        ),
        (
            "#[other::task] async fn run(cx: run::Context) {}",
            "unsupported attribute",
        ),
    ] {
        let temp = TempSource::new();
        temp.write("lib.rs", source);
        let error = discover_tasks(&temp.root()).unwrap_err().to_string();
        assert!(error.contains(message), "{error}");
        assert!(error.contains("lib.rs"));
    }
}

#[test]
fn reports_missing_ambiguous_and_duplicate_modules() {
    let temp = TempSource::new();
    temp.write("lib.rs", "mod child;");
    assert!(
        discover_tasks(&temp.root())
            .unwrap_err()
            .to_string()
            .contains("missing source")
    );
    temp.write("child.rs", "");
    temp.write("child/mod.rs", "");
    assert!(
        discover_tasks(&temp.root())
            .unwrap_err()
            .to_string()
            .contains("ambiguous")
    );
    temp.write("lib.rs", "mod child {} mod child {}");
    assert!(
        discover_tasks(&temp.root())
            .unwrap_err()
            .to_string()
            .contains("duplicate module")
    );
}

#[test]
fn file_modules_nested_under_inline_modules_use_the_correct_directory() {
    let temp = TempSource::new();
    temp.write("lib.rs", "mod outer { mod inner; }");
    temp.write(
        "outer/inner.rs",
        "#[ferroforge::task] async fn run(cx: run::Context) {}",
    );
    let sources = discover_tasks(&temp.root()).unwrap();
    assert_eq!(
        sources
            .select(&[("run", id(&sources.root, &["outer", "inner"], "run"))])
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn rejects_duplicate_task_attributes_names_and_context_module_collisions() {
    for (source, expected) in [
        (
            "#[task] #[ferroforge::task] async fn run(cx: run::Context) {}",
            "exactly one",
        ),
        (
            "#[task] async fn run(cx: run::Context) {} #[task] async fn run(cx: run::Context) {}",
            "duplicate task",
        ),
        (
            "mod run {} #[task] async fn run(cx: run::Context) {}",
            "conflicts with module",
        ),
        (
            "#[task(local = [led])] async fn run(cx: run::Context) {}",
            "inline type",
        ),
    ] {
        let temp = TempSource::new();
        temp.write("lib.rs", source);
        assert!(
            discover_tasks(&temp.root())
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn package_discovery_requires_a_package_with_a_library_target() {
    let virtual_manifest = TempSource::new();
    virtual_manifest.write("Cargo.toml", "[workspace]\nmembers = []\n");
    let error = discover_task_package(&virtual_manifest.manifest())
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("does not describe a Cargo package"),
        "{error}"
    );

    let binary = TempSource::new();
    binary.write(
        "Cargo.toml",
        "[package]\nname = \"binary-only\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[workspace]\n",
    );
    binary.write("src/main.rs", "fn main() {}\n");
    let error = discover_task_package(&binary.manifest())
        .unwrap_err()
        .to_string();
    assert!(error.contains("needs a library target"), "{error}");
}

#[test]
fn package_discovery_uses_the_declared_library_path() {
    let package = TempSource::new();
    package.write(
        "Cargo.toml",
        "[package]\nname = \"custom-lib\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"source/tasks.rs\"\n\n[workspace]\n",
    );
    package.write(
        "source/tasks.rs",
        "#[ferroforge::task] async fn run(cx: run::Context) {}\n",
    );

    let discovered = discover_task_package(&package.manifest()).unwrap();
    assert_eq!(
        discovered.library_root,
        package.0.join("source/tasks.rs").canonicalize().unwrap()
    );
    assert_eq!(discovered.sources.modules.len(), 1);
}
