//! The call-through renderer must emit an app that really compiles: adapters
//! that construct each reusable task's own context and call it, with no
//! transplanted bodies.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

use ferroforge_renderer::{
    callthrough::{CallthroughProjectOptions, render_callthrough_app, render_callthrough_project},
    composition::{
        ConfigurationBinding, MonotonicProfile, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskSelection, validate_composition_across,
    },
    dependencies::{DependencyContributor, DependencyRequirement, DependencySource},
    source::{DefinitionId, ModuleId, TaskSources, discover_task_package},
    standalone::StandaloneTarget,
    transplant::RticAppShell,
};

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn reusable_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../experiments/callthrough/blinky-reusable/Cargo.toml")
}

fn id(sources: &TaskSources, name: &str) -> DefinitionId {
    DefinitionId {
        module: ModuleId {
            crate_root: sources.root.crate_root.clone(),
            path: Vec::new(),
        },
        name: name.to_owned(),
    }
}

fn binding(requirement: &str, resource: &str) -> ResourceBinding {
    ResourceBinding {
        requirement: requirement.to_owned(),
        resource: resource.to_owned(),
    }
}

fn shell() -> RticAppShell {
    RticAppShell {
        crate_imports: vec![
            "use defmt_rtt as _;".to_owned(),
            "use panic_probe as _;".to_owned(),
        ],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
        shared: "struct Shared { blink_enabled: bool }".to_owned(),
        local: "struct Local {
            status_led: stm32f4xx_hal::gpio::PA5<
                stm32f4xx_hal::gpio::Output<stm32f4xx_hal::gpio::PushPull>,
            >,
            blink_count: u32,
        }"
        .to_owned(),
        init: "fn init(cx: init::Context) -> (Shared, Local) {
            use stm32f4xx_hal::prelude::*;
            let mut rcc = cx.device.RCC.freeze(stm32f4xx_hal::rcc::Config::hsi().sysclk(84.MHz()));
            Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());
            let gpioa = cx.device.GPIOA.split(&mut rcc);
            let mut status_led = gpioa.pa5.into_push_pull_output();
            status_led.set_low();
            status_blink::spawn().unwrap();
            (Shared { blink_enabled: true }, Local { status_led, blink_count: 0 })
        }"
        .to_owned(),
    }
}

struct TempProject(PathBuf);

impl TempProject {
    /// Copy the firmware fixture so the emitted source can be checked with the
    /// real target configuration and dependency set.
    fn new(source: &str) -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "ferroforge-callthrough-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        let fixture = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../experiments/callthrough/firmware");
        fs::create_dir_all(root.join("src")).unwrap();
        fs::create_dir_all(root.join(".cargo")).unwrap();
        for (from, to) in [
            ("Cargo.toml", "Cargo.toml"),
            ("memory.x", "memory.x"),
            (".cargo/config.toml", ".cargo/config.toml"),
        ] {
            fs::copy(fixture.join(from), root.join(to)).unwrap();
        }
        // Point the copy at the reusable crate and at this source.
        let manifest = fs::read_to_string(root.join("Cargo.toml"))
            .unwrap()
            .replace(
                "blinky-tasks = { path = \"../blinky-tasks\" }",
                &format!(
                    "blinky-reusable = {{ path = \"{}\" }}",
                    fixture
                        .join("../blinky-reusable")
                        .canonicalize()
                        .unwrap()
                        .to_string_lossy()
                        .trim_start_matches(r"\\?\")
                        .replace('\\', "/")
                ),
            );
        fs::write(root.join("Cargo.toml"), manifest).unwrap();
        fs::write(root.join("src/main.rs"), source).unwrap();
        Self(root)
    }
}

