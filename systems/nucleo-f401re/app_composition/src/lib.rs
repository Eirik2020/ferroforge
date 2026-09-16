//! System-owned composition for the Nucleo-F401RE blinky firmware.
//!
//! Stage order, failure reporting, and Cargo invocation belong to
//! `ferroforge-pipeline`. What stays here is what this firmware authors: its
//! instance and resource names, its task graph, and its runtime selections.

use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use ferroforge_pipeline::{CommandExecutor, PipelineInputs, ProcessExecutor};
pub use ferroforge_pipeline::{
    PipelineError, PipelineStage, RenderedFirmware as RenderedNucleoF401re,
};

use ferroforge_renderer::{
    RenderError,
    composition::{
        ConfigurationBinding, MonotonicProfile, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskSelection, ValidatedComposition, validate_composition,
    },
    dependencies::{DependencyContributor, DependencyRequirement, DependencySource},
    init_check::{InitCheckOptions, render_init_check},
    source::{
        DefinitionId, InitPackage, TaskPackage, TaskSources, discover_init_package,
        discover_task_package,
    },
    standalone::{
        RenderedStandaloneProject, StandaloneProjectOptions, StandaloneTarget,
        render_standalone_project,
    },
    transplant::RticAppTarget,
};

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

/// System-owned names and values for the bounded reusable blinky composition
/// on the Nucleo-F401RE target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BlinkyNucleoF401reProfile {
    pub init_check_package_name: String,
    pub firmware_package_name: String,
    pub blink_instance: String,
    pub report_instance: String,
    pub led_resource: String,
    pub count_resource: String,
    pub enabled_resource: String,
    pub period_ms: u32,
}

impl BlinkyNucleoF401reProfile {
    pub fn primary() -> Self {
        Self {
            init_check_package_name: "nucleo-f401re-init-check".to_owned(),
            firmware_package_name: "nucleo-f401re-rtic".to_owned(),
            blink_instance: "status_blink".to_owned(),
            report_instance: "telemetry".to_owned(),
            led_resource: "status_led".to_owned(),
            count_resource: "blink_count".to_owned(),
            enabled_resource: "blink_enabled".to_owned(),
            period_ms: 500,
        }
    }
}

pub fn task_manifest(repository_root: &Path) -> PathBuf {
    repository_root.join("tasks/blinky/Cargo.toml")
}

fn init_manifest(repository_root: &Path) -> PathBuf {
    repository_root.join("systems/nucleo-f401re/init/Cargo.toml")
}

/// Run the complete first-system pipeline without linking target code into the
/// host process. Each Cargo stage must succeed before the next stage begins.
pub fn run_nucleo_f401re_pipeline(
    repository_root: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
) -> Result<RenderedNucleoF401re, PipelineError> {
    run_blinky_nucleo_f401re_pipeline(
        repository_root,
        &init_manifest(repository_root),
        init_check_dir,
        firmware_dir,
        &BlinkyNucleoF401reProfile::primary(),
    )
}

/// Run one system-owned blinky composition on the supported Nucleo-F401RE
/// target while keeping the reusable task package unchanged.
pub fn run_blinky_nucleo_f401re_pipeline(
    repository_root: &Path,
    init_manifest: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
    profile: &BlinkyNucleoF401reProfile,
) -> Result<RenderedNucleoF401re, PipelineError> {
    run_blinky_pipeline_with_executor(
        repository_root,
        &task_manifest(repository_root),
        init_manifest,
        init_check_dir,
        firmware_dir,
        profile,
        &mut ProcessExecutor::from_env(),
    )
}

/// The same pipeline with an explicit executor, so tests can inject failures
/// at a chosen stage without running Cargo.
pub fn run_blinky_pipeline_with_executor(
    repository_root: &Path,
    task_manifest: &Path,
    init_manifest: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
    profile: &BlinkyNucleoF401reProfile,
    executor: &mut impl CommandExecutor,
) -> Result<RenderedNucleoF401re, PipelineError> {
    let target = StandaloneTarget::stm32f401re();
    let inputs = PipelineInputs {
        repository_root,
        task_manifest,
        init_manifest,
        init_check_dir,
        firmware_dir,
        init_check_package_name: &profile.init_check_package_name,
        firmware_package_name: &profile.firmware_package_name,
        rust_target: &target.rust_target,
    };

    ferroforge_pipeline::run(
        &inputs,
        |sources| composition(sources, profile),
        |task_package, init_package, validated| {
            render_firmware(
                task_package,
                init_package,
                validated,
                firmware_dir,
                &target,
                profile,
            )
        },
        executor,
    )
}

pub fn render_nucleo_f401re(
    repository_root: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
) -> Result<RenderedNucleoF401re, RenderError> {
    render_blinky_nucleo_f401re(
        repository_root,
        &init_manifest(repository_root),
        init_check_dir,
        firmware_dir,
        &BlinkyNucleoF401reProfile::primary(),
    )
}

