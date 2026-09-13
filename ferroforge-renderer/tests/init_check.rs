use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::{
    composition::{MonotonicProfile, StandaloneComposition, TaskSelection, validate_composition},
    init_check::{InitCheckOptions, render_init_check, render_init_check_interface},
    source::{DefinitionId, ModuleId, TaskSources, discover_init_package, discover_task_package},
};

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn rust_analyzer() -> std::ffi::OsString {
    env::var_os("RUST_ANALYZER").unwrap_or_else(|| "rust-analyzer".into())
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}/Cargo.toml"))
}

fn id(sources: &TaskSources, module: &[&str], name: &str) -> DefinitionId {
    DefinitionId {
        module: ModuleId {
            crate_root: sources.root.crate_root.clone(),
            path: module.iter().map(|part| (*part).to_owned()).collect(),
        },
        name: name.to_owned(),
    }
}

fn task(instance: &str, definition: DefinitionId) -> TaskSelection {
    TaskSelection {
        instance: instance.to_owned(),
        definition,
        priority: 1,
        local: Vec::new(),
        shared: Vec::new(),
        configuration: Vec::new(),
        spawn: Vec::new(),
    }
}

fn composition(sources: &TaskSources, include_telemetry: bool) -> StandaloneComposition {
    let mut tasks = vec![task("status", id(sources, &[], "root_task"))];
    if include_telemetry {
        tasks.push(task("telemetry", id(sources, &["layout"], "consumer")));
    }
    StandaloneComposition {
        tasks,
        monotonic: Some(MonotonicProfile::initial_systick()),
    }
}

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "ferroforge-init-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        Self(root)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed.
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn generated_interfaces_check_the_complete_native_init_on_arm() {
    let init = discover_init_package(&fixture("init")).unwrap();
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let composition = composition(&tasks.sources, true);
    let validated = validate_composition(&tasks.sources, &composition).unwrap();
    let project = TempProject::new();
    let rendered = render_init_check(
        &init,
        &validated,
        InitCheckOptions {
            package_name: "ferroforge-init-check",
            output_dir: &project.0,
        },
    )
    .unwrap();
    let source = fs::read_to_string(&rendered.library_source).unwrap();
    let interfaces = fs::read_to_string(&rendered.interface_source).unwrap();
    let manifest = fs::read_to_string(&rendered.manifest).unwrap();

    assert!(interfaces.contains("pub device: ::stm32f4xx_hal::pac::Peripherals"));
    assert!(interfaces.contains("pub core: ::cortex_m::Peripherals"));
    assert!(interfaces.contains("pub fn start("));
    assert!(interfaces.contains("_syst: ::cortex_m::peripheral::SYST"));
    assert!(interfaces.contains("pub mod status"));
    assert!(interfaces.contains("pub mod telemetry"));
    assert!(interfaces.contains("pub fn spawn(value: u32) -> ::core::result::Result<(), u32>"));
    assert!(source.contains("cx.device.RCC.freeze"));
    assert!(source.contains("Mono::start(cx.core.SYST"));
    assert!(source.contains("ferroforge::init"));
    assert!(manifest.contains("cortex-m = \"0.7.7\""));
    assert!(manifest.contains("stm32f4xx-hal ="));
    assert!(manifest.contains("ferroforge ="));
    assert!(manifest.contains("check-only-dependencies = [\"ferroforge\"]"));

    let check = Command::new(cargo())
        .args(["check", "--lib", "--locked", "--offline"])
        .arg("--manifest-path")
        .arg(&rendered.manifest)
        .env("CARGO_TARGET_DIR", project.0.join("target"))
        .current_dir(&project.0)
        .status()
        .unwrap();
    assert!(check.success(), "generated native init checker must check");
}

#[test]
fn init_interface_regeneration_tracks_composition_and_profile() {
    let init = discover_init_package(&fixture("init")).unwrap();
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let full = validate_composition(&tasks.sources, &composition(&tasks.sources, true)).unwrap();
    let reduced =
        validate_composition(&tasks.sources, &composition(&tasks.sources, false)).unwrap();
    let full_source = render_init_check_interface(&init, &full).unwrap();
    let reduced_source = render_init_check_interface(&init, &reduced).unwrap();

    assert!(full_source.contains("pub mod telemetry"));
    assert!(!reduced_source.contains("pub mod telemetry"));
    assert_ne!(full_source, reduced_source);

    let mut missing_profile = full;
    missing_profile.monotonic = None;
    let error = render_init_check_interface(&init, &missing_profile)
        .unwrap_err()
        .to_string();
    assert!(
        error.contains("requires the initial SysTick profile"),
        "{error}"
    );
}

