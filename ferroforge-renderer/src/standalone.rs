//! Project emission for the bounded standalone source-transplant path.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    RenderError,
    composition::ValidatedComposition,
    dependencies::{DependencyRequirement, DependencySource, MergedDependency, merge_dependencies},
    source::{InitPackage, TaskPackage},
    transplant::{RticAppTarget, render_rtic_app_with_init},
};

const CRATES_IO_SOURCE: &str = "registry+https://github.com/rust-lang/crates.io-index";

#[derive(Clone, Debug)]
pub struct StandaloneProjectOptions<'a> {
    pub package_name: &'a str,
    pub output_dir: &'a Path,
    pub app_target: &'a RticAppTarget,
    pub target: &'a StandaloneTarget,
    /// Backend/runtime selections contributed by the system. These are merged
    /// conservatively with normal dependencies from the task and init packages.
    pub system_dependencies: &'a [DependencyRequirement],
}

/// Board/MCU-owned files needed to check, link, and run the generated target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StandaloneTarget {
    pub rust_target: String,
    pub probe_rs_chip: String,
    pub flash_origin: u32,
    pub flash_size_bytes: u32,
    pub ram_origin: u32,
    pub ram_size_bytes: u32,
    /// Offset from the flash origin at which `.text` begins after the vector
    /// table, including any linker-alignment padding.
    pub text_offset: u32,
    pub defmt_log: String,
}