pub fn render_blinky_nucleo_f401re(
    repository_root: &Path,
    init_manifest: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
    profile: &BlinkyNucleoF401reProfile,
) -> Result<RenderedNucleoF401re, RenderError> {
    let task_package = discover_task_package(&task_manifest(repository_root))?;
    let init_package = discover_init_package(init_manifest)?;
    let composition = composition(&task_package.sources, profile);
    let validated = validate_composition(&task_package.sources, &composition)?;

    let init_check = render_init_check(
        &init_package,
        &validated,
        InitCheckOptions {
            package_name: &profile.init_check_package_name,
            output_dir: init_check_dir,
        },
    )?;
    let target = StandaloneTarget::stm32f401re();
    let firmware = render_firmware(
        &task_package,
        &init_package,
        &validated,
        firmware_dir,
        &target,
        profile,
    )?;

    Ok(RenderedNucleoF401re {
        init_check,
        firmware,
    })
}

fn render_firmware(
    task_package: &TaskPackage,
    init_package: &InitPackage,
    validated: &ValidatedComposition<'_>,
    firmware_dir: &Path,
    target: &StandaloneTarget,
    profile: &BlinkyNucleoF401reProfile,
) -> Result<RenderedStandaloneProject, RenderError> {
    let app_target = RticAppTarget {
        crate_imports: vec![
            "use defmt_rtt as _;".to_owned(),
            "use panic_probe as _;".to_owned(),
        ],
        device: "stm32f4xx_hal::pac".to_owned(),
        app_module: "app".to_owned(),
        dispatchers: vec!["USART1".to_owned()],
    };
    let system_dependencies = system_dependencies();
    render_standalone_project(
        task_package,
        init_package,
        validated,
        StandaloneProjectOptions {
            package_name: &profile.firmware_package_name,
            output_dir: firmware_dir,
            app_target: &app_target,
            target,
            system_dependencies: &system_dependencies,
        },
    )
}

