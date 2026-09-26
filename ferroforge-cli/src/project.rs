//! Project conventions.
//!
//! A FerroForge project is the nearest directory, from here upwards, that has a
//! `firmware/` in it. That is the whole convention: no marker file, nothing to
//! initialize, nothing to keep in sync. Cargo finds its workspace root the same
//! way, and G7 asks only that operations fail when a required input can no
//! longer be recognized - not that the layout be policed.

use std::{collections::BTreeMap, fmt, fs, io, path::Path, path::PathBuf};

use serde::Deserialize;

#[derive(Debug)]
pub enum Error {
    NotAProject { start: String },
    NoFirmware { root: String },
    UnknownFirmware { name: String, known: String },
    Ambiguous { known: String },
    NoChip { firmware: String },
    BadSetting { path: String, message: String },
    Read { path: String, source: io::Error },
    Parse { path: String, message: String },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAProject { start } => write!(
                formatter,
                "not inside a FerroForge project: no `firmware/` directory in \
                 {start} or any parent"
            ),
            Self::NoFirmware { root } => write!(
                formatter,
                "{root}/firmware contains no applications; `ferroforge new` \
                 creates one"
            ),
            Self::UnknownFirmware { name, known } => {
                write!(
                    formatter,
                    "no firmware named `{name}`. This project has: {known}"
                )
            }
            Self::Ambiguous { known } => write!(
                formatter,
                "this project has several applications, so name one or run from \
                 inside its directory: {known}"
            ),
            Self::NoChip { firmware } => write!(
                formatter,
                "{firmware} does not say which chip it is for. Add to its \
                 Cargo.toml:\n\n    [package.metadata.ferroforge]\n    \
                 chip = \"stm32f401re\""
            ),
            Self::BadSetting { path, message } => write!(
                formatter,
                "{path}: under [package.metadata.ferroforge], {message}"
            ),
            Self::Read { path, source } => write!(formatter, "cannot read {path}: {source}"),
            Self::Parse { path, message } => write!(formatter, "{path}: {message}"),
        }
    }
}

impl std::error::Error for Error {}

#[derive(Debug, Deserialize)]
struct Manifest {
    #[serde(default)]
    package: Package,
}

#[derive(Debug, Default, Deserialize)]
struct Package {
    #[serde(default)]
    metadata: Metadata,
}

#[derive(Debug, Default, Deserialize)]
struct Metadata {
    #[serde(default)]
    ferroforge: Settings,
}

/// What a firmware declares about itself.
///
/// Everything here ends up in a file the CLI writes, which is why none of it is
/// a command-line flag: a flag would leave the emitted file disagreeing with
/// everything that records why it says what it says.
///
/// Unknown keys are rejected. A misspelled key that is silently ignored is a
/// setting that appears to be applied and is not, and the emitted file gives no
/// hint which of the two happened.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "kebab-case", deny_unknown_fields)]
pub struct Settings {
    chip: Option<String>,
    defmt_log: Option<String>,
    defmt_location: Option<bool>,
    probe_command: Option<String>,
    #[serde(default)]
    probe_args: Vec<String>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    platform: BTreeMap<String, Source>,
    /// Declared in this same table by task crates, and documented with the
    /// dependencies rather than here. Accepted so that one table name means one
    /// thing: a firmware's `ferroforge` dependency is as check-only as a task
    /// crate's, and a firmware saying so must not be refused for it.
    #[serde(default, rename = "check-only-dependencies")]
    _check_only_dependencies: Vec<String>,
}

/// Where a platform crate comes from, when the backend's own selection will not
/// do.
///
/// The backend still chooses which crates a chip needs and which chip features
/// they carry; this says only where one of them is fetched from, which is a
/// property of a project's supply chain rather than of a chip family.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Source {
    pub version: Option<String>,
    pub git: Option<String>,
    pub rev: Option<String>,
    pub branch: Option<String>,
    pub tag: Option<String>,
    pub path: Option<String>,
    /// Features to add to the backend's own list for this crate. The backend's
    /// chip feature is always kept, so a source override cannot quietly build
    /// for the wrong chip.
    #[serde(default)]
    pub features: Vec<String>,
}

