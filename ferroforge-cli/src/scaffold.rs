//! `ferroforge new` and `ferroforge add`: firmware that builds before you have
//! written anything.
//!
//! The layout it writes is the convention the rest of the CLI recognizes -
//! `firmware/` holding applications, `tasks/` holding reusable definitions -
//! and nothing more. There is no marker file to create, so a project made by
//! hand is indistinguishable from one made here.

use std::{fmt, fs, io, path::Path};

use crate::{
    backend::{self, Backend},
    project::{self, Firmware},
};

#[derive(Debug)]
pub enum Error {
    Exists { path: String },
    InvalidName { name: String, reason: &'static str },
    UnknownChip { chip: String, known: String },
    NoStarter { chip: String },
    Backend(backend::Error),
    Project(project::Error),
    Write { path: String, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists { path } => {
                write!(formatter, "{path} already exists")
            }
            Self::InvalidName { name, reason } => {
                write!(formatter, "`{name}` cannot name a firmware: {reason}")
            }
            Self::UnknownChip { chip, known } => write!(
                formatter,
                "no backend for chip `{chip}`. FerroForge ships: {known}"
            ),
            Self::NoStarter { chip } => write!(
                formatter,
                "FerroForge knows `{chip}` but has no starting firmware for its HAL"
            ),
            Self::Backend(error) => write!(formatter, "{error}"),
            Self::Project(error) => write!(formatter, "{error}"),
            Self::Write { path, source } => write!(formatter, "cannot write {path}: {source}"),
        }
    }
}

impl std::error::Error for Error {}

fn write(path: &Path, contents: &str) -> Result<(), Error> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|source| Error::Write {
            path: parent.display().to_string(),
            source,
        })?;
    }
    fs::write(path, contents).map_err(|source| Error::Write {
        path: path.display().to_string(),
        source,
    })
}

/// The backend for a chip and the starting firmware for its HAL. Both are
/// needed before anything is written, so a chip that cannot be started is
/// refused up front rather than leaving half a firmware behind.
fn resolve(chip: &str) -> Result<(Backend, &'static Starter), Error> {
    let backend = Backend::for_chip(chip).map_err(|error| match error {
        backend::Error::UnknownChip { chip, known } => Error::UnknownChip { chip, known },
        other => Error::Backend(other),
    })?;
    let starter = starter(&backend).ok_or_else(|| Error::NoStarter {
        chip: chip.to_owned(),
    })?;
    Ok((backend, starter))
}

/// The task crate `new` writes and every firmware it or `add` writes selects
/// from. A fixed name, not the project's: `add` has to find it again, and a
/// project directory can be renamed.
const TASK_CRATE: &str = "heartbeat";

/// `ferroforge` is how the generated manifests depend on FerroForge: the
/// published version requirement, or a path to a checkout.
pub fn create(root: &Path, chip: &str, ferroforge: &str) -> Result<(), Error> {
    if root.exists() {
        return Err(Error::Exists {
            path: root.display().to_string(),
        });
    }
    let (backend, starter) = resolve(chip)?;

    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "firmware".to_owned());
    validate_name(&name)?;
    let task_crate = TASK_CRATE;

    let derived = write_firmware(
        &root.join("firmware").join(&name),
        &name,
        chip,
        &backend,
        starter,
        task_crate,
        ferroforge,
    )?;
    write_task_crate(root, task_crate, ferroforge)?;
    write(&root.join(".gitignore"), "target/\n")?;

    let shown = root.display().to_string().replace('\\', "/");
    println!("created {shown}");
    println!("  firmware/{name}/ for {}", backend.chip.name);
    println!("  tasks/{task_crate}/");
    println!("  {derived} files derived from the chip, already written");
    println!("\nnext: cd {shown} && ferroforge run");
    Ok(())
}

/// `ferroforge add`: another firmware in a project that already exists.
///
/// It selects `heartbeat` from the task crate `new` wrote, and writes no tasks
/// of its own: the task crate is authored code by now, and a firmware reusing
/// a definition is the point. If that crate has since been removed, the new
/// firmware fails to build in Cargo, on the dependency it names.
///
/// `ferroforge` is how its manifest depends on FerroForge. Without one it is
/// taken from a firmware already in the project, so a project made against a
/// checkout stays on that checkout.
pub fn add(root: &Path, name: &str, chip: &str, ferroforge: Option<&str>) -> Result<(), Error> {
    validate_name(name)?;
    let firmware = root.join("firmware").join(name);
    if firmware.exists() {
        return Err(Error::Exists {
            path: format!("firmware/{name}"),
        });
    }
    let (backend, starter) = resolve(chip)?;

    let ferroforge = match ferroforge {
        Some(dependency) => dependency.to_owned(),
        None => inherited_dependency(root).unwrap_or_else(|| PUBLISHED.to_owned()),
    };
    let task_crate = TASK_CRATE;
    let derived = write_firmware(
        &firmware,
        name,
        chip,
        &backend,
        starter,
        task_crate,
        &ferroforge,
    )?;

    println!("added firmware/{name}/ for {}", backend.chip.name);
    println!("  selects `heartbeat` from tasks/{task_crate}/");
    println!("  {derived} files derived from the chip, already written");
    println!("\nnext: ferroforge run {name}");
    Ok(())
}

