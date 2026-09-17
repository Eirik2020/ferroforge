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

/// `ferroforge` is how the generated manifests should depend on FerroForge: a
/// version requirement once it is published, or a path to a checkout, which is
/// the only thing that resolves until then.
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
        &main_rs(&backend, &task_crate),
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
    println!("\nnext: cd {shown} && ferroforge check");
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
         embedded-hal = \"1.0.0\"\n\n\
         [package.metadata.ferroforge]\n\
         check-only-dependencies = [\"ferroforge\"]\n\n\
         [workspace]\n"
    )
}

const TASK_LIB: &str = "\
//! Reusable task definitions. Nothing here names a chip or a HAL, so these
//! compile on their own and can be selected by any firmware.

#![no_std]

use embedded_hal::digital::StatefulOutputPin;

#[ferroforge::task(
    bounds = [led: StatefulOutputPin],
    local = [led, count: u32],
)]
pub fn toggle(cx: toggle::Context) {
    let _ = StatefulOutputPin::toggle(&mut *cx.local.led);
    *cx.local.count = cx.local.count.wrapping_add(1);
}
";

/// The composition is left deliberately small: a real `init` needs HAL calls
/// this scaffold cannot guess, so it names the one thing it can - the device -
/// and leaves the rest to the author.
fn main_rs(backend: &Backend, task_crate: &str) -> String {
    let device = &backend.chip.device;
    let crate_path = task_crate.replace('-', "_");
    format!(
        "//! Fill in `init` with your board's setup, then bind `toggle` to a pin.\n\
         //!\n\
         //! `ferroforge check` compiles this; `ferroforge run` flashes it.\n\n\
         #![no_std]\n\
         #![no_main]\n\n\
         use defmt_rtt as _;\n\
         use panic_probe as _;\n\n\
         ferroforge::app! {{\n\
         \x20   device = {device},\n\
         \x20   // Add `dispatchers = [..]` once you have software tasks, and\n\
         \x20   // `monotonic = Mono` once one of them needs a clock.\n\n\
         \x20   use {crate_path}::toggle;\n\n\
         \x20   #[shared]\n\
         \x20   struct Shared {{}}\n\n\
         \x20   #[local]\n\
         \x20   struct Local {{}}\n\n\
         \x20   #[init]\n\
         \x20   fn init(_cx: init::Context) -> (Shared, Local) {{\n\
         \x20       todo!(\"set up clocks and peripherals, then return the resources\")\n\
         \x20   }}\n\
         }}\n"
    )
}