impl Source {
    /// The Cargo key-value pairs naming this source, in the order Cargo's own
    /// documentation writes them.
    pub fn keys(&self) -> Vec<(&'static str, String)> {
        let candidates = [
            ("version", self.version.as_ref()),
            ("path", self.path.as_ref()),
            ("git", self.git.as_ref()),
            ("branch", self.branch.as_ref()),
            ("tag", self.tag.as_ref()),
            ("rev", self.rev.as_ref()),
        ];
        candidates
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value.clone())))
            .collect()
    }

    /// Why this source cannot be used, in Cargo's terms. Checked here so a
    /// firmware is refused before any file is written, rather than by a Cargo
    /// error about a manifest FerroForge generated.
    fn fault(&self) -> Option<String> {
        if self.version.is_none() && self.git.is_none() && self.path.is_none() {
            return Some(
                "names no source; give it a `version`, a `git` URL or a `path`".to_owned(),
            );
        }
        let pointers = [("rev", &self.rev), ("branch", &self.branch), ("tag", &self.tag)];
        let named = pointers
            .iter()
            .filter(|(_, value)| value.is_some())
            .map(|(key, _)| *key)
            .collect::<Vec<_>>();
        if self.git.is_none() && !named.is_empty() {
            return Some(format!(
                "sets `{}` without a `git` URL; Cargo accepts those only on a git source",
                named.join("` and `"),
            ));
        }
        if named.len() > 1 {
            return Some(format!(
                "sets `{}` at once; a git source is pinned by exactly one of them",
                named.join("` and `"),
            ));
        }
        None
    }
}

/// One application under `firmware/`.
pub struct Firmware {
    pub name: String,
    pub path: PathBuf,
}

/// Arguments the CLI puts on the probe command line itself, and the setting
/// that owns each. A firmware repeating one of these would have the generated
/// file name the same thing twice, with nothing deciding which wins.
const OWNED_PROBE_ARGUMENTS: &[(&str, &str)] = &[
    ("--chip", "chip"),
    ("--no-location", "defmt-location"),
];

/// Environment variables the CLI writes, and the setting that owns each.
const OWNED_ENVIRONMENT: &[(&str, &str)] = &[("DEFMT_LOG", "defmt-log")];

/// The probe subcommands that make sense as a Cargo runner. `run` flashes and
/// runs; `attach` connects to what is already there.
const PROBE_COMMANDS: &[&str] = &["run", "attach"];

impl Settings {
    /// What this firmware wants in `DEFMT_LOG`: a level, or a filter such as
    /// `info,noisy_crate=off`.
    pub fn defmt_log(&self) -> &str {
        self.defmt_log.as_deref().unwrap_or("info")
    }

    /// Whether the probe prints the file and line each log came from.
    ///
    /// On by default, as probe-rs has it. A firmware whose logs are read as a
    /// running commentary rather than debugged turns it off.
    pub fn defmt_location(&self) -> bool {
        self.defmt_location.unwrap_or(true)
    }

    /// The probe subcommand the runner invokes. `run` by default, which is what
    /// `cargo run` means everywhere else.
    pub fn probe_command(&self) -> &str {
        self.probe_command.as_deref().unwrap_or("run")
    }

    /// Extra arguments for the probe, such as `--protocol swd`. Placed after the
    /// chip, so they read in the order a person would type them.
    pub fn probe_args(&self) -> &[String] {
        &self.probe_args
    }

    /// Extra environment variables for every build of this firmware.
    pub fn env(&self) -> &BTreeMap<String, String> {
        &self.env
    }

    /// Where a platform crate comes from, for the crates a firmware overrides.
    pub fn platform(&self) -> &BTreeMap<String, Source> {
        &self.platform
    }

    /// Why these settings cannot be used. Checked before anything is written,
    /// because a firmware that is refused must keep the files it had.
    fn fault(&self) -> Option<String> {
        if !PROBE_COMMANDS.contains(&self.probe_command()) {
            return Some(format!(
                "`probe-command` is `{}`; it must be one of: {}",
                self.probe_command(),
                PROBE_COMMANDS.join(", "),
            ));
        }
        for argument in &self.probe_args {
            if argument.trim().is_empty() {
                return Some("`probe-args` holds an empty argument".to_owned());
            }
            if let Some((_, owner)) = OWNED_PROBE_ARGUMENTS
                .iter()
                .find(|(owned, _)| argument == owned)
            {
                return Some(format!(
                    "`probe-args` names `{argument}`, which the generated runner \
                     already carries; set `{owner}` instead",
                ));
            }
        }
        for name in self.env.keys() {
            if let Some((_, owner)) = OWNED_ENVIRONMENT.iter().find(|(owned, _)| name == owned) {
                return Some(format!(
                    "`env` sets `{name}`, which the generated file already \
                     carries; set `{owner}` instead",
                ));
            }
        }
        for (crate_name, source) in &self.platform {
            if let Some(fault) = source.fault() {
                return Some(format!("`platform.{crate_name}` {fault}"));
            }
        }
        None
    }
}

