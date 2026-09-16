//! Framework-owned orchestration for the standalone source-transplant path.
//!
//! Each firmware supplies its own composition and firmware-rendering choices;
//! the stage order, failure reporting, and Cargo invocation live here so no
//! firmware package carries a copy of the pipeline.

use std::{
    env,
    error::Error,
    ffi::OsString,
    fmt,
    path::{Path, PathBuf},
    process::Command,
};

use ferroforge_renderer::{
    RenderError,
    composition::{StandaloneComposition, ValidatedComposition, validate_composition},
    init_check::{InitCheckOptions, RenderedInitCheck, render_init_check},
    source::{
        InitPackage, TaskPackage, TaskSources, discover_init_package, discover_task_package,
    },
    standalone::RenderedStandaloneProject,
};

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
    pub fn new(stage: PipelineStage, message: impl Into<String>) -> Self {
        Self {
            stage,
            message: message.into(),
        }
    }

    pub fn render(stage: PipelineStage, error: RenderError) -> Self {
        Self::new(stage, error.to_string())
    }
}

impl fmt::Display for PipelineError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} failed: {}", self.stage, self.message)
    }
}

impl Error for PipelineError {}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CargoCommand {
    pub current_dir: PathBuf,
    pub args: Vec<OsString>,
}

pub trait CommandExecutor {
    fn execute(&mut self, stage: PipelineStage, command: &CargoCommand)
    -> Result<(), PipelineError>;
}

/// Runs each stage as a child Cargo process so target code is never linked
/// into the host.
pub struct ProcessExecutor {
    cargo: OsString,
}

impl ProcessExecutor {
    pub fn from_env() -> Self {
        Self {
            cargo: env::var_os("CARGO").unwrap_or_else(|| "cargo".into()),
        }
    }
}

impl Default for ProcessExecutor {
    fn default() -> Self {
        Self::from_env()
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

/// Firmware-owned paths and names the pipeline needs. Everything here is
/// authored by the firmware; none of it is framework policy.
#[derive(Clone, Copy, Debug)]
pub struct PipelineInputs<'a> {
    pub repository_root: &'a Path,
    pub task_manifest: &'a Path,
    pub init_manifest: &'a Path,
    pub init_check_dir: &'a Path,
    pub firmware_dir: &'a Path,
    pub init_check_package_name: &'a str,
    pub firmware_package_name: &'a str,
    pub rust_target: &'a str,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedFirmware {
    pub init_check: RenderedInitCheck,
    pub firmware: RenderedStandaloneProject,
}

/// Run every stage in order, stopping at the first failure so no later stage
/// can report success against stale output.
pub fn run<C, F>(
    inputs: &PipelineInputs<'_>,
    build_composition: C,
    render_firmware: F,
    executor: &mut impl CommandExecutor,
) -> Result<RenderedFirmware, PipelineError>
where
    C: FnOnce(&TaskSources) -> StandaloneComposition,
    F: FnOnce(
        &TaskPackage,
        &InitPackage,
        &ValidatedComposition<'_>,
    ) -> Result<RenderedStandaloneProject, RenderError>,
{
    executor.execute(
        PipelineStage::ReusableTaskCheck,
        &CargoCommand {
            current_dir: inputs.repository_root.to_path_buf(),
            args: args_with_path(
                &[
                    "check",
                    "--lib",
                    "--target",
                    inputs.rust_target,
                    "--manifest-path",
                ],
                inputs.task_manifest,
                &["--locked", "--offline"],
            ),
        },
    )?;

    let stage = PipelineStage::CompositionValidation;
    let task_package =
        discover_task_package(inputs.task_manifest).map_err(|e| PipelineError::render(stage, e))?;
    let init_package =
        discover_init_package(inputs.init_manifest).map_err(|e| PipelineError::render(stage, e))?;
    let composition = build_composition(&task_package.sources);
    let validated = validate_composition(&task_package.sources, &composition)
        .map_err(|e| PipelineError::render(stage, e))?;

    let init_check = render_init_check(
        &init_package,
        &validated,
        InitCheckOptions {
            package_name: inputs.init_check_package_name,
            output_dir: inputs.init_check_dir,
        },
    )
    .map_err(|e| PipelineError::render(PipelineStage::InitInterfaceGeneration, e))?;
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

    let firmware = render_firmware(&task_package, &init_package, &validated)
        .map_err(|e| PipelineError::render(PipelineStage::FirmwareGeneration, e))?;
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
                inputs.firmware_package_name,
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
                inputs.firmware_package_name,
                "--locked",
                "--offline",
            ]),
        },
    )?;

    Ok(RenderedFirmware {
        init_check,
        firmware,
    })
}

pub fn strings(values: &[&str]) -> Vec<OsString> {
    values.iter().map(OsString::from).collect()
}

pub fn args_with_path(prefix: &[&str], path: &Path, suffix: &[&str]) -> Vec<OsString> {
    prefix
        .iter()
        .map(OsString::from)
        .chain(std::iter::once(path.as_os_str().to_owned()))
        .chain(suffix.iter().map(OsString::from))
        .collect()
}