impl StandaloneTarget {
    /// Initial target shared with the existing Nucleo-F401RE backend.
    pub fn stm32f401re() -> Self {
        Self {
            rust_target: "thumbv7em-none-eabihf".to_owned(),
            probe_rs_chip: "STM32F401RE".to_owned(),
            flash_origin: 0x0800_0000,
            flash_size_bytes: 512 * 1024,
            ram_origin: 0x2000_0000,
            ram_size_bytes: 96 * 1024,
            // The F401 vector table ends at 0x194. Start at the next 8-byte
            // boundary to satisfy current rust-lld section alignment.
            text_offset: 0x198,
            defmt_log: "info".to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderedStandaloneProject {
    pub root: PathBuf,
    pub manifest: PathBuf,
    pub main_source: PathBuf,
    pub memory_layout: PathBuf,
    pub cargo_config: PathBuf,
    pub embed_config: PathBuf,
}

/// Emit the first standalone project from one task package, one system init
/// package, a validated composition, and explicit system runtime selections.
///
/// The task/init packages remain source inputs rather than dependencies of the
/// generated target. Their supported normal dependencies are retained
/// conservatively; explicitly classified check-only dependencies have already
/// been removed during package discovery.
pub fn render_standalone_project(
    task_package: &TaskPackage,
    init_package: &InitPackage,
    composition: &ValidatedComposition<'_>,
    options: StandaloneProjectOptions<'_>,
) -> Result<RenderedStandaloneProject, RenderError> {
    validate_package_name(options.package_name)?;
    validate_target(options.target)?;

    let source = render_rtic_app_with_init(
        &task_package.sources,
        init_package,
        composition,
        options.app_target,
    )?;
    let dependencies = merge_dependencies(
        task_package
            .dependencies
            .iter()
            .chain(&init_package.package.dependencies)
            .chain(options.system_dependencies)
            .cloned(),
    )?;
    let manifest = render_manifest(options.package_name, &dependencies)?;
    let cargo_config = render_cargo_config(options.target);
    let memory_layout = render_memory_layout(options.target);
    let embed_config = render_embed_config(options.target);

    let root = options.output_dir.to_path_buf();
    let source_dir = root.join("src");
    let cargo_dir = root.join(".cargo");
    fs::create_dir_all(&source_dir)?;
    fs::create_dir_all(&cargo_dir)?;

    let manifest_path = root.join("Cargo.toml");
    let main_source = source_dir.join("main.rs");
    let memory_layout_path = root.join("memory.x");
    let cargo_config_path = cargo_dir.join("config.toml");
    let embed_config_path = root.join("Embed.toml");
    fs::write(&manifest_path, manifest)?;
    fs::write(&main_source, source)?;
    fs::write(&memory_layout_path, memory_layout)?;
    fs::write(&cargo_config_path, cargo_config)?;
    fs::write(&embed_config_path, embed_config)?;
    fs::write(root.join(".gitignore"), "/target/\n")?;

    Ok(RenderedStandaloneProject {
        root,
        manifest: manifest_path,
        main_source,
        memory_layout: memory_layout_path,
        cargo_config: cargo_config_path,
        embed_config: embed_config_path,
    })
}

fn render_manifest(
    package_name: &str,
    dependencies: &BTreeMap<String, MergedDependency>,
) -> Result<String, RenderError> {
    let mut manifest = format!(
        "[package]\nname = {}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\nbuild = false\n\n[[bin]]\nname = {}\npath = \"src/main.rs\"\ntest = false\nbench = false\n\n[dependencies]\n",
        toml_string(package_name),
        toml_string(package_name),
    );
    for dependency in dependencies.values() {
        if dependency.source != DependencySource::Registry(CRATES_IO_SOURCE.to_owned()) {
            return Err(invalid(format!(
                "standalone manifest emission supports only crates.io dependencies; `{}` uses {}",
                dependency.package, dependency.source
            )));
        }
        let features = dependency
            .features
            .iter()
            .map(|feature| toml_string(feature))
            .collect::<Vec<_>>()
            .join(", ");
        manifest.push_str(&format!(
            "{} = {{ version = {}, default-features = {}, features = [{}] }}\n",
            dependency.name,
            toml_string(&dependency.version_requirement),
            dependency.default_features,
            features,
        ));
    }
    manifest.push_str(
        "\n[profile.release]\ncodegen-units = 1\ndebug = 2\nlto = true\nopt-level = \"s\"\n\n[workspace]\n",
    );
    Ok(manifest)
}

pub(crate) fn render_cargo_config(target: &StandaloneTarget) -> String {
    let runner = format!("probe-rs run --chip {}", target.probe_rs_chip);
    format!(
        "[build]\ntarget = {}\n\n[target.{}]\nrunner = {}\nrustflags = [\n    \"-C\", \"link-arg=-L.\",\n    \"-C\", \"link-arg=-Tlink.x\",\n    \"-C\", \"link-arg=-Tdefmt.x\",\n]\n\n[env]\nDEFMT_LOG = {}\n",
        toml_string(&target.rust_target),
        target.rust_target,
        toml_string(&runner),
        toml_string(&target.defmt_log),
    )
}

pub(crate) fn render_memory_layout(target: &StandaloneTarget) -> String {
    format!(
        "MEMORY\n{{\n  FLASH : ORIGIN = {:#010X}, LENGTH = {}K\n  RAM   : ORIGIN = {:#010X}, LENGTH = {}K\n}}\n\n_stext = ORIGIN(FLASH) + {:#X};\n",
        target.flash_origin,
        target.flash_size_bytes / 1024,
        target.ram_origin,
        target.ram_size_bytes / 1024,
        target.text_offset,
    )
}

pub(crate) fn render_embed_config(target: &StandaloneTarget) -> String {
    format!(
        "[default.general]\nchip = {}\n\n[default.rtt]\nenabled = true\nup_channels = [\n    {{ channel = 0, mode = \"BlockIfFull\", format = \"Defmt\" }},\n]\n",
        toml_string(&target.probe_rs_chip),
    )
}

pub(crate) fn validate_package_name(package_name: &str) -> Result<(), RenderError> {
    if valid_identifier_like(package_name) {
        Ok(())
    } else {
        Err(invalid(format!(
            "invalid standalone package name `{package_name}`"
        )))
    }
}

pub(crate) fn validate_target(target: &StandaloneTarget) -> Result<(), RenderError> {
    for (role, value) in [
        ("Rust target", target.rust_target.as_str()),
        ("probe-rs chip", target.probe_rs_chip.as_str()),
        ("DEFMT_LOG filter", target.defmt_log.as_str()),
    ] {
        if !valid_identifier_like(value) {
            return Err(invalid(format!("invalid standalone {role} `{value}`")));
        }
    }
    for (role, size) in [
        ("flash", target.flash_size_bytes),
        ("RAM", target.ram_size_bytes),
    ] {
        if size == 0 || size % 1024 != 0 {
            return Err(invalid(format!(
                "standalone {role} size must be a nonzero whole number of KiB"
            )));
        }
    }
    if target.text_offset >= target.flash_size_bytes || target.text_offset % 4 != 0 {
        return Err(invalid(
            "standalone text offset must be word-aligned and inside flash",
        ));
    }
    Ok(())
}

fn valid_identifier_like(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}

pub(crate) fn toml_string(value: &str) -> String {
    let mut quoted = String::with_capacity(value.len() + 2);
    quoted.push('"');
    for character in value.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            character if character.is_control() => {
                use std::fmt::Write as _;
                write!(quoted, "\\u{:04X}", character as u32)
                    .expect("writing to a string cannot fail");
            }
            character => quoted.push(character),
        }
    }
    quoted.push('"');
    quoted
}

fn invalid(message: impl Into<String>) -> RenderError {
    RenderError::InvalidDeclaration(message.into())
}