impl Firmware {
    /// The chip this firmware declares. Not defaulted: guessing which chip a
    /// binary is for would produce a firmware that links and cannot run.
    pub fn chip(&self) -> Result<String, Error> {
        self.settings()?.chip.ok_or_else(|| Error::NoChip {
            firmware: self.path.display().to_string(),
        })
    }

    /// Everything this firmware declares, checked. Read from the manifest on
    /// every call rather than cached: the manifest is authored, and a stale copy
    /// would emit files for a firmware as it used to be.
    pub fn settings(&self) -> Result<Settings, Error> {
        let manifest = self.path.join("Cargo.toml");
        let display = manifest.display().to_string();
        let text = fs::read_to_string(&manifest).map_err(|source| Error::Read {
            path: display.clone(),
            source,
        })?;
        let parsed: Manifest = toml::from_str(&text).map_err(|error| Error::Parse {
            path: display.clone(),
            message: error.to_string(),
        })?;
        let settings = parsed.package.metadata.ferroforge;
        match settings.fault() {
            Some(message) => Err(Error::BadSetting {
                path: display,
                message,
            }),
            None => Ok(settings),
        }
    }
}

pub struct Project {
    pub root: PathBuf,
}

impl Project {
    /// The nearest enclosing project, starting at `start` and walking up.
    ///
    /// Canonical, because working out which firmware you are standing in means
    /// comparing paths - but not verbatim: every path derived from this ends up
    /// in a message, and `\\?\C:\...` is not something to print at anyone.
    pub fn containing(start: &Path) -> Result<Self, Error> {
        let start = start
            .canonicalize()
            .map(|path| plain(&path))
            .unwrap_or_else(|_| start.to_path_buf());
        for directory in start.ancestors() {
            if directory.join("firmware").is_dir() {
                return Ok(Self {
                    root: directory.to_path_buf(),
                });
            }
        }
        Err(Error::NotAProject {
            start: start.display().to_string(),
        })
    }

    /// Every application, in a stable order so listings and errors read the
    /// same way twice.
    pub fn firmwares(&self) -> Result<Vec<Firmware>, Error> {
        let directory = self.root.join("firmware");
        let display = directory.display().to_string();
        let mut found = Vec::new();
        for entry in fs::read_dir(&directory).map_err(|source| Error::Read {
            path: display,
            source,
        })? {
            let path = match entry {
                Ok(entry) => entry.path(),
                Err(_) => continue,
            };
            if !path.join("Cargo.toml").is_file() {
                continue;
            }
            let Some(name) = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
            else {
                continue;
            };
            found.push(Firmware { name, path });
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        if found.is_empty() {
            return Err(Error::NoFirmware {
                root: self.root.display().to_string(),
            });
        }
        Ok(found)
    }

    /// Which application a command applies to, resolved the way Cargo resolves
    /// a package: an explicit name wins, then where you are standing, then the
    /// only candidate. Anything else is ambiguous and says so.
    pub fn select(&self, named: Option<&str>, from: &Path) -> Result<Firmware, Error> {
        let mut candidates = self.firmwares()?;
        let names = || candidates_names(&self.firmwares().unwrap_or_default());

        if let Some(name) = named {
            let position = candidates.iter().position(|firmware| firmware.name == name);
            return match position {
                Some(index) => Ok(candidates.swap_remove(index)),
                None => Err(Error::UnknownFirmware {
                    name: name.to_owned(),
                    known: names(),
                }),
            };
        }

        let from = from
            .canonicalize()
            .map(|path| plain(&path))
            .unwrap_or_else(|_| from.to_path_buf());
        if let Some(index) = candidates.iter().position(|firmware| {
            let path = firmware
                .path
                .canonicalize()
                .map(|path| plain(&path))
                .unwrap_or_else(|_| firmware.path.clone());
            from.starts_with(&path)
        }) {
            return Ok(candidates.swap_remove(index));
        }

        if candidates.len() == 1 {
            return Ok(candidates.remove(0));
        }
        Err(Error::Ambiguous { known: names() })
    }
}

fn candidates_names(firmwares: &[Firmware]) -> String {
    firmwares
        .iter()
        .map(|firmware| firmware.name.clone())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Drop Windows' verbatim prefix. The path still resolves; it just reads like
/// one a person typed.
fn plain(path: &Path) -> PathBuf {
    const VERBATIM: &str = r#"\\?\"#;
    let text = path.display().to_string();
    match text.strip_prefix(VERBATIM) {
        Some(rest) => PathBuf::from(rest),
        None => path.to_path_buf(),
    }
}
