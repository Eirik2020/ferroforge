use std::{
    collections::BTreeSet,
    env,
    error::Error,
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use ferroforge_renderer::{
    RenderError,
    composition::{
        ConfigurationBinding, MonotonicProfile, ResourceBinding, SpawnBinding,
        StandaloneComposition, TaskSelection, ValidatedComposition, validate_composition,
    },
    dependencies::{DependencyContributor, DependencyRequirement, DependencySource},
    init_check::{InitCheckOptions, RenderedInitCheck, render_init_check},
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedNucleoF401re {
    pub init_check: RenderedInitCheck,
    pub firmware: RenderedStandaloneProject,
}

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

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PipelineStage {
    ReusableTaskCheck,
    CompositionValidation,
    InitInterfaceGeneration,
    InitCheck,
    FirmwareGeneration,
    FirmwareDependencyResolution,
    FirmwareCheck,
    FirmwareBuild,
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::ReusableTaskCheck => "reusable task check",
            Self::CompositionValidation => "composition validation",
            Self::InitInterfaceGeneration => "init interface generation",
            Self::InitCheck => "system init check",
            Self::FirmwareGeneration => "firmware generation",
            Self::FirmwareDependencyResolution => "firmware dependency resolution",
            Self::FirmwareCheck => "generated firmware check",
            Self::FirmwareBuild => "generated firmware release build",
        })
    }
}

#[derive(Debug, PartialEq, Eq)]
pub struct PipelineError {
    pub stage: PipelineStage,
    pub message: String,
}

impl PipelineError {
    fn new(stage: PipelineStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }

    fn render(stage: PipelineStage, error: RenderError) -> Self {
        Self::new(stage, error.to_string())
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed: {}", self.stage, self.message)
    }
}

impl Error for PipelineError {}

struct CargoCommand {
    current_dir: PathBuf,
    args: Vec<OsString>,
}

trait CommandExecutor {
    fn execute(
        &mut self,
        stage: PipelineStage,
        command: &CargoCommand,
    ) -> Result<(), PipelineError>;
}

struct ProcessExecutor {
    cargo: OsString,
}

struct PipelineSources {
    task_manifest: PathBuf,
    init_manifest: PathBuf,
}

impl PipelineSources {
    fn blinky(repository_root: &Path, init_manifest: &Path) -> Self {
        Self {
            task_manifest: repository_root.join("tasks/blinky/Cargo.toml"),
            init_manifest: init_manifest.to_path_buf(),
        }
    }
}

impl CommandExecutor for ProcessExecutor {
    fn execute(
        &mut self,
        stage: PipelineStage,
        command: &CargoCommand,
    ) -> Result<(), PipelineError> {
        let status = Command::new(&self.cargo)
            .args(&command.args)
            .current_dir(&command.current_dir)
            .status()
            .map_err(|error| PipelineError::new(stage, error.to_string()))?;
        if status.success() {
            Ok(())
        } else {
            Err(PipelineError::new(
                stage,
                format!("Cargo exited with {status}"),
            ))
        }
    }
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
        &repository_root.join("systems/nucleo-f401re/init/Cargo.toml"),
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
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let sources = PipelineSources::blinky(repository_root, init_manifest);
    run_pipeline_with_executor(
        repository_root,
        &sources,
        init_check_dir,
        firmware_dir,
        profile,
        &mut ProcessExecutor { cargo },
    )
}

fn run_pipeline_with_executor(
    repository_root: &Path,
    sources: &PipelineSources,
    init_check_dir: &Path,
    firmware_dir: &Path,
    profile: &BlinkyNucleoF401reProfile,
    executor: &mut impl CommandExecutor,
) -> Result<RenderedNucleoF401re, PipelineError> {
    let target = StandaloneTarget::stm32f401re();

    executor.execute(
        PipelineStage::ReusableTaskCheck,
        &CargoCommand {
            current_dir: repository_root.to_path_buf(),
            args: args_with_path(
                &[
                    "check",
                    "--lib",
                    "--target",
                    &target.rust_target,
                    "--manifest-path",
                ],
                &sources.task_manifest,
                &["--locked", "--offline"],
            ),
        },
    )?;

    let task_package = discover_task_package(&sources.task_manifest)
        .map_err(|error| PipelineError::render(PipelineStage::CompositionValidation, error))?;
    let init_package = discover_init_package(&sources.init_manifest)
        .map_err(|error| PipelineError::render(PipelineStage::CompositionValidation, error))?;
    let composition = composition(&task_package.sources, profile);
    let validated = validate_composition(&task_package.sources, &composition)
        .map_err(|error| PipelineError::render(PipelineStage::CompositionValidation, error))?;

    let init_check = render_init_check(
        &init_package,
        &validated,
        InitCheckOptions {
            package_name: &profile.init_check_package_name,
            output_dir: init_check_dir,
        },
    )
    .map_err(|error| PipelineError::render(PipelineStage::InitInterfaceGeneration, error))?;
    executor.execute(
        PipelineStage::InitCheck,
        &CargoCommand {
            current_dir: init_check.root.clone(),
            args: args_with_path(
                &["check", "--lib", "--manifest-path"],
                &init_check.manifest,
                &["--locked", "--offline"],
            ),
        },
    )?;

    let firmware = render_firmware(
        &task_package,
        &init_package,
        &validated,
        firmware_dir,
        &target,
        profile,
    )
    .map_err(|error| PipelineError::render(PipelineStage::FirmwareGeneration, error))?;
    executor.execute(
        PipelineStage::FirmwareDependencyResolution,
        &CargoCommand {
            current_dir: firmware.root.clone(),
            args: strings(&["generate-lockfile", "--offline"]),
        },
    )?;
    executor.execute(
        PipelineStage::FirmwareCheck,
        &CargoCommand {
            current_dir: firmware.root.clone(),
            args: strings(&[
                "check",
                "--bin",
                &profile.firmware_package_name,
                "--locked",
                "--offline",
            ]),
        },
    )?;
    executor.execute(
        PipelineStage::FirmwareBuild,
        &CargoCommand {
            current_dir: firmware.root.clone(),
            args: strings(&[
                "build",
                "--release",
                "--bin",
                &profile.firmware_package_name,
                "--locked",
                "--offline",
            ]),
        },
    )?;

    Ok(RenderedNucleoF401re {
        init_check,
        firmware,
    })
}

