//! The FerroForge CLI.
//!
//! It does two things: emit a firmware's target files from chip-family data, and
//! run the build. There is no generation step to orchestrate - a firmware crate
//! is its own binary - so the pipeline that used to exist is now `cargo build`.

mod backend;

use std::{env, path::PathBuf, process::ExitCode};

use backend::Backend;

const USAGE: &str = "\
ferroforge - compose reusable RTIC tasks into firmware

USAGE:
    ferroforge target <backend.toml> <firmware-dir> [--defmt-log <level>]
        Emit memory.x, .cargo/config.toml and Embed.toml for a firmware from
        chip-family data, and refresh the platform crates in its Cargo.toml.
        Those files are overwritten; in the manifest only the region between
        `# ferroforge:platform-dependencies` and `# ferroforge:end` is touched.

    ferroforge platform-deps <backend.toml>
        Print the same platform dependency lines without writing anything.
        Logging and panic backends are the firmware's own choice.
";

fn main() -> ExitCode {
    let arguments = env::args().skip(1).collect::<Vec<_>>();
    match run(&arguments) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("error: {message}");
            ExitCode::FAILURE
        }
    }
}

fn run(arguments: &[String]) -> Result<(), String> {
    let Some(command) = arguments.first() else {
        print!("{USAGE}");
        return Ok(());
    };

    match command.as_str() {
        "target" => {
            let backend_path = argument(arguments, 1, "a backend TOML path")?;
            let firmware = argument(arguments, 2, "a firmware directory")?;
            let defmt_log = flag(arguments, "--defmt-log").unwrap_or_else(|| "info".to_owned());

            let backend = Backend::load(&PathBuf::from(&backend_path)).map_err(to_message)?;
            let written = backend
                .emit(&PathBuf::from(&firmware), &defmt_log)
                .map_err(to_message)?;
            println!("{} ({}):", backend.chip.name, backend.chip.device);
            for path in written {
                println!("  wrote {path}");
            }
            Ok(())
        }
        "platform-deps" => {
            let backend_path = argument(arguments, 1, "a backend TOML path")?;
            let backend = Backend::load(&PathBuf::from(&backend_path)).map_err(to_message)?;
            println!("# {}: device = {}", backend.chip.name, backend.chip.device);
            print!("{}", backend.platform_dependency_lines());
            Ok(())
        }
        "--help" | "-h" | "help" => {
            print!("{USAGE}");
            Ok(())
        }
        other => Err(format!("unknown command `{other}`\n\n{USAGE}")),
    }
}

fn argument(arguments: &[String], index: usize, expected: &str) -> Result<String, String> {
    arguments
        .get(index)
        .filter(|value| !value.starts_with("--"))
        .cloned()
        .ok_or_else(|| format!("expected {expected}\n\n{USAGE}"))
}

fn flag(arguments: &[String], name: &str) -> Option<String> {
    let position = arguments.iter().position(|argument| argument == name)?;
    arguments.get(position + 1).cloned()
}

fn to_message(error: backend::Error) -> String {
    error.to_string()
}