/// The published crate, as a manifest value.
pub const PUBLISHED: &str = "\"0.4\"";

/// The `ferroforge = ...` value of the first firmware in the project that has
/// one. Firmwares all sit at `firmware/<name>/`, so a relative path means the
/// same thing copied into a sibling.
fn inherited_dependency(root: &Path) -> Option<String> {
    let mut manifests = fs::read_dir(root.join("firmware"))
        .ok()?
        .flatten()
        .map(|entry| entry.path().join("Cargo.toml"))
        .collect::<Vec<_>>();
    manifests.sort();
    manifests.iter().find_map(|manifest| {
        let text = fs::read_to_string(manifest).ok()?;
        let mut in_dependencies = false;
        text.lines().find_map(|line| {
            let line = line.trim();
            if line.starts_with('[') {
                in_dependencies = line == "[dependencies]";
                return None;
            }
            let (key, value) = line.split_once('=')?;
            (in_dependencies && key.trim() == "ferroforge").then(|| value.trim().to_owned())
        })
    })
}

/// The name becomes a directory, a Cargo package and a binary, so it has to be
/// all three. Cargo would refuse a bad one later, but from inside a build and
/// after the directory had been written.
fn validate_name(name: &str) -> Result<(), Error> {
    let refuse = |reason| {
        Err(Error::InvalidName {
            name: name.to_owned(),
            reason,
        })
    };
    let Some(first) = name.chars().next() else {
        return refuse("it is empty");
    };
    if !first.is_ascii_alphabetic() {
        return refuse("it must start with a letter");
    }
    if !name
        .chars()
        .all(|character| character.is_ascii_alphanumeric() || matches!(character, '-' | '_'))
    {
        return refuse("use letters, digits, `-` and `_` only");
    }
    if name == TASK_CRATE {
        return refuse("it is the task crate's name, and a package cannot depend on its own name");
    }
    Ok(())
}

/// One firmware: its manifest, its source, and everything its chip implies.
/// Returns how many files came from the chip.
fn write_firmware(
    firmware: &Path,
    name: &str,
    chip: &str,
    backend: &Backend,
    starter: &Starter,
    task_crate: &str,
    ferroforge: &str,
) -> Result<usize, Error> {
    write(
        &firmware.join("Cargo.toml"),
        &firmware_manifest(name, chip, backend, task_crate, ferroforge),
    )?;
    write(
        &firmware.join("src/main.rs"),
        &main_rs(backend, starter, task_crate),
    )?;
    // Written by the same code a later `sync` uses, and from the manifest just
    // written rather than from repeated defaults, so a new firmware is already
    // in the state `sync` would leave it.
    let settings = Firmware {
        name: name.to_owned(),
        path: firmware.to_owned(),
    }
    .settings()
    .map_err(Error::Project)?;
    let written = backend.emit(firmware, &settings).map_err(Error::Backend)?;
    Ok(written.len())
}

fn write_task_crate(root: &Path, task_crate: &str, ferroforge: &str) -> Result<(), Error> {
    let tasks = root.join("tasks").join(task_crate);
    write(
        &tasks.join("Cargo.toml"),
        &task_manifest(task_crate, ferroforge),
    )?;
    write(&tasks.join("src/lib.rs"), TASK_LIB)
}

/// Platform crates a backend selects that are not on crates.io yet, with the
/// exact revision a new firmware fetches each from. The backend names them by
/// version, as every platform crate is named; until that version is published
/// the firmware says where to fetch it, through the setting that already owns
/// a crate's source. The firmware owns the pin once written, so `sync` keeps
/// it. Empty this once the crates are published.
///
/// FerroHAL's revision is the one `ferroforge-stm32` pins, so a firmware that
/// also selects that crate's groups resolves one FerroHAL rather than two.
const UNPUBLISHED: &[(&str, &str, &str)] = &[(
    "ferrohal-stm32",
    "https://github.com/Eirik2020/ferrohal-stm32",
    "d9ddc472eae17a335dd6fdd217af3c4dc4050d1b",
)];

