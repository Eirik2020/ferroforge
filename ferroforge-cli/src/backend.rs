//! Chip-family data and the files derived from it.
//!
//! Per G2a a backend is build-time data, not a crate: nothing depends on it at
//! compile time, so it exists only to be read here. Per G4 it must not name
//! interrupts - the device's own enum is the only interrupt list.

use std::{collections::BTreeMap, fmt, fs, io, path::Path};

use serde::Deserialize;

/// The firmware manifest is authored, so the CLI owns only the lines between
/// these markers. Everything else in it - the task crates, the logging and panic
/// backends, the profile - belongs to the application.
const MARKER_BEGIN: &str = "# ferroforge:platform-dependencies";
const MARKER_END: &str = "# ferroforge:end";

#[derive(Debug)]
pub enum Error {
    Read { path: String, source: io::Error },
    Parse { path: String, message: String },
    Write { path: String, source: io::Error },
    NamesInterrupts { path: String },
    NoManifestMarkers { path: String },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "cannot read {path}: {source}"),
            Self::Parse { path, message } => write!(formatter, "{path}: {message}"),
            Self::Write { path, source } => write!(formatter, "cannot write {path}: {source}"),
            Self::NoManifestMarkers { path } => write!(
                formatter,
                "{path} has no `{MARKER_BEGIN}` / `{MARKER_END}` block; add one \
                 inside [dependencies] so the platform crates for this chip are \
                 derived rather than maintained by hand"
            ),
            Self::NamesInterrupts { path } => write!(
                formatter,
                "{path} names interrupts; per G4 the device's own enum is the only \
                 interrupt list, so backend data must not restate one"
            ),
        }
    }
}

impl std::error::Error for Error {}

/// Unknown keys are rejected, which is also how interrupt data is caught: per
/// G4 there is no field for it, so naming any is an error rather than something
/// silently ignored.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Backend {
    pub chip: Chip,
    pub memory: Memory,
    #[serde(rename = "platform-dependencies", default)]
    pub platform_dependencies: BTreeMap<String, Dependency>,
}

#[derive(Debug, Deserialize)]
pub struct Chip {
    pub name: String,
    #[serde(rename = "rust-target")]
    pub rust_target: String,
    #[serde(rename = "probe-rs-chip")]
    pub probe_rs_chip: String,
    pub device: String,
}

#[derive(Debug, Deserialize)]
pub struct Memory {
    #[serde(rename = "flash-origin")]
    pub flash_origin: u64,
    #[serde(rename = "flash-size")]
    pub flash_size: u64,
    #[serde(rename = "ram-origin")]
    pub ram_origin: u64,
    #[serde(rename = "ram-size")]
    pub ram_size: u64,
    #[serde(rename = "text-offset")]
    pub text_offset: u64,
}

#[derive(Debug, Deserialize)]
pub struct Dependency {
    pub version: String,
    #[serde(rename = "default-features")]
    pub default_features: Option<bool>,
    #[serde(default)]
    pub features: Vec<String>,
}

impl Backend {
    pub fn load(path: &Path) -> Result<Self, Error> {
        let display = path.display().to_string();
        let text = fs::read_to_string(path).map_err(|source| Error::Read {
            path: display.clone(),
            source,
        })?;
        // Checked on the data, not the raw text, so a comment explaining the
        // rule does not trip it. Unknown keys are rejected by the parse itself;
        // this exists only to give the G4 reason rather than "unknown field".
        if names_interrupts(&text) {
            return Err(Error::NamesInterrupts { path: display });
        }
        toml::from_str(&text).map_err(|error| Error::Parse {
            path: display,
            message: error.to_string(),
        })
    }

    /// Linker memory regions. Sizes are emitted in KiB when they divide evenly,
    /// because that is how a person reads a memory map.
    pub fn memory_x(&self) -> String {
        format!(
            "MEMORY\n{{\n  FLASH : ORIGIN = {:#010x}, LENGTH = {}\n  RAM   : ORIGIN = {:#010x}, LENGTH = {}\n}}\n\n_stext = ORIGIN(FLASH) + {:#x};\n",
            self.memory.flash_origin,
            human_size(self.memory.flash_size),
            self.memory.ram_origin,
            human_size(self.memory.ram_size),
            self.memory.text_offset,
        )
    }

