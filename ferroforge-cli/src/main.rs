//! The FerroForge CLI.
//!
//! The verbs are Cargo's, because a firmware crate is an ordinary Cargo package
//! and pretending otherwise would only add vocabulary. `check`, `build` and
//! `run` refresh what the chip implies and then hand over to Cargo; `sync` is
//! the one verb Cargo has no analogue for, and does only the refreshing.

mod all;
mod backend;
mod drift;
mod project;
mod scaffold;

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use backend::Backend;
use project::{Firmware, Project};

const USAGE: &str = "\
ferroforge - compose reusable RTIC tasks into firmware

USAGE:
    ferroforge new <path> [--chip <name>] [--ferroforge <path>]
        Create a project: one firmware running one task, the task crate it
        selects from, and the files its chip implies. `--chip` defaults to
        stm32f401re; `ferroforge chips` lists the rest. `--ferroforge` depends
        on a local checkout instead of the published crate.

    ferroforge add <name> --chip <name> [--ferroforge <path>]
        Add a firmware to the project you are in, selecting `heartbeat` from
        the task crate `new` wrote. No tasks are created. It depends on
        FerroForge the way the project's other firmware does, unless
        `--ferroforge` says otherwise.

    ferroforge sync [<firmware> | --all]
        Refresh what the chip implies - memory.x, .cargo/config.toml,
        Embed.toml, and the platform crates in Cargo.toml. Nothing else in the
        manifest is touched.

    ferroforge check [<firmware> | --all] [-- <cargo args>]
    ferroforge build [<firmware> | --all] [-- <cargo args>]
    ferroforge run   [<firmware>] [-- <cargo args>]
        Sync, then the matching cargo command in that firmware. `build` and
        `run` are release builds; `run` flashes via the configured probe.

        `--all` does it for every firmware in the project, one line each: ok,
        warn or FAILED, with Cargo's output shown under a warn or a failure.
        A failure does not stop the rest.

    ferroforge drift
        Compare regions of code copied between firmwares, marked in each copy
        with `// ferroforge:begin <name>` and `// ferroforge:end <name>`, and
        report any that differ. For code that must be duplicated because it
        cannot be a reusable task. Indentation, blank lines and `//` comments
        are ignored; nested regions are compared on their own too.

    ferroforge chips [--names]
        The chips this build of FerroForge knows, with their HAL, target and
        memory. `--names` prints the names alone, one per line.

A firmware is named, or inferred from the directory you are in, or is the only
one in the project. The project is the nearest parent holding a `firmware/`.
";

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(code) => code,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

/// Everything after `--` goes to Cargo untouched.
fn split_forwarded(arguments: &[String]) -> (&[String], &[String]) {
    match arguments.iter().position(|argument| argument == "--") {
        Some(index) => (&arguments[..index], &arguments[index + 1..]),
        None => (arguments, &[]),
    }
}