/// A `[package.metadata.ferroforge.platform.<crate>]` table for each crate the
/// backend selects from [`UNPUBLISHED`].
fn unpublished_sources(backend: &Backend) -> String {
    UNPUBLISHED
        .iter()
        .filter(|(name, _, _)| backend.platform_dependencies.contains_key(*name))
        .map(|(name, git, rev)| {
            format!(
                "# Where {name} comes from until it is on crates.io.\n\
                 [package.metadata.ferroforge.platform.{name}]\n\
                 git = \"{git}\"\n\
                 rev = \"{rev}\"\n\n"
            )
        })
        .collect()
}

fn firmware_manifest(
    name: &str,
    chip: &str,
    backend: &Backend,
    task_crate: &str,
    ferroforge: &str,
) -> String {
    let sources = unpublished_sources(backend);
    format!(
        "[package]\n\
         name = \"{name}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\
         build = false\n\n\
         [package.metadata.ferroforge]\n\
         # The chip this firmware is built for. Everything the chip implies is\n\
         # derived from this, and nothing else records it.\n\
         chip = \"{chip}\"\n\n\
         {sources}\
         [[bin]]\n\
         name = \"{name}\"\n\
         path = \"src/main.rs\"\n\
         test = false\n\
         bench = false\n\n\
         [dependencies]\n\
         ferroforge = {ferroforge}\n\
         {task_crate} = {{ path = \"../../tasks/{task_crate}\" }}\n\n\
         # This application's own choices: what it logs with and how it panics.\n\
         defmt = \"1.0.1\"\n\
         defmt-rtt = \"1.3.0\"\n\
         panic-probe = {{ version = \"1.0.0\", features = [\"print-defmt\"] }}\n\n\
         # ferroforge:platform-dependencies\n\
         # ferroforge:end\n\n\
         [profile.release]\n\
         codegen-units = 1\n\
         debug = 2\n\
         lto = true\n\
         opt-level = \"s\"\n\n\
         [workspace]\n"
    )
}

fn task_manifest(task_crate: &str, ferroforge: &str) -> String {
    format!(
        "[package]\n\
         name = \"{task_crate}\"\n\
         version = \"0.1.0\"\n\
         edition = \"2024\"\n\
         publish = false\n\n\
         [lib]\n\
         test = false\n\
         bench = false\n\n\
         [dependencies]\n\
         ferroforge = {ferroforge}\n\
         defmt = \"1.0.1\"\n\
         fugit = \"0.3.9\"\n\
         rtic = {{ version = \"2.3.1\", default-features = false, features = [\"thumbv7-backend\"] }}\n\
         rtic-monotonics = {{ version = \"2.2.1\", default-features = false, features = [\"cortex-m-systick\"] }}\n\n\
         [package.metadata.ferroforge]\n\
         check-only-dependencies = [\"ferroforge\"]\n\n\
         [workspace]\n"
    )
}

const TASK_LIB: &str = "\
//! Reusable task definitions. Nothing here names a chip or a HAL, so these
//! compile on their own and any firmware can select them.

#![no_std]

use fugit::ExtU64 as _;

