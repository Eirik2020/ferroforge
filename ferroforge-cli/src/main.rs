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
        are ignored; nested regions are compared on their own too. Rules in
        the project's `ferroforge.toml` forgive expected differences.

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

/// The options each command takes, and whether each takes a value. Anything
/// else before `--` is refused: an option that is silently ignored looks
/// applied and is not, and Cargo's own belong after `--`.
fn options(command: &str) -> &'static [(&'static str, bool)] {
    match command {
        "new" | "add" => &[("--chip", true), ("--ferroforge", true)],
        "sync" | "check" | "build" => &[("--all", false)],
        "chips" => &[("--names", false)],
        _ => &[],
    }
}

/// The command's arguments, checked against what it takes: the one name it may
/// be given, wherever it sits among the options, and each option's value.
struct Arguments<'a> {
    name: Option<&'a str>,
    values: Vec<(&'a str, &'a str)>,
    switches: Vec<&'a str>,
}

impl<'a> Arguments<'a> {
    fn parse(command: &str, arguments: &'a [String]) -> Result<Self, String> {
        let known = options(command);
        let mut parsed = Self {
            name: None,
            values: Vec::new(),
            switches: Vec::new(),
        };
        let mut rest = arguments.iter().map(String::as_str);
        while let Some(argument) = rest.next() {
            if !argument.starts_with('-') {
                if let Some(first) = parsed.name {
                    return Err(format!(
                        "`{command}` takes one name, and was given `{first}` and `{argument}`"
                    ));
                }
                parsed.name = Some(argument);
                continue;
            }
            match known.iter().find(|(option, _)| *option == argument) {
                Some((option, true)) => match rest.next() {
                    Some(value) if !value.starts_with('-') => parsed.values.push((option, value)),
                    _ => return Err(format!("`{option}` needs a value")),
                },
                Some((option, false)) => parsed.switches.push(option),
                // Named for where it does apply, since that is the mistake.
                None if argument == "--all" => {
                    return Err(format!(
                        "`--all` applies to sync, check and build, not `{command}`"
                    ));
                }
                None => {
                    return Err(format!(
                        "`{command}` has no option `{argument}`; arguments for Cargo \
                         go after `--`\n\n{USAGE}"
                    ));
                }
            }
        }
        Ok(parsed)
    }

    fn value(&self, option: &str) -> Option<&'a str> {
        self.values
            .iter()
            .find(|(name, _)| *name == option)
            .map(|(_, value)| *value)
    }

    fn switch(&self, option: &str) -> bool {
        self.switches.contains(&option)
    }
}

fn run(arguments: &[String]) -> Result<ExitCode, String> {
    let (arguments, forwarded) = split_forwarded(arguments);
    let Some(command) = arguments.first() else {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    };
    if matches!(command.as_str(), "--help" | "-h" | "help") {
        print!("{USAGE}");
        return Ok(ExitCode::SUCCESS);
    }
    let parsed = Arguments::parse(command, &arguments[1..])?;
    let named = parsed.name;
    let every = parsed.switch("--all");
    if every {
        if let Some(name) = named {
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
            let chip = parsed.value("--chip").unwrap_or("stm32f401re");
            // The published crate by default. `--ferroforge` names a checkout
            // instead, for working on FerroForge itself.
            let dependency =
                ferroforge_dependency(&parsed).unwrap_or_else(|| scaffold::PUBLISHED.to_owned());
            scaffold::create(Path::new(path), chip, &dependency)
                .map_err(|error| error.to_string())?;
            Ok(ExitCode::SUCCESS)
        }
        "add" => {
            let name = named.ok_or_else(|| format!("expected a firmware name\n\n{USAGE}"))?;
            // Not defaulted, unlike `new`: a project already exists, so there
            // is no first-run convenience to buy with a guess.
            let chip = parsed.value("--chip").ok_or_else(|| {
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
                chip,
                ferroforge_dependency(&parsed).as_deref(),
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
            if parsed.switch("--names") {
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
        .select(named, &here)
        .map_err(|error| error.to_string())?;
    let written = sync_firmware(&firmware)?;
    Ok((project.root, firmware.path, written))
}

/// Bring one firmware's derived files up to date from the chip it declares.
fn sync_firmware(firmware: &Firmware) -> Result<Vec<String>, String> {
    let chip = firmware.chip().map_err(|error| error.to_string())?;
    let backend = Backend::for_chip(&chip).map_err(|error| error.to_string())?;
    let settings = firmware.settings().map_err(|error| error.to_string())?;
    backend
        .emit(&firmware.path, &settings)
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
/// `.cargo/config.toml` is discovered, and with its exit code preserved: a
/// script telling a compile error (101) from a failure to start must still be
/// able to. A Cargo ended by a signal has no code, and is a plain failure.
fn delegate(firmware: &Path, arguments: &[&str]) -> Result<ExitCode, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let status = Command::new(cargo)
        .args(arguments)
        .current_dir(firmware)
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    Ok(match status.code() {
        Some(0) => ExitCode::SUCCESS,
        Some(code) => ExitCode::from(u8::try_from(code).unwrap_or(1)),
        None => ExitCode::FAILURE,
    })
}

/// `--ferroforge <path>` as a manifest value: a path dependency on a checkout.
fn ferroforge_dependency(arguments: &Arguments<'_>) -> Option<String> {
    arguments
        .value("--ferroforge")
        .map(|path| format!("{{ path = \"{}\" }}", path.replace('\\', "/")))
}
