//! The FerroForge CLI.
//!
//! The verbs are Cargo's, because a firmware crate is an ordinary Cargo package
//! and pretending otherwise would only add vocabulary. `check`, `build` and
//! `run` refresh what the chip implies and then hand over to Cargo; `sync` is
//! the one verb Cargo has no analogue for, and does only the refreshing.

mod backend;
mod project;
mod scaffold;

use std::{
    env,
    path::{Path, PathBuf},
    process::{Command, ExitCode},
};

use backend::Backend;
use project::Project;

const USAGE: &str = "\
ferroforge - compose reusable RTIC tasks into firmware

USAGE:
    ferroforge new <path> [--chip <name>] [--ferroforge <path>]
        Create a project: one firmware, a task crate, and the files its chip
        implies.

    ferroforge sync [<firmware>]
        Refresh what the chip implies - memory.x, .cargo/config.toml,
        Embed.toml, and the platform crates in Cargo.toml. Nothing else in the
        manifest is touched.

    ferroforge check [<firmware>] [-- <cargo args>]
    ferroforge build [<firmware>] [-- <cargo args>]
    ferroforge run   [<firmware>] [-- <cargo args>]
        Sync, then the matching cargo command in that firmware. `build` and
        `run` are release builds; `run` flashes via the configured probe.

    ferroforge chips
        The chips this build of FerroForge knows.

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

    match command.as_str() {
        "new" => {
            let path = named.ok_or_else(|| format!("expected a path\n\n{USAGE}"))?;
            let chip = flag(arguments, "--chip").unwrap_or_else(|| "stm32f401re".to_owned());
            // FerroForge is not published, so a version requirement resolves to
            // nothing. `--ferroforge` names a checkout; without it the generated
            // manifests are what they will be once it is published, and say so.
            let dependency = match flag(arguments, "--ferroforge") {
                Some(path) => format!("{{ path = \"{}\" }}", path.replace('\\', "/")),
                None => "\"0.1\"".to_owned(),
            };
            scaffold::create(Path::new(path), &chip, &dependency)
                .map_err(|error| error.to_string())?;
            if flag(arguments, "--ferroforge").is_none() {
                println!(
                    "\nnote: FerroForge is not published yet, so `ferroforge = \"0.1\"` will \
                     not resolve.\n      Re-run with `--ferroforge <path-to-checkout>/ferroforge` \
                     to build today."
                );
            }
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
            let mut cargo = vec![command.as_str()];
            if command != "check" {
                cargo.push("--release");
            }
            cargo.extend(forwarded.iter().map(String::as_str));
            delegate(&firmware, &cargo)
        }
        "chips" => {
            for chip in backend::known_chips() {
                println!("{chip}");
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
    let chip = firmware.chip().map_err(|error| error.to_string())?;

    let backend = Backend::for_chip(&chip).map_err(|error| error.to_string())?;
    let defmt_log = firmware.defmt_log().map_err(|error| error.to_string())?;
    let written = backend
        .emit(&firmware.path, &defmt_log)
        .map_err(|error| error.to_string())?;
    Ok((project.root, firmware.path, written))
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

fn flag(arguments: &[String], name: &str) -> Option<String> {
    let position = arguments.iter().position(|argument| argument == name)?;
    arguments.get(position + 1).cloned()
}
