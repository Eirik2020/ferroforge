//! Chip-family data and the files derived from it.
//!
//! Per G2a a backend is build-time data, not a crate: nothing depends on it at
//! compile time, so it exists only to be read here. Per G4 it must not name
//! interrupts - the device's own enum is the only interrupt list.
//!
//! Backends ship with FerroForge rather than with a project, because a chip's
//! memory map is not a property of anyone's application. They are embedded at
//! compile time, not read from disk, so an installed binary carries its own
//! chip data and does not depend on where its source tree was.

use std::{collections::BTreeMap, fmt, fs, io, path::Path};

use serde::Deserialize;

/// Every chip FerroForge knows, keyed by the name a firmware declares.
///
/// Listed rather than globbed so that adding a file is a deliberate act; a test
/// asserts this matches `backends/` so the two cannot drift.
const BUILTIN: &[(&str, &str)] = &[
    (
        "stm32f401re",
        include_str!("../backends/stm32f4/stm32f401re.toml"),
    ),
    (
        "stm32f405rg",
        include_str!("../backends/stm32f4/stm32f405rg.toml"),
    ),
    (
        "stm32f411re",
        include_str!("../backends/stm32f4/stm32f411re.toml"),
    ),
    (
        "stm32h753zi",
        include_str!("../backends/stm32h7/stm32h753zi.toml"),
    ),
];

/// The chips this build knows, for error messages and listings.
pub fn known_chips() -> Vec<&'static str> {
    BUILTIN.iter().map(|(name, _)| *name).collect()
}

/// Every known chip with the facts a person picks one by: its HAL, target and
/// memory. Read from the same data a build uses, so it cannot disagree with it.
pub fn chip_table() -> Result<String, Error> {
    let mut rows = vec![[
        "CHIP".to_owned(),
        "HAL".to_owned(),
        "TARGET".to_owned(),
        "FLASH".to_owned(),
        "RAM".to_owned(),
    ]];
    for chip in known_chips() {
        let backend = Backend::for_chip(chip)?;
        let hal = backend
            .platform_dependencies
            .keys()
            .find(|name| name.ends_with("-hal"))
            .cloned()
            .unwrap_or_else(|| "-".to_owned());
        rows.push([
            chip.to_owned(),
            hal,
            backend.chip.rust_target.clone(),
            human_size(backend.memory.flash_size),
            human_size(backend.memory.ram_size),
        ]);
    }

    let mut widths = [0; 5];
    for row in &rows {
        for (width, cell) in widths.iter_mut().zip(row) {
            *width = (*width).max(cell.len());
        }
    }
    let mut table = String::new();
    for row in &rows {
        let line = row
            .iter()
            .zip(widths)
            .map(|(cell, width)| format!("{cell:width$}"))
            .collect::<Vec<_>>()
            .join("   ");
        table.push_str(line.trim_end());
        table.push('\n');
    }
    Ok(table)
}

/// Every chip feature any known backend enables on a platform crate, such as
/// `stm32f401`. A firmware enabling one of these itself is naming a chip outside
/// the generated block, where nothing keeps it in step.
fn chip_features() -> Vec<String> {
    let mut found = Vec::new();
    for (_, text) in BUILTIN {
        if let Ok(backend) = Backend::parse("a built-in backend", text) {
            for dependency in backend.platform_dependencies.values() {
                found.extend(dependency.features.iter().cloned());
            }
        }
    }
    found.sort();
    found.dedup();
    found
}

/// A chip feature a firmware enables by hand that the selected chip does not
/// want. The HAL rejects two chip features at once, but only from a build
/// script, which reports a panic rather than a cause.
fn conflicting_chip_feature(manifest: &str, wanted: &[String]) -> Option<(String, String)> {
    let owned = chip_features();
    let mut generated = false;
    for line in manifest.lines() {
        // The generated block is about to be replaced, and until it is it still
        // names the previous chip. Scanning it would make changing a firmware's
        // chip impossible - the one thing this is here to keep working.
        let trimmed = line.trim_start();
        if trimmed.starts_with(MARKER_BEGIN) {
            generated = true;
            continue;
        }
        if trimmed.starts_with(MARKER_END) {
            generated = false;
            continue;
        }
        if generated {
            continue;
        }

        let line = line.split('#').next().unwrap_or_default();
        let Some((name, rest)) = line.split_once('=') else {
            continue;
        };
        let name = name.trim();
        if name.is_empty() || !rest.contains("features") {
            continue;
        }
        for feature in &owned {
            if wanted.contains(feature) {
                continue;
            }
            if rest.contains(&format!("\"{feature}\"")) {
                return Some((name.to_owned(), feature.clone()));
            }
        }
    }
    None
}