pub fn render_nucleo_f401re(
    repository_root: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
) -> Result<RenderedNucleoF401re, RenderError> {
    render_blinky_nucleo_f401re(
        repository_root,
        &repository_root.join("systems/nucleo-f401re/init/Cargo.toml"),
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
    let task_package = discover_task_package(&repository_root.join("tasks/blinky/Cargo.toml"))?;
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

fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

fn args_with_path(prefix: &[&str], path: &Path, suffix: &[&str]) -> Vec<OsString> {
    prefix
        .iter()
        .map(OsString::from)
        .chain(std::iter::once(path.as_os_str().to_owned()))
        .chain(suffix.iter().map(OsString::from))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
    };

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

    #[test]
    fn command_failure_stops_every_later_pipeline_stage() {
        let repository_root = repository_root();
        let profile = BlinkyNucleoF401reProfile::primary();
        let sources = PipelineSources::blinky(
            &repository_root,
            &repository_root.join("systems/nucleo-f401re/init/Cargo.toml"),
        );
        let command_stages = [
            PipelineStage::ReusableTaskCheck,
            PipelineStage::InitCheck,
            PipelineStage::FirmwareDependencyResolution,
            PipelineStage::FirmwareCheck,
            PipelineStage::FirmwareBuild,
        ];

        for (index, fail_at) in command_stages.iter().copied().enumerate() {
            let output = TempOutput::new();
            let mut executor = FakeExecutor {
                fail_at,
                seen: Vec::new(),
            };
            let error = run_pipeline_with_executor(
                &repository_root,
                &sources,
                &output.0.join(format!("init-check-{index}")),
                &output.0.join(format!("gen-app-{index}")),
                &profile,
                &mut executor,
            )
            .unwrap_err();

            assert_eq!(error.stage, fail_at);
            assert_eq!(executor.seen, command_stages[..=index]);
        }
    }

    #[test]
    fn real_failures_stop_at_their_pipeline_boundaries() {
        let repository_root = repository_root();
        let profile = BlinkyNucleoF401reProfile::primary();
        let default_sources = PipelineSources::blinky(
            &repository_root,
            &repository_root.join("systems/nucleo-f401re/init/Cargo.toml"),
        );
        let output = TempOutput::new();
        let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
        let command_stages = [
            PipelineStage::ReusableTaskCheck,
            PipelineStage::InitCheck,
            PipelineStage::FirmwareDependencyResolution,
            PipelineStage::FirmwareCheck,
            PipelineStage::FirmwareBuild,
        ];
        let cases = [
            (
                "source-check",
                PipelineSources {
                    task_manifest: fixture(
                        &repository_root,
                        "ferroforge-renderer/tests/fixtures/ra-diagnostics",
                    ),
                    init_manifest: default_sources.init_manifest.clone(),
                },
                PipelineStage::ReusableTaskCheck,
                1,
                false,
            ),
            (
                "composition",
                PipelineSources {
                    task_manifest: fixture(
                        &repository_root,
                        "ferroforge-renderer/tests/fixtures/sw",
                    ),
                    init_manifest: default_sources.init_manifest.clone(),
                },
                PipelineStage::CompositionValidation,
                1,
                false,
            ),
            (
                "init-check",
                PipelineSources {
                    task_manifest: default_sources.task_manifest.clone(),
                    init_manifest: fixture(
                        &repository_root,
                        "ferroforge-renderer/tests/fixtures/init-diagnostics",
                    ),
                },
                PipelineStage::InitCheck,
                2,
                false,
            ),
            (
                "firmware-generation",
                PipelineSources {
                    task_manifest: default_sources.task_manifest.clone(),
                    init_manifest: fixture(
                        &repository_root,
                        "systems/nucleo-f401re/app_composition/tests/fixtures/init-check-only-leak",
                    ),
                },
                PipelineStage::FirmwareGeneration,
                2,
                false,
            ),
            (
                "firmware-check",
                PipelineSources {
                    task_manifest: default_sources.task_manifest.clone(),
                    init_manifest: fixture(
                        &repository_root,
                        "systems/nucleo-f401re/app_composition/tests/fixtures/init-wrong-resource",
                    ),
                },
                PipelineStage::FirmwareCheck,
                4,
                false,
            ),
            (
                "firmware-link",
                default_sources,
                PipelineStage::FirmwareBuild,
                5,
                true,
            ),
        ];

        for (name, sources, expected_stage, command_count, corrupt_memory) in cases {
            let mut executor = TestProcessExecutor {
                cargo: cargo.clone(),
                target_dir: output.0.join("target"),
                corrupt_memory_before_build: corrupt_memory,
                seen: Vec::new(),
            };
            let error = run_pipeline_with_executor(
                &repository_root,
                &sources,
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
                command_stages[..command_count],
                "later command ran after {name} failed",
            );
        }
    }
}