/// Logs a count at a fixed period, forever. It needs no pins, so it runs on
/// any board, and `ferroforge run` shows what it prints.
#[ferroforge::task(
    local = [count: u32],
    config = [period_ms: u32],
    monotonic = Mono,
)]
pub async fn heartbeat(cx: heartbeat::Context) -> ! {
    loop {
        *cx.local.count = cx.local.count.wrapping_add(1);
        defmt::info!(\"heartbeat {=u32}\", *cx.local.count);
        Mono::delay(u64::from(CONFIG::PERIOD_MS).millis()).await;
    }
}
";

/// What a new firmware needs that depends on its HAL and chip family: the
/// imports, and the clock setup that ends by starting the monotonic.
///
/// Keyed by HAL and family because that is where the code differs - FerroHAL
/// opens every STM32F4 the same way, and every STM32H7 another way - and each
/// starts from the reset clock tree, which is on every board. A chip with no
/// starter here is refused by `new`, and a test holds every built-in chip to
/// having one.
struct Starter {
    hal: &'static str,
    /// The start of the chip names this applies to, as the backend spells them.
    family: &'static str,
    imports: &'static str,
    clocks: &'static str,
}

const STARTERS: &[Starter] = &[
    Starter {
        hal: "ferrohal-stm32",
        family: "STM32F4",
        imports: "use ferrohal_stm32::rcc::Config;",
        clocks: "        // The reset clock tree: 16 MHz from the internal oscillator, which is
        // on every board. Switch to the crystal and raise the clock once you
        // know the board.
        let Ok(rcc) = cx.device.RCC.freeze(&Config::hsi()) else {
            defmt::panic!(\"the clock configuration was refused\");
        };
        Mono::start(cx.core.SYST, rcc.clocks().cpu.0);",
    },
    Starter {
        hal: "ferrohal-stm32",
        family: "STM32H7",
        imports: "use ferrohal_stm32::rcc::{AhbPrescaler, ApbPrescaler, Config, Supply, Sysclk, VoltageScale};",
        clocks: "        // The reset clock tree: 64 MHz from the internal oscillator, which is
        // on every board, at the reset voltage scale. Raise the scale before
        // the clock once you know the board.
        let config = Config {
            hse: None,
            pll1: None,
            sys: Sysclk::Hsi,
            cpu_pre: AhbPrescaler::Div1,
            ahb_pre: AhbPrescaler::Div1,
            apb1_pre: ApbPrescaler::Div1,
            apb2_pre: ApbPrescaler::Div1,
            apb3_pre: ApbPrescaler::Div1,
            apb4_pre: ApbPrescaler::Div1,
            supply: Supply::Ldo,
            voltage_scale: VoltageScale::Scale3,
        };
        let Ok(rcc) = cx.device.RCC.freeze(&config) else {
            defmt::panic!(\"the clock configuration was refused\");
        };
        Mono::start(cx.core.SYST, rcc.clocks().cpu.0);",
    },
];

fn starter(backend: &Backend) -> Option<&'static Starter> {
    STARTERS.iter().find(|starter| {
        backend.platform_dependencies.contains_key(starter.hal)
            && backend.chip.name.starts_with(starter.family)
    })
}

/// A firmware that runs as soon as it is flashed: one task, selected from the
/// project's own task crate and declared the way every other one will be.
fn main_rs(backend: &Backend, starter: &Starter, task_crate: &str) -> String {
    let device = &backend.chip.device;
    let crate_path = task_crate.replace('-', "_");
    let Starter {
        imports, clocks, ..
    } = starter;
    format!(
        "//! One task from `tasks/{task_crate}`, selected and running.\n\
         //!\n\
         //! `ferroforge run` flashes this and shows the heartbeat over RTT. Grow it\n\
         //! from here: resources in `Shared` and `Local`, peripherals in `init`,\n\
         //! and one `#[task(from = ..)]` declaration per task instance.\n\n\
         #![no_std]\n\
         #![no_main]\n\n\
         use defmt_rtt as _;\n\
         use panic_probe as _;\n\n\
         ferroforge::app! {{\n\
         \x20   device = {device},\n\
         \x20   // Software tasks run from interrupts the application does not\n\
         \x20   // otherwise use. Pick another if this one becomes a peripheral's.\n\
         \x20   dispatchers = [SPI1],\n\n\
         \x20   use rtic_monotonics::systick::prelude::*;\n\n\
         \x20   // The clock tasks are handed, at any rate. A hardware timer gives\n\
         \x20   // finer timestamps; see `monotonic-timer` in the book.\n\
         \x20   systick_monotonic!(Mono, 1000);\n\n\
         \x20   use {crate_path}::heartbeat;\n\
         \x20   {imports}\n\n\
         \x20   #[shared]\n\
         \x20   struct Shared {{}}\n\n\
         \x20   #[local]\n\
         \x20   struct Local {{\n\
         \x20       heartbeat_count: u32,\n\
         \x20   }}\n\n\
         \x20   #[init]\n\
         \x20   fn init(cx: init::Context) -> (Shared, Local) {{\n\
         {clocks}\n\n\
         \x20       status::spawn().unwrap();\n\
         \x20       (Shared {{}}, Local {{ heartbeat_count: 0 }})\n\
         \x20   }}\n\n\
         \x20   // `heartbeat` is the definition; `status` is this firmware's instance\n\
         \x20   // of it, with its own resource and period.\n\
         \x20   #[task(\n\
         \x20       from = heartbeat,\n\
         \x20       priority = 1,\n\
         \x20       local = [count = heartbeat_count],\n\
         \x20       config = [period_ms: u32 = 1000],\n\
         \x20   )]\n\
         \x20   async fn status(cx: status::Context) -> !;\n\
         }}\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A chip `new` accepts but cannot start would only fail once someone
    /// tried it, so every built-in chip is held to having a starter here.
    #[test]
    fn every_built_in_chip_has_a_starting_firmware() {
        for chip in backend::known_chips() {
            let backend = Backend::for_chip(chip).expect("a built-in chip loads");
            assert!(
                starter(&backend).is_some(),
                "`{chip}` has no starter for any of its platform crates"
            );
        }
    }
}
