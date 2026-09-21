//! Project conventions.
//!
//! A FerroForge project is the nearest directory, from here upwards, that has a
//! `firmware/` in it. That is the whole convention: no marker file, nothing to
//! initialize, nothing to keep in sync. Cargo finds its workspace root the same
//! way, and G7 asks only that operations fail when a required input can no
//! longer be recognized - not that the layout be policed.

use std::{fmt, fs, io, path::Path, path::PathBuf};

use serde::Deserialize;

#[derive(Debug)]
pub enum Error {
    NotAProject { start: String },
    NoFirmware { root: String },
    UnknownFirmware { name: String, known: String },
    Ambiguous { known: String },
    NoChip { firmware: String },
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
    ferroforge: FerroForge,
}

#[derive(Debug, Default, Deserialize)]
struct FerroForge {
    chip: Option<String>,
    #[serde(rename = "defmt-log")]
    defmt_log: Option<String>,
    #[serde(rename = "defmt-location")]
    defmt_location: Option<bool>,
}

/// One application under `firmware/`.
pub struct Firmware {
    pub name: String,
    pub path: PathBuf,
}

impl Firmware {
    /// What this firmware wants in `DEFMT_LOG`: a level, or a filter such as
    /// `info,noisy_crate=off`.
    ///
    /// Declared rather than passed on the command line, because it is written
    /// into an emitted file. A flag would leave the file disagreeing with
    /// everything that records why, which is the drift the derived files exist
    /// to prevent.
    pub fn defmt_log(&self) -> Result<String, Error> {
        Ok(self
            .metadata()?
            .defmt_log
            .unwrap_or_else(|| "info".to_owned()))
    }

    /// Whether the probe prints the file and line each log came from.
    /// Declared rather than passed, for the same reason `defmt-log` is: it is
    /// written into an emitted file.
    ///
    /// On by default, as probe-rs has it. A firmware whose logs are read as a
    /// running commentary rather than debugged turns it off.
    pub fn defmt_location(&self) -> Result<bool, Error> {
        Ok(self.metadata()?.defmt_location.unwrap_or(true))
    }

    /// The chip this firmware declares. Not defaulted: guessing which chip a
    /// binary is for would produce a firmware that links and cannot run.
    pub fn chip(&self) -> Result<String, Error> {
        self.metadata()?.chip.ok_or_else(|| Error::NoChip {
            firmware: self.path.display().to_string(),
        })
    }

    fn metadata(&self) -> Result<FerroForge, Error> {
        let manifest = self.path.join("Cargo.toml");
        let display = manifest.display().to_string();
        let text = fs::read_to_string(&manifest).map_err(|source| Error::Read {
            path: display.clone(),
            source,
        })?;
        let parsed: Manifest = toml::from_str(&text).map_err(|error| Error::Parse {
            path: display,
            message: error.to_string(),
        })?;
        Ok(parsed.package.metadata.ferroforge)
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
