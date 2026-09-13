use std::{
    collections::BTreeSet,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::{
    composition::{MonotonicProfile, StandaloneComposition, TaskSelection, validate_composition},
    dependencies::{DependencyContributor, DependencyRequirement, DependencySource},
    source::{DefinitionId, ModuleId, TaskSources, discover_init_package, discover_task_package},
    standalone::{StandaloneProjectOptions, StandaloneTarget, render_standalone_project},
    transplant::RticAppTarget,
};

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn fixture(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(format!("tests/fixtures/{name}/Cargo.toml"))
}

fn id(sources: &TaskSources, name: &str) -> DefinitionId {
    DefinitionId {
        module: ModuleId {
            crate_root: sources.root.crate_root.clone(),
            path: vec!["layout".to_owned()],
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

fn composition(sources: &TaskSources) -> StandaloneComposition {
    StandaloneComposition {
        tasks: vec![
            task("status", id(sources, "no_args")),
            task("telemetry", id(sources, "consumer")),
        ],
        monotonic: Some(MonotonicProfile::initial_systick()),
    }
}

fn app_target() -> RticAppTarget {
    RticAppTarget {
        crate_imports: vec![
            "use defmt_rtt as _;".to_owned(),
            "use panic_probe as _;".to_owned(),
        ],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
    }
}

fn dependency(
    name: &str,
    version: &str,
    default_features: bool,
    features: &[&str],
    selection: &str,
) -> DependencyRequirement {
    DependencyRequirement {
        name: name.to_owned(),
        package: name.to_owned(),
        source: DependencySource::Registry(CRATES_IO_SOURCE.to_owned()),
        version_requirement: version.to_owned(),
        default_features,
        features: features
            .iter()
            .map(|feature| (*feature).to_owned())
            .collect::<BTreeSet<_>>(),
        contributor: DependencyContributor::System(selection.to_owned()),
    }
}

fn system_dependencies() -> Vec<DependencyRequirement> {
    vec![
        dependency("cortex-m", "0.7.7", true, &[], "Cortex-M runtime"),
        dependency("cortex-m-rt", "0.7.6", true, &[], "Cortex-M runtime"),
        dependency("defmt-rtt", "1.3.0", true, &[], "logging backend"),
        dependency(
            "panic-probe",
            "1.0.0",
            true,
            &["print-defmt"],
            "panic backend",
        ),
        dependency("rtic", "2.3.1", false, &["thumbv7-backend"], "RTIC backend"),
        dependency(
            "rtic-monotonics",
            "2.2.1",
            false,
            &["cortex-m-systick"],
            "SysTick backend",
        ),
        dependency(
            "stm32f4xx-hal",
            "0.23.0",
            false,
            &["stm32f401"],
            "STM32F401 target",
        ),
    ]
}

struct TempProject(PathBuf);

impl TempProject {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(env::temp_dir().join(format!(
            "ferroforge-standalone-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed.
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn emits_checks_and_links_a_manifest_driven_standalone_project() {
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let init = discover_init_package(&fixture("init")).unwrap();
    let validated = validate_composition(&tasks.sources, &composition(&tasks.sources)).unwrap();
    let output = TempProject::new();
    let dependencies = system_dependencies();
    let app_target = app_target();
    let target = StandaloneTarget::stm32f401re();
    let rendered = render_standalone_project(
        &tasks,
        &init,
        &validated,
        StandaloneProjectOptions {
            package_name: "standalone-nucleo",
            output_dir: &output.0,
            app_target: &app_target,
            target: &target,
            system_dependencies: &dependencies,
        },
    )
    .unwrap();

    let manifest = fs::read_to_string(&rendered.manifest).unwrap();
    let source = fs::read_to_string(&rendered.main_source).unwrap();
    let cargo_config = fs::read_to_string(&rendered.cargo_config).unwrap();
    let memory_layout = fs::read_to_string(&rendered.memory_layout).unwrap();
    let embed_config = fs::read_to_string(&rendered.embed_config).unwrap();
    assert!(manifest.contains("embedded-hal = { version = \"1.0.0\""));
    assert!(manifest.contains("heapless = { version = \"0.8.0\""));
    assert!(manifest.contains("stm32f4xx-hal = { version = \"0.23.0\""));
    assert!(manifest.contains("features = [\"stm32f401\"]"));
    assert!(manifest.contains("rtic = { version = \"2.3.1\""));
    assert!(manifest.contains("features = [\"thumbv7-backend\"]"));
    assert!(manifest.contains("[profile.release]"));
    assert!(!manifest.contains("ferroforge"));
    assert!(!source.contains("ferroforge::"));
    assert!(source.contains("cx.device.RCC.freeze"));
    assert!(cargo_config.contains("target = \"thumbv7em-none-eabihf\""));
    assert!(cargo_config.contains("runner = \"probe-rs run --chip STM32F401RE\""));
    assert!(cargo_config.contains("link-arg=-Tlink.x"));
    assert!(cargo_config.contains("link-arg=-Tdefmt.x"));
    assert!(cargo_config.contains("DEFMT_LOG = \"info\""));
    assert!(memory_layout.contains("FLASH : ORIGIN = 0x08000000, LENGTH = 512K"));
    assert!(memory_layout.contains("RAM   : ORIGIN = 0x20000000, LENGTH = 96K"));
    assert!(memory_layout.contains("_stext = ORIGIN(FLASH) + 0x198;"));
    assert!(embed_config.contains("chip = \"STM32F401RE\""));

    let status = Command::new(cargo())
        .args(["check", "--bin", "standalone-nucleo", "--offline"])
        .current_dir(&rendered.root)
        .status()
        .unwrap();
    assert!(status.success(), "generated standalone project must check");

    let status = Command::new(cargo())
        .args([
            "build",
            "--release",
            "--bin",
            "standalone-nucleo",
            "--offline",
        ])
        .current_dir(&rendered.root)
        .status()
        .unwrap();
    assert!(
        status.success(),
        "generated standalone project must link in release mode"
    );
}

#[test]
fn rejects_invalid_target_packaging_before_writing() {
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let init = discover_init_package(&fixture("init")).unwrap();
    let validated = validate_composition(&tasks.sources, &composition(&tasks.sources)).unwrap();
    let output = TempProject::new();
    let dependencies = system_dependencies();
    let app_target = app_target();
    let mut target = StandaloneTarget::stm32f401re();
    target.text_offset = target.flash_size_bytes;

    let error = render_standalone_project(
        &tasks,
        &init,
        &validated,
        StandaloneProjectOptions {
            package_name: "standalone-nucleo",
            output_dir: &output.0,
            app_target: &app_target,
            target: &target,
            system_dependencies: &dependencies,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("text offset must be word-aligned and inside flash"));
    assert!(
        !output.0.exists(),
        "invalid target data must not leave partial output"
    );
}

#[test]
fn reports_system_and_source_manifest_dependency_conflicts_before_writing() {
    let tasks = discover_task_package(&fixture("sw")).unwrap();
    let init = discover_init_package(&fixture("init")).unwrap();
    let validated = validate_composition(&tasks.sources, &composition(&tasks.sources)).unwrap();
    let output = TempProject::new();
    let mut dependencies = system_dependencies();
    dependencies
        .iter_mut()
        .find(|dependency| dependency.name == "cortex-m")
        .unwrap()
        .version_requirement = "0.7.8".to_owned();
    let app_target = app_target();
    let target = StandaloneTarget::stm32f401re();

    let error = render_standalone_project(
        &tasks,
        &init,
        &validated,
        StandaloneProjectOptions {
            package_name: "standalone-nucleo",
            output_dir: &output.0,
            app_target: &app_target,
            target: &target,
            system_dependencies: &dependencies,
        },
    )
    .unwrap_err()
    .to_string();

    assert!(error.contains("dependency `cortex-m` has conflicting version requirement"));
    assert!(error.contains(&init.package.manifest.display().to_string()));
    assert!(error.contains("system selection `Cortex-M runtime`"));
    assert!(
        !output.0.exists(),
        "conflicts must not leave partial output"
    );
}