    pub fn cargo_config(&self, defmt_log: &str) -> String {
        format!(
            "[build]\ntarget = \"{target}\"\n\n[target.{target}]\nrunner = \"probe-rs run --chip {chip}\"\nrustflags = [\n    \"-C\", \"link-arg=-L.\",\n    \"-C\", \"link-arg=-Tlink.x\",\n    \"-C\", \"link-arg=-Tdefmt.x\",\n]\n\n[env]\nDEFMT_LOG = \"{defmt_log}\"\n",
            target = self.chip.rust_target,
            chip = self.chip.probe_rs_chip,
        )
    }

    pub fn embed_toml(&self) -> String {
        format!(
            "[default.general]\nchip = \"{}\"\n\n[default.rtt]\nenabled = true\n\n[default.gdb]\nenabled = false\n",
            self.chip.probe_rs_chip,
        )
    }

    /// The platform crates a firmware for this chip cannot build without, as
    /// manifest lines. The firmware still owns its logging and panic backends
    /// and its task crates.
    pub fn platform_dependency_lines(&self) -> String {
        let mut lines = String::new();
        for (name, dependency) in &self.platform_dependencies {
            let features = dependency
                .features
                .iter()
                .map(|feature| format!("\"{feature}\""))
                .collect::<Vec<_>>()
                .join(", ");
            lines.push_str(&format!(
                "{name} = {{ version = \"{}\", default-features = {}, features = [{features}] }}\n",
                dependency.version,
                dependency.default_features.unwrap_or(true),
            ));
        }
        lines
    }

    /// The marked manifest region, markers included. The chip is named on the
    /// opening line so selecting a different one visibly rewrites the block.
    pub fn manifest_platform_block(&self) -> String {
        format!(
            "{MARKER_BEGIN} for {} - generated, do not edit\n{}{MARKER_END}\n",
            self.chip.name,
            self.platform_dependency_lines(),
        )
    }

    /// Replace the marked region of a firmware's manifest. The manifest is not
    /// created or otherwise rewritten: absent markers are an error, because
    /// guessing where platform crates belong in an authored file is worse than
    /// asking for them.
    fn sync_manifest(&self, manifest: &Path) -> Result<(), Error> {
        let display = manifest.display().to_string();
        let text = fs::read_to_string(manifest).map_err(|source| Error::Read {
            path: display.clone(),
            source,
        })?;

        let lines = text.lines().collect::<Vec<_>>();
        let begin = lines
            .iter()
            .position(|line| line.trim_start().starts_with(MARKER_BEGIN))
            .ok_or_else(|| Error::NoManifestMarkers {
                path: display.clone(),
            })?;
        let end = lines[begin..]
            .iter()
            .position(|line| line.trim_start().starts_with(MARKER_END))
            .map(|offset| begin + offset)
            .ok_or_else(|| Error::NoManifestMarkers {
                path: display.clone(),
            })?;

        let mut updated = lines[..begin].join("\n");
        updated.push('\n');
        updated.push_str(&self.manifest_platform_block());
        if end + 1 < lines.len() {
            updated.push_str(&lines[end + 1..].join("\n"));
            updated.push('\n');
        }

        fs::write(manifest, updated).map_err(|source| Error::Write {
            path: display,
            source,
        })
    }

    /// Write every file derived from this chip into a firmware directory.
    pub fn emit(&self, firmware: &Path, defmt_log: &str) -> Result<Vec<String>, Error> {
        let cargo_dir = firmware.join(".cargo");
        fs::create_dir_all(&cargo_dir).map_err(|source| Error::Write {
            path: cargo_dir.display().to_string(),
            source,
        })?;
        let files = [
            (firmware.join("memory.x"), self.memory_x()),
            (cargo_dir.join("config.toml"), self.cargo_config(defmt_log)),
            (firmware.join("Embed.toml"), self.embed_toml()),
        ];
        let mut written = Vec::new();
        for (path, contents) in files {
            fs::write(&path, contents).map_err(|source| Error::Write {
                path: path.display().to_string(),
                source,
            })?;
            written.push(path.display().to_string());
        }

        let manifest = firmware.join("Cargo.toml");
        self.sync_manifest(&manifest)?;
        written.push(format!("{} (platform dependencies)", manifest.display()));

        Ok(written)
    }
}

/// True when a key or table names interrupts. Comments are stripped first, so
/// documenting the rule does not violate it.
fn names_interrupts(text: &str) -> bool {
    text.lines()
        .map(|line| line.split('#').next().unwrap_or_default())
        .any(|line| {
            let line = line.trim().to_ascii_lowercase();
            line.starts_with("[interrupt") || line.starts_with("interrupt")
        })
}

fn human_size(bytes: u64) -> String {
    if bytes.is_multiple_of(1024) {
        format!("{}K", bytes / 1024)
    } else {
        bytes.to_string()
    }
}
