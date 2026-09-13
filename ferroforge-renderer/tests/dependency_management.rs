use std::{
    collections::BTreeSet,
    fs,
    path::PathBuf,
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::{
    dependencies::{
        DependencyContributor, DependencyRequirement, DependencySource, merge_dependencies,
    },
    source::discover_task_package,
};

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

fn requirement(
    version: &str,
    default_features: bool,
    features: &[&str],
    contributor: DependencyContributor,
) -> DependencyRequirement {
    DependencyRequirement {
        name: "example".to_owned(),
        package: "example".to_owned(),
        source: DependencySource::Registry(CRATES_IO_SOURCE.to_owned()),
        version_requirement: version.to_owned(),
        default_features,
        features: features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect(),
        contributor,
    }
}

#[test]
fn merges_matching_requirements_and_unions_features() {
    let first = PathBuf::from("tasks/Cargo.toml");
    let second = PathBuf::from("init/Cargo.toml");
    let merged = merge_dependencies([
        requirement(
            "1.2.3",
            false,
            &["alpha"],
            DependencyContributor::Manifest(first.clone()),
        ),
        requirement(
            "1.2.3",
            false,
            &["beta", "alpha"],
            DependencyContributor::Manifest(second.clone()),
        ),
    ])
    .unwrap();
    let dependency = &merged["example"];
    assert_eq!(
        dependency.features,
        BTreeSet::from(["alpha".to_owned(), "beta".to_owned()])
    );
    assert_eq!(
        dependency.contributors,
        BTreeSet::from([
            DependencyContributor::Manifest(first),
            DependencyContributor::Manifest(second),
        ])
    );
}

#[test]
fn rejects_conflicting_versions_defaults_and_sources_with_contributors() {
    for (change, property, incoming) in [
        ("version", "version requirement", "^1.2.3"),
        ("defaults", "default-features", "false"),
        (
            "source",
            "source",
            "registry source `registry+https://example.invalid`",
        ),
    ] {
        let mut second = requirement(
            "1.2.3",
            true,
            &[],
            DependencyContributor::System("RTIC backend".to_owned()),
        );
        match change {
            "version" => second.version_requirement = "^1.2.3".to_owned(),
            "defaults" => second.default_features = false,
            "source" => {
                second.source =
                    DependencySource::Registry("registry+https://example.invalid".to_owned())
            }
            _ => unreachable!(),
        }
        let error = merge_dependencies([
            requirement(
                "1.2.3",
                true,
                &[],
                DependencyContributor::Manifest(PathBuf::from("tasks/Cargo.toml")),
            ),
            second,
        ])
        .unwrap_err()
        .to_string();
        assert!(error.contains(property), "{change}: {error}");
        assert!(error.contains("tasks/Cargo.toml"), "{change}: {error}");
        assert!(error.contains("RTIC backend"), "{change}: {error}");
        assert!(error.contains(incoming), "{change}: {error}");
    }
}

struct TempPackage(PathBuf);

impl TempPackage {
    fn new(manifest: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::temp_dir().join(format!(
            "ferroforge-dependencies-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::write(root.join("Cargo.toml"), manifest).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        fs::write(root.join("src/lib.rs"), "#![no_std]\n").unwrap();
        Self(root)
    }

    fn manifest(&self) -> PathBuf {
        self.0.join("Cargo.toml")
    }
}

impl Drop for TempPackage {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed.
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn manifest(dependencies: &str, metadata: &str) -> String {
    format!(
        "[package]\nname = \"dependency-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{dependencies}\n{metadata}\n[workspace]\n"
    )
}

#[test]
fn preserves_authored_version_requirements_and_excludes_check_only_dependencies() {
    let package = TempPackage::new(&manifest(
        "serde = \"1\"\nferroforge = { path = \"local-check\" }",
        "[package.metadata.ferroforge]\ncheck-only-dependencies = [\"ferroforge\"]\n\n",
    ));
    fs::create_dir(package.0.join("local-check")).unwrap();
    fs::write(
        package.0.join("local-check/Cargo.toml"),
        "[package]\nname = \"ferroforge\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .unwrap();
    fs::create_dir(package.0.join("local-check/src")).unwrap();
    fs::write(package.0.join("local-check/src/lib.rs"), "").unwrap();

    let discovered = discover_task_package(&package.manifest()).unwrap();
    assert_eq!(discovered.dependencies.len(), 1);
    assert_eq!(discovered.dependencies[0].name, "serde");
    assert_eq!(discovered.dependencies[0].version_requirement, "1");
    assert_eq!(
        discovered.check_only_dependencies,
        BTreeSet::from(["ferroforge".to_owned()])
    );
}

#[test]
fn diagnoses_invalid_check_only_metadata() {
    for (metadata, expected) in [
        (
            "[package.metadata.ferroforge]\nunknown = []\n\n",
            "unknown package.metadata.ferroforge key",
        ),
        (
            "[package.metadata.ferroforge]\ncheck-only-dependencies = [\"absent\"]\n\n",
            "is not a normal dependency",
        ),
        (
            "[package.metadata.ferroforge]\ncheck-only-dependencies = [\"serde\", \"serde\"]\n\n",
            "duplicate check-only dependency",
        ),
    ] {
        let package = TempPackage::new(&manifest("serde = \"1\"", metadata));
        let error = discover_task_package(&package.manifest())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn diagnoses_manifest_forms_outside_the_initial_runtime_scope() {
    for (dependency, expected) in [
        (
            "serde = { version = \"1\", optional = true }",
            "optional runtime dependency",
        ),
        (
            "renamed = { package = \"serde\", version = \"1\" }",
            "renamed runtime dependency",
        ),
    ] {
        let package = TempPackage::new(&manifest(dependency, ""));
        let error = discover_task_package(&package.manifest())
            .unwrap_err()
            .to_string();
        assert!(error.contains(expected), "{error}");
    }
}

#[test]
fn diagnoses_target_specific_and_workspace_inherited_dependencies() {
    let target_specific = TempPackage::new(
        "[package]\nname = \"dependency-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[target.'cfg(windows)'.dependencies]\nserde = \"1\"\n\n[workspace]\n",
    );
    let error = discover_task_package(&target_specific.manifest())
        .unwrap_err()
        .to_string();
    assert!(error.contains("target-specific"), "{error}");

    let inherited = TempPackage::new(
        "[package]\nname = \"dependency-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nserde = { workspace = true }\n\n[workspace]\nmembers = [\".\"]\n\n[workspace.dependencies]\nserde = \"1\"\n",
    );
    let error = discover_task_package(&inherited.manifest())
        .unwrap_err()
        .to_string();
    assert!(error.contains("workspace-inherited"), "{error}");
}