/// The firmware manifest is authored, so the CLI owns only the lines between
/// these markers. Everything else in it - the task crates, the logging and panic
/// backends, the profile - belongs to the application.
const MARKER_BEGIN: &str = "# ferroforge:platform-dependencies";
const MARKER_END: &str = "# ferroforge:end";

#[derive(Debug)]
pub enum Error {
    Read {
        path: String,
        source: io::Error,
    },
    Parse {
        path: String,
        message: String,
    },
    Write {
        path: String,
        source: io::Error,
    },
    NamesInterrupts {
        path: String,
    },
    NoManifestMarkers {
        path: String,
    },
    UnknownChip {
        chip: String,
        known: String,
    },
    ConflictingChipFeature {
        path: String,
        dependency: String,
        feature: String,
        chip: String,
    },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Read { path, source } => write!(formatter, "cannot read {path}: {source}"),
            Self::Parse { path, message } => write!(formatter, "{path}: {message}"),
            Self::Write { path, source } => write!(formatter, "cannot write {path}: {source}"),
            Self::UnknownChip { chip, known } => write!(
                formatter,
                "no backend for chip `{chip}`. FerroForge ships: {known}"
            ),
            Self::ConflictingChipFeature {
                path,
                dependency,
                feature,
                chip,
            } => write!(
                formatter,
                "{path} enables `{feature}` on `{dependency}`, but this firmware \
                 is for {chip}. A chip feature outside the generated block is a \
                 chip named twice, and the HAL refuses two at once. Drop it: a \
                 task crate's own HAL dependency takes the chip from this \
                 firmware's platform crates, because Cargo features are additive."
            ),
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

/// `FLASH` and `RAM` are the pair `cortex-m-rt` requires and are named as it
/// expects. Anything else a part offers is an extra region: emitted so a
/// firmware can place sections in it, never used automatically, because which
/// memory suits which data is the application's decision.
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
    #[serde(default)]
    pub region: Vec<Region>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Region {
    pub name: String,
    pub origin: u64,
    pub size: u64,
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
    /// The backend for a chip a firmware named. Matching is case-insensitive so
    /// `STM32F401RE` and `stm32f401re` are the same chip.
    pub fn for_chip(chip: &str) -> Result<Self, Error> {
        let wanted = chip.to_ascii_lowercase();
        let (name, text) = BUILTIN
            .iter()
            .find(|(name, _)| *name == wanted)
            .ok_or_else(|| Error::UnknownChip {
                chip: chip.to_owned(),
                known: known_chips().join(", "),
            })?;
        Self::parse(&format!("the built-in backend for {name}"), text)
    }

    fn parse(origin: &str, text: &str) -> Result<Self, Error> {
        // Checked on the data, not the raw text, so a comment explaining the
        // rule does not trip it. Unknown keys are rejected by the parse itself;
        // this exists only to give the G4 reason rather than "unknown field".
        if names_interrupts(text) {
            return Err(Error::NamesInterrupts {
                path: origin.to_owned(),
            });
        }
        toml::from_str(text).map_err(|error| Error::Parse {
            path: origin.to_owned(),
            message: error.to_string(),
        })
    }

    /// Linker memory regions. Sizes are emitted in KiB when they divide evenly,
    /// because that is how a person reads a memory map.
    pub fn memory_x(&self) -> String {
        // Names are padded to the widest so the map reads as a column, which is
        // the point of writing it out rather than computing it.
        let width = self
            .memory
            .region
            .iter()
            .map(|region| region.name.len())
            .chain([5])
            .max()
            .unwrap_or(5);

        let mut regions = format!(
            "  {:width$} : ORIGIN = {:#010x}, LENGTH = {}\n  {:width$} : ORIGIN = {:#010x}, LENGTH = {}\n",
            "FLASH",
            self.memory.flash_origin,
            human_size(self.memory.flash_size),
            "RAM",
            self.memory.ram_origin,
            human_size(self.memory.ram_size),
        );
        for region in &self.memory.region {
            regions.push_str(&format!(
                "  {:width$} : ORIGIN = {:#010x}, LENGTH = {}\n",
                region.name,
                region.origin,
                human_size(region.size),
            ));
        }

        format!(
            "MEMORY\n{{\n{regions}}}\n\n_stext = ORIGIN(FLASH) + {:#x};\n",
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

        // Checked before rewriting, so the manifest is left as it was.
        let wanted = self
            .platform_dependencies
            .values()
            .flat_map(|dependency| dependency.features.iter().cloned())
            .collect::<Vec<_>>();
        if let Some((dependency, feature)) = conflicting_chip_feature(&text, &wanted) {
            return Err(Error::ConflictingChipFeature {
                path: display,
                dependency,
                feature,
                chip: self.chip.name.clone(),
            });
        }

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

#[cfg(test)]
mod tests {
    use super::*;

    const MINIMAL: &str = "[chip]\nname = \"X\"\nrust-target = \"t\"\nprobe-rs-chip = \"X\"\n\
                           device = \"x::pac\"\n\n[memory]\nflash-origin = 0\nflash-size = 1024\n\
                           ram-origin = 0\nram-size = 1024\ntext-offset = 0\n";

    /// Per G4 the device's own enum is the only interrupt list, so backend data
    /// naming one is refused with that reason rather than "unknown field".
    #[test]
    fn backend_data_may_not_name_interrupts() {
        let text = format!("{MINIMAL}\n[interrupts]\nTIM2 = 28\n");
        let error = Backend::parse("under test", &text).unwrap_err().to_string();
        assert!(error.contains("names interrupts"), "{error}");
        assert!(error.contains("G4"), "the error must say why: {error}");
    }

    /// The shipped backend documents that rule in a comment. Documenting a rule
    /// must not violate it.
    #[test]
    fn a_comment_about_interrupts_is_not_interrupt_data() {
        let backend = Backend::for_chip("stm32f401re").expect("the shipped backend must parse");
        assert!(backend.platform_dependencies.contains_key("stm32f4xx-hal"));
        // Logging and panic backends are the firmware's choice, not the chip's.
        assert!(!backend.platform_dependencies.contains_key("defmt-rtt"));
        assert!(!backend.platform_dependencies.contains_key("panic-probe"));
    }

    #[test]
    fn a_chip_is_matched_however_it_is_spelled() {
        assert!(Backend::for_chip("STM32F401RE").is_ok());
        assert!(Backend::for_chip("stm32f401re").is_ok());
    }

    /// An unknown chip must say what is available; a bare "not found" leaves an
    /// author guessing at spelling.
    #[test]
    fn an_unknown_chip_lists_what_is_available() {
        let error = Backend::for_chip("stm32f999zz").unwrap_err().to_string();
        assert!(error.contains("stm32f999zz"), "{error}");
        assert!(error.contains("stm32f401re"), "{error}");
    }

    /// The registry is hand-written, so this is what stops a backend file from
    /// existing in the tree while being invisible to every command.
    #[test]
    fn every_backend_file_is_registered() {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("backends");

        let mut found = Vec::new();
        let mut stack = vec![directory];
        while let Some(current) = stack.pop() {
            for entry in fs::read_dir(&current)
                .expect("backends/ must be readable")
                .flatten()
            {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                } else if path
                    .extension()
                    .is_some_and(|extension| extension == "toml")
                {
                    found.push(
                        path.file_stem()
                            .expect("a .toml file has a stem")
                            .to_string_lossy()
                            .into_owned(),
                    );
                }
            }
        }
        found.sort();

        let mut registered = known_chips();
        registered.sort_unstable();
        assert_eq!(
            found, registered,
            "backends/ and the built-in registry have drifted"
        );
    }
}

#[cfg(test)]
mod derivation {
    use super::*;

    fn variant(name: &str, target: &str, flash: u64, offset: u64) -> Backend {
        let text = format!(
            "[chip]\nname = \"{name}\"\nrust-target = \"{target}\"\n\
             probe-rs-chip = \"{name}\"\ndevice = \"x::pac\"\n\n\
             [memory]\nflash-origin = 0x08000000\nflash-size = {flash}\n\
             ram-origin = 0x20000000\nram-size = 98304\ntext-offset = {offset}\n"
        );
        Backend::parse("under test", &text).expect("the fixture must parse")
    }

    /// Every emitted file must be derived, so two chips must produce three
    /// different files. One that stays put is hardcoded here rather than read
    /// from the backend.
    #[test]
    fn a_different_chip_changes_every_emitted_file() {
        let first = variant("CHIP_A", "thumbv7em-none-eabihf", 524288, 0x198);
        let second = variant("CHIP_B", "thumbv7m-none-eabi", 262144, 0x1a0);

        assert_ne!(first.memory_x(), second.memory_x());
        assert_ne!(
            first.cargo_config("info"),
            second.cargo_config("info"),
            "the target triple and probe chip both come from the backend"
        );
        assert_ne!(first.embed_toml(), second.embed_toml());
        assert_ne!(
            first.manifest_platform_block(),
            second.manifest_platform_block(),
            "the block names its chip, so selecting another rewrites it"
        );
    }

    /// `--defmt-log` is the caller's, not the chip's.
    #[test]
    fn the_log_level_reaches_the_emitted_config() {
        let backend = variant("CHIP_A", "thumbv7em-none-eabihf", 524288, 0x198);
        assert!(
            backend
                .cargo_config("trace")
                .contains("DEFMT_LOG = \"trace\"")
        );
    }
}