#[test]
fn regenerated_interfaces_reject_stale_or_mistyped_init_calls() {
    let init = discover_init_package(&fixture("init")).unwrap();
    let diagnostics = discover_init_package(&fixture("init-diagnostics")).unwrap();
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let full = validate_composition(&tasks.sources, &composition(&tasks.sources, true)).unwrap();
    let reduced =
        validate_composition(&tasks.sources, &composition(&tasks.sources, false)).unwrap();
    let project = TempProject::new();

    let rendered = render_init_check(
        &init,
        &reduced,
        InitCheckOptions {
            package_name: "ferroforge-init-check-negative",
            output_dir: &project.0,
        },
    )
    .unwrap();
    check_failure(
        &project.0,
        &rendered.manifest,
        "unresolved module or unlinked crate `telemetry`",
    );

    let rendered = render_init_check(
        &diagnostics,
        &full,
        InitCheckOptions {
            package_name: "ferroforge-init-check-negative",
            output_dir: &project.0,
        },
    )
    .unwrap();
    check_failure(
        &project.0,
        &rendered.manifest,
        "this function takes 0 arguments but 1 argument was supplied",
    );
    check_failure(
        &project.0,
        &rendered.manifest,
        "expected `u32`, found `u64`",
    );
}

#[test]
#[ignore = "requires the rust-analyzer CLI; see the mdBook workflow"]
fn rust_analyzer_reports_init_errors_on_authored_lines() {
    let init = discover_init_package(&fixture("init-diagnostics")).unwrap();
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let composition = composition(&tasks.sources, true);
    let validated = validate_composition(&tasks.sources, &composition).unwrap();
    let project = TempProject::new();
    let _fixture_output = FixtureOutput::new(&init.package.root);
    let rendered = render_init_check(
        &init,
        &validated,
        InitCheckOptions {
            package_name: "ferroforge-init-ra-check",
            output_dir: &init.package.root,
        },
    )
    .unwrap();
    let cargo_config = fs::read_to_string(&rendered.cargo_config).unwrap();
    assert!(
        !cargo_config.contains("//?/"),
        "generated Cargo config must use a Rust Analyzer-compatible Windows path"
    );
    let output = Command::new(rust_analyzer())
        .arg("diagnostics")
        .arg(&init.package.root)
        .args(["--severity", "error"])
        .env("CARGO_TARGET_DIR", project.0.join("target"))
        .env("RUST_BACKTRACE", "0")
        .current_dir(&init.package.root)
        .output()
        .expect("rust-analyzer must be installed to run this ignored test");
    assert!(
        !output.status.success(),
        "the intentionally invalid init produced no Rust Analyzer errors"
    );
    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        diagnostics.contains("init-diagnostics\\src\\lib.rs")
            || diagnostics.contains("init-diagnostics/src/lib.rs"),
        "expected the authored init fixture path in:\n{diagnostics}"
    );

    for (zero_based_line, expected) in [
        (7, "expected 0 arguments, found 1"),
        (8, "expected u32, found u64"),
    ] {
        assert!(
            diagnostics.contains(&format!("LineCol {{ line: {zero_based_line},")),
            "expected a diagnostic on authored zero-based line {zero_based_line}:\n{diagnostics}"
        );
        assert!(
            diagnostics.contains(expected),
            "expected diagnostic `{expected}`:\n{diagnostics}"
        );
    }
}

struct FixtureOutput {
    interface_source: PathBuf,
    cargo_config: PathBuf,
    cargo_dir: PathBuf,
}

impl FixtureOutput {
    fn new(root: &Path) -> Self {
        let interface_source = root.join("interfaces.rs");
        let cargo_dir = root.join(".cargo");
        let cargo_config = cargo_dir.join("config.toml");
        assert!(
            !interface_source.exists() && !cargo_config.exists(),
            "the Rust Analyzer fixture output paths must be generator-owned"
        );
        Self {
            interface_source,
            cargo_config,
            cargo_dir,
        }
    }
}

impl Drop for FixtureOutput {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.interface_source);
        let _ = fs::remove_file(&self.cargo_config);
        if self
            .cargo_dir
            .read_dir()
            .is_ok_and(|mut entries| entries.next().is_none())
        {
            let _ = fs::remove_dir(&self.cargo_dir);
        }
    }
}

fn check_failure(project: &Path, manifest: &Path, expected: &str) {
    let output = Command::new(cargo())
        .args([
            "check",
            "--lib",
            "--locked",
            "--offline",
            "--color",
            "never",
        ])
        .arg("--manifest-path")
        .arg(manifest)
        .env("CARGO_TARGET_DIR", project.join("target"))
        .current_dir(project)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "invalid init unexpectedly checked"
    );
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains(expected),
        "expected diagnostic `{expected}` in:\n{diagnostics}"
    );
}