fn composition(
    sources: &TaskSources,
    profile: &BlinkyNucleoF401reProfile,
) -> StandaloneComposition {
    StandaloneComposition {
        tasks: vec![
            TaskSelection {
                instance: profile.blink_instance.clone(),
                definition: definition(sources, "blink"),
                priority: 1,
                interrupt: None,
                local: vec![
                    resource("led", &profile.led_resource),
                    resource("count", &profile.count_resource),
                ],
                shared: vec![resource("enabled", &profile.enabled_resource)],
                configuration: vec![ConfigurationBinding {
                    name: "period_ms".to_owned(),
                    rust_type: "u32".to_owned(),
                    value: profile.period_ms.to_string(),
                }],
                spawn: vec![SpawnBinding {
                    alias: "report".to_owned(),
                    target: profile.report_instance.clone(),
                }],
            },
            TaskSelection {
                instance: profile.report_instance.clone(),
                definition: definition(sources, "report"),
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

fn definition(sources: &TaskSources, name: &str) -> DefinitionId {
    DefinitionId {
        module: sources.root.clone(),
        name: name.to_owned(),
    }
}

fn resource(requirement: &str, resource: &str) -> ResourceBinding {
    ResourceBinding {
        requirement: requirement.to_owned(),
        resource: resource.to_owned(),
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

#[cfg(test)]
mod tests {
    use std::{
        env,
        ffi::OsString,
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

    use ferroforge_pipeline::CargoCommand;

    use super::*;

    struct FakeExecutor {
        fail_at: PipelineStage,
        seen: Vec<PipelineStage>,
    }

    impl CommandExecutor for FakeExecutor {
        fn execute(
            &mut self,
            stage: PipelineStage,
            command: &CargoCommand,
        ) -> Result<(), PipelineError> {
            assert!(!command.args.is_empty());
            self.seen.push(stage);
            if stage == self.fail_at {
                Err(PipelineError::new(stage, "injected command failure"))
            } else {
                Ok(())
            }
        }
    }

    struct TestProcessExecutor {
        cargo: OsString,
        target_dir: PathBuf,
        corrupt_memory_before_build: bool,
        seen: Vec<PipelineStage>,
    }

    impl CommandExecutor for TestProcessExecutor {
        fn execute(
            &mut self,
            stage: PipelineStage,
            command: &CargoCommand,
        ) -> Result<(), PipelineError> {
            self.seen.push(stage);
            if self.corrupt_memory_before_build && stage == PipelineStage::FirmwareBuild {
                fs::write(
                    command.current_dir.join("memory.x"),
                    "this is not a linker script\n",
                )
                .map_err(|error| PipelineError::new(stage, error.to_string()))?;
            }

            let output = Command::new(&self.cargo)
                .args(&command.args)
                .env("CARGO_TARGET_DIR", &self.target_dir)
                .current_dir(&command.current_dir)
                .output()
                .map_err(|error| PipelineError::new(stage, error.to_string()))?;
            if output.status.success() {
                Ok(())
            } else {
                Err(PipelineError::new(
                    stage,
                    format!(
                        "Cargo exited with {}\n{}{}",
                        output.status,
                        String::from_utf8_lossy(&output.stdout),
                        String::from_utf8_lossy(&output.stderr),
                    ),
                ))
            }
        }
    }

    struct TempOutput(PathBuf);

    impl TempOutput {
        fn new() -> Self {
            static NEXT: AtomicU64 = AtomicU64::new(0);
            Self(env::temp_dir().join(format!(
                "ferroforge-nucleo-pipeline-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            )))
        }
    }

    impl Drop for TempOutput {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    fn repository_root() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(3)
            .expect("system package must be three levels below the repository root")
            .to_path_buf()
    }

    fn fixture(repository_root: &Path, relative: &str) -> PathBuf {
        repository_root.join(relative).join("Cargo.toml")
    }

    const COMMAND_STAGES: [PipelineStage; 5] = [
        PipelineStage::ReusableTaskCheck,
        PipelineStage::InitCheck,
        PipelineStage::FirmwareDependencyResolution,
        PipelineStage::FirmwareCheck,
        PipelineStage::FirmwareBuild,
    ];

    #[test]
    fn command_failure_stops_every_later_pipeline_stage() {
        let repository_root = repository_root();
        let profile = BlinkyNucleoF401reProfile::primary();
        let task = task_manifest(&repository_root);
        let init = init_manifest(&repository_root);

        for (index, fail_at) in COMMAND_STAGES.iter().copied().enumerate() {
            let output = TempOutput::new();
            let mut executor = FakeExecutor {
                fail_at,
                seen: Vec::new(),
            };
            let error = run_blinky_pipeline_with_executor(
                &repository_root,
                &task,
                &init,
                &output.0.join(format!("init-check-{index}")),
                &output.0.join(format!("gen-app-{index}")),
                &profile,
                &mut executor,
            )
            .unwrap_err();

            assert_eq!(error.stage, fail_at);
            assert_eq!(executor.seen, COMMAND_STAGES[..=index]);
        }
    }

    #[test]
    fn real_failures_stop_at_their_pipeline_boundaries() {
        let repository_root = repository_root();
        let profile = BlinkyNucleoF401reProfile::primary();
        let default_task = task_manifest(&repository_root);
        let default_init = init_manifest(&repository_root);
        let output = TempOutput::new();
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let cases = [
            (
                "source-check",
                fixture(
                    &repository_root,
                    "ferroforge-renderer/tests/fixtures/ra-diagnostics",
                ),
                default_init.clone(),
                PipelineStage::ReusableTaskCheck,
                1,
                false,
            ),
            (
                "composition",
                fixture(&repository_root, "ferroforge-renderer/tests/fixtures/sw"),
                default_init.clone(),
                PipelineStage::CompositionValidation,
                1,
                false,
            ),
            (
                "init-check",
                default_task.clone(),
                fixture(
                    &repository_root,
                    "ferroforge-renderer/tests/fixtures/init-diagnostics",
                ),
                PipelineStage::InitCheck,
                2,
                false,
            ),
            (
                "firmware-generation",
                default_task.clone(),
                fixture(
                    &repository_root,
                    "systems/nucleo-f401re/app_composition/tests/fixtures/init-check-only-leak",
                ),
                PipelineStage::FirmwareGeneration,
                2,
                false,
            ),
            (
                "firmware-check",
                default_task.clone(),
                fixture(
                    &repository_root,
                    "systems/nucleo-f401re/app_composition/tests/fixtures/init-wrong-resource",
                ),
                PipelineStage::FirmwareCheck,
                4,
                false,
            ),
            (
                "firmware-link",
                default_task.clone(),
                default_init.clone(),
                PipelineStage::FirmwareBuild,
                5,
                true,
            ),
        ];

        for (name, task, init, expected_stage, command_count, corrupt_memory) in cases {
            let mut executor = TestProcessExecutor {
                cargo: cargo.clone(),
                target_dir: output.0.join("target"),
                corrupt_memory_before_build: corrupt_memory,
                seen: Vec::new(),
            };
            let error = run_blinky_pipeline_with_executor(
                &repository_root,
                &task,
                &init,
                &output.0.join(format!("{name}-init-check")),
                &output.0.join(format!("{name}-gen-app")),
                &profile,
                &mut executor,
            )
            .unwrap_err();

            assert_eq!(
                error.stage, expected_stage,
                "unexpected boundary for {name}"
            );
            assert_eq!(
                executor.seen,
                COMMAND_STAGES[..command_count],
                "later command ran after {name} failed",
            );
        }
    }
}
