//! `ferroforge new`: a project that builds before you have written anything.
//!
//! The layout it writes is the convention the rest of the CLI recognizes -
//! `firmware/` holding applications, `tasks/` holding reusable definitions -
//! and nothing more. There is no marker file to create, so a project made by
//! hand is indistinguishable from one made here.

use std::{fmt, fs, io, path::Path};

use crate::backend::{self, Backend};

#[derive(Debug)]
pub enum Error {
    Exists { path: String },
    UnknownChip { chip: String, known: String },
    NoStarter { chip: String },
    Backend(backend::Error),
    Write { path: String, source: io::Error },
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Exists { path } => {
                write!(formatter, "{path} already exists")
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

/// `ferroforge` is how the generated manifests depend on FerroForge: the
/// published version requirement, or a path to a checkout.
pub fn create(root: &Path, chip: &str, ferroforge: &str) -> Result<(), Error> {
    if root.exists() {
        return Err(Error::Exists {
            path: root.display().to_string(),
        });
    }
    // Fail before writing anything if the chip is unknown, rather than leaving
    // half a project behind.
    let backend = Backend::for_chip(chip).map_err(|error| match error {
        backend::Error::UnknownChip { chip, known } => Error::UnknownChip { chip, known },
        other => Error::Backend(other),
    })?;
    let starter = starter(&backend).ok_or_else(|| Error::NoStarter {
        chip: chip.to_owned(),
    })?;

    let name = root
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "firmware".to_owned());
    let firmware = root.join("firmware").join(&name);
    let task_crate = format!("{name}-tasks");

    write(
        &firmware.join("Cargo.toml"),
        &firmware_manifest(&name, chip, &task_crate, ferroforge),
    )?;
    write(
        &firmware.join("src/main.rs"),
        &main_rs(&backend, starter, &task_crate),
    )?;
    let tasks = root.join("tasks").join(&task_crate);
    write(
        &tasks.join("Cargo.toml"),
        &task_manifest(&task_crate, ferroforge),
    )?;
    write(&tasks.join("src/lib.rs"), TASK_LIB)?;
    write(&root.join(".gitignore"), "target/\n")?;

    // Everything the chip implies, written by the same code a later `sync`
    // uses, so a new project is already in the state `sync` would leave it.
    let written = backend.emit(&firmware, "info").map_err(Error::Backend)?;

    let shown = root.display().to_string().replace('\\', "/");
    println!("created {shown}");
    println!("  firmware/{name}/ for {}", backend.chip.name);
    println!("  tasks/{task_crate}/");
    println!(
        "  {} files derived from the chip, already written",
        written.len()
    );
    println!("\nnext: cd {shown} && ferroforge run");
    Ok(())
}

fn firmware_manifest(name: &str, chip: &str, task_crate: &str, ferroforge: &str) -> String {
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

use fugit::ExtU32 as _;

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
        Mono::delay(CONFIG.PERIOD_MS.millis()).await;
    }
}
";

/// What a new firmware needs that depends on its HAL: the imports, and the
/// clock setup that ends by starting the monotonic.
///
/// Keyed by HAL rather than by chip because that is where the code differs -
/// every STM32F4 starts the same way - and each is copied from a firmware that
/// has run on hardware. A chip whose HAL is missing here is refused by `new`,
/// and a test holds every built-in chip to having one.
struct Starter {
    hal: &'static str,
    imports: &'static str,
    clocks: &'static str,
}

const STARTERS: &[Starter] = &[
    Starter {
        hal: "stm32f4xx-hal",
        imports: "use stm32f4xx_hal::{prelude::*, rcc::Config};",
        clocks: "\
        // The internal oscillator, because it is on every board. Switch to the
        // crystal and raise `sysclk` once you know the board.
        let rcc = cx.device.RCC.freeze(Config::hsi());
        Mono::start(cx.core.SYST, rcc.clocks.sysclk().raw());",
    },
    Starter {
        hal: "stm32h7xx-hal",
        imports: "use stm32h7xx_hal::prelude::*;",
        clocks: "\
        // An H7 sets its core voltage before its clocks. The defaults run from
        // the internal oscillator, which is on every board.
        let pwr = cx.device.PWR.constrain();
        let pwrcfg = pwr.freeze();
        let rcc = cx.device.RCC.constrain();
        let ccdr = rcc.freeze(pwrcfg, &cx.device.SYSCFG);
        Mono::start(cx.core.SYST, ccdr.clocks.sys_ck().raw());",
    },
];

fn starter(backend: &Backend) -> Option<&'static Starter> {
    STARTERS
        .iter()
        .find(|starter| backend.platform_dependencies.contains_key(starter.hal))
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
         use panic_probe as _;\n\
         use rtic_monotonics::systick::prelude::*;\n\n\
         systick_monotonic!(Mono, 1000);\n\n\
         ferroforge::app! {{\n\
         \x20   device = {device},\n\
         \x20   // Software tasks run from interrupts the application does not\n\
         \x20   // otherwise use. Pick another if this one becomes a peripheral's.\n\
         \x20   dispatchers = [SPI1],\n\
         \x20   monotonic = Mono,\n\n\
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