fn run(arguments: &[String]) -> Result<ExitCode, String> {
    let (arguments, forwarded) = split_forwarded(arguments);
    let Some(command) = arguments.first() else {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    };
    let named = arguments.get(1).map(String::as_str);
    let every = arguments.iter().any(|argument| argument == "--all");
    if every {
        if !matches!(command.as_str(), "sync" | "check" | "build") {
            return Err(format!(
                "`--all` applies to sync, check and build, not `{command}`"
            ));
        }
        if let Some(name) = arguments[1..]
            .iter()
            .find(|argument| !argument.starts_with('-'))
        {
            return Err(format!(
                "name a firmware or pass `--all`, not both (`{name}`)"
            ));
        }
        let here = env::current_dir().map_err(|error| error.to_string())?;
        let project = Project::containing(&here).map_err(|error| error.to_string())?;
        return all::run(command, &cargo_arguments(command, forwarded), &project);
    }

    match command.as_str() {
        "new" => {
            let path = named.ok_or_else(|| format!("expected a path\n\n{USAGE}"))?;
            let chip = flag(arguments, "--chip").unwrap_or_else(|| "stm32f401re".to_owned());
            // The published crate by default. `--ferroforge` names a checkout
            // instead, for working on FerroForge itself.
            let dependency =
                ferroforge_dependency(arguments).unwrap_or_else(|| scaffold::PUBLISHED.to_owned());
            scaffold::create(Path::new(path), &chip, &dependency)
                .map_err(|error| error.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
        "add" => {
            let name = named
                .filter(|name| !name.starts_with('-'))
                .ok_or_else(|| format!("expected a firmware name\n\n{USAGE}"))?;
            // Not defaulted, unlike `new`: a project already exists, so there
            // is no first-run convenience to buy with a guess.
            let chip = flag(arguments, "--chip").ok_or_else(|| {
                format!(
                    "expected `--chip <name>`; `ferroforge chips` lists them: {}",
                    backend::known_chips().join(", ")
                )
            })?;
            let here = env::current_dir().map_err(|error| error.to_string())?;
            let project = Project::containing(&here).map_err(|error| error.to_string())?;
            scaffold::add(
                &project.root,
                name,
                &chip,
                ferroforge_dependency(arguments).as_deref(),
            )
            .map_err(|error| error.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
        "sync" => {
            let (root, firmware, written) = sync(named)?;
            println!("{}:", relative(&root, &firmware.display().to_string()));
            for path in written {
                println!("  {}", relative(&root, &path));
            }
            Ok(ExitCode::SUCCESS)
        }
        "check" | "build" | "run" => {
            let (_, firmware, _) = sync(named)?;
            delegate(&firmware, &cargo_arguments(command, forwarded))
        }
        "drift" => {
            let here = env::current_dir().map_err(|error| error.to_string())?;
            let project = Project::containing(&here).map_err(|error| error.to_string())?;
            drift::run(&project)
        }
        "chips" => {
            if arguments.iter().any(|argument| argument == "--names") {
                for chip in backend::known_chips() {
                    println!("{chip}");
                }
            } else {
                print!(
                    "{}",
                    backend::chip_table().map_err(|error| error.to_string())?
                );
            }
            Ok(ExitCode::SUCCESS)
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(ExitCode::SUCCESS)
        }
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

/// Resolve the firmware, then bring its derived files up to date. Every command
/// that touches a firmware does this first, so the files a build consumes
/// cannot drift from the chip the firmware declares.
fn sync(named: Option<&str>) -> Result<(PathBuf, PathBuf, Vec<String>), String> {
    let here = env::current_dir().map_err(|error| error.to_string())?;
    let project = Project::containing(&here).map_err(|error| error.to_string())?;
    let firmware = project
        .select(named.filter(|name| !name.starts_with('-')), &here)
        .map_err(|error| error.to_string())?;
    let written = sync_firmware(&firmware)?;
    Ok((project.root, firmware.path, written))
}

/// Bring one firmware's derived files up to date from the chip it declares.
fn sync_firmware(firmware: &Firmware) -> Result<Vec<String>, String> {
    let chip = firmware.chip().map_err(|error| error.to_string())?;
    let backend = Backend::for_chip(&chip).map_err(|error| error.to_string())?;
    let defmt_log = firmware.defmt_log().map_err(|error| error.to_string())?;
    backend
        .emit(&firmware.path, &defmt_log)
        .map_err(|error| error.to_string())
}

/// The Cargo command a verb means: `build` and `run` are release builds.
fn cargo_arguments<'a>(command: &'a str, forwarded: &'a [String]) -> Vec<&'a str> {
    let mut cargo = vec![command];
    if command != "check" {
        cargo.push("--release");
    }
    cargo.extend(forwarded.iter().map(String::as_str));
    cargo
}

/// Paths as a person would type them. `canonicalize` is needed to work out
/// which firmware you are standing in, but its Windows verbatim prefix is not
/// something to print at anyone.
fn relative(root: &Path, path: &str) -> String {
    let (path, note) = match path.split_once(" (") {
        Some((path, note)) => (path, format!(" ({note}")),
        None => (path, String::new()),
    };
    let shown = Path::new(path)
        .strip_prefix(root)
        .unwrap_or(Path::new(path));
    format!("{}{note}", shown.display().to_string().replace('\\', "/"))
}

/// Hand over to Cargo, from inside the firmware directory so its
/// `.cargo/config.toml` is discovered, and with its exit code preserved.
fn delegate(firmware: &Path, arguments: &[&str]) -> Result<ExitCode, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(arguments)
        .current_dir(firmware)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    match status.success() {
        true => Ok(ExitCode::SUCCESS),
        false => Ok(ExitCode::FAILURE),
    }
}

/// `--ferroforge <path>` as a manifest value: a path dependency on a checkout.
fn ferroforge_dependency(arguments: &[String]) -> Option<String> {
    flag(arguments, "--ferroforge")
        .map(|path| format!("{{ path = \"{}\" }}", path.replace('\\', "/")))
}

fn flag(arguments: &[String], name: &str) -> Option<String> {
    let position = arguments.iter().position(|argument| argument == name)?;
    arguments.get(position + 1).cloned()
}