impl Drop for TempProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(env::temp_dir().join(format!(
            "ferroforge-callthrough-project-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn dependency(
    name: &str,
    version: &str,
    default_features: bool,
    features: &[&str],
) -> DependencyRequirement {
    DependencyRequirement {
        name: name.to_owned(),
        package: name.to_owned(),
        source: DependencySource::Registry(
            "registry+https://github.com/rust-lang/crates.io-index".to_owned(),
        ),
        version_requirement: version.to_owned(),
        default_features,
        features: features.iter().map(|f| (*f).to_owned()).collect(),
        contributor: DependencyContributor::System("test".to_owned()),
    }
}

fn system_dependencies() -> Vec<DependencyRequirement> {
    vec![
        dependency("cortex-m", "0.7.7", true, &[]),
        dependency("cortex-m-rt", "0.7.6", true, &[]),
        dependency("defmt", "1.0.1", true, &[]),
        dependency("defmt-rtt", "1.3.0", true, &[]),
        dependency("panic-probe", "1.0.0", true, &["print-defmt"]),
        dependency("rtic", "2.3.1", false, &["thumbv7-backend"]),
        dependency("rtic-monotonics", "2.2.1", false, &["cortex-m-systick"]),
        dependency("stm32f4xx-hal", "0.23.0", false, &["stm32f401"]),
    ]
}

fn blinky_composition(sources: &TaskSources) -> StandaloneComposition {
    StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: "status_blink".to_owned(),
                definition: id(sources, "blink"),
                priority: 1,
                interrupt: None,
                local: vec![
                    binding("led", "status_led"),
                    binding("count", "blink_count"),
                ],
                shared: vec![binding("enabled", "blink_enabled")],
                configuration: vec![ConfigurationBinding {
                    name: "period_ms".to_owned(),
                    rust_type: "u32".to_owned(),
                    value: "500".to_owned(),
                }],
                spawn: vec![SpawnBinding {
                    alias: "report".to_owned(),
                    target: "telemetry".to_owned(),
                }],
            },
            TaskSelection {
                instance: "telemetry".to_owned(),
                definition: id(sources, "report"),
                priority: 1,
                interrupt: None,
                local: Vec::new(),
                shared: Vec::new(),
                configuration: Vec::new(),
                spawn: Vec::new(),
            },
        ],
        monotonic: Some(MonotonicProfile::initial_systick()),
    }
}

#[test]
fn renders_adapters_that_compile_against_the_reusable_crate() {
    let package = discover_task_package(&reusable_manifest()).unwrap();
    let composition = blinky_composition(&package.sources);
    let validated = validate_composition_across(&[&package], &composition).unwrap();
    let source = render_callthrough_app(&validated, &shell()).unwrap();

    // The body is never copied: the adapter calls the reusable definition.
    assert!(source.contains("blinky_reusable::blink"), "{source}");
    assert!(!source.contains("wrapping_add"), "{source}");
    assert!(source.contains("local = [blink_count, status_led]"), "{source}");
    assert!(source.contains("shared = [blink_enabled]"), "{source}");
    // Configuration arrives as a const generic argument, not an inlined literal.
    assert!(source.contains("500"), "{source}");

    let project = TempProject::new(&source);
    let status = Command::new(cargo())
        .args([
            "check",
            "--bin",
            "callthrough-firmware",
            "--offline",
        ])
        .current_dir(&project.0)
        .status()
        .unwrap();
    assert!(status.success(), "rendered call-through app must check:\n{source}");
}

/// The renderer must emit a complete, buildable project - not only source -
/// which is the same bar the transplant path meets.
#[test]
fn emits_a_project_that_checks_and_release_links() {
    let package = discover_task_package(&reusable_manifest()).unwrap();
    let composition = blinky_composition(&package.sources);
    let validated = validate_composition_across(&[&package], &composition).unwrap();

    let output = TempDir::new();
    let rendered = render_callthrough_project(
        &validated,
        CallthroughProjectOptions {
            package_name: "callthrough-firmware",
            output_dir: &output.0,
            shell: &shell(),
            target: &StandaloneTarget::stm32f401re(),
            system_dependencies: &system_dependencies(),
            task_packages: &[&package],
        },
    )
    .unwrap();

    let manifest = fs::read_to_string(&rendered.manifest).unwrap();
    // The reusable crate is a real dependency now, not a source input.
    assert!(manifest.contains("blinky-reusable = { path ="), "{manifest}");
    assert!(rendered.memory_layout.exists());
    assert!(rendered.cargo_config.exists());
    assert!(rendered.embed_config.exists());

    for arguments in [
        ["check", "--bin", "callthrough-firmware", "--offline"],
        ["build", "--release", "--bin", "callthrough-firmware"],
    ] {
        let mut command = Command::new(cargo());
        command.args(arguments);
        if arguments[0] == "build" {
            command.arg("--offline");
        }
        let status = command.current_dir(&rendered.root).status().unwrap();
        assert!(
            status.success(),
            "generated project must {}",
            arguments[0]
        );
    }
}
