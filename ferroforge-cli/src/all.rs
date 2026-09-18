//! `--all`: one command over every firmware in the project.
//!
//! Each firmware is its own Cargo workspace (G5), so there is no single Cargo
//! invocation to delegate to; this runs one per firmware, in the order the
//! project lists them. A failure does not stop the rest, because the point of
//! asking for all of them is to learn which are broken.
//!
//! Quiet on success: a firmware that builds cleanly is one line. One with
//! warnings or errors shows Cargo's output under its line, since that is what
//! someone would act on.

use std::{
    env,
    io::IsTerminal,
    path::Path,
    process::{Command, ExitCode},
};

use crate::{project::Project, sync_firmware};

enum Outcome {
    Clean,
    Warned(String),
    Failed(String),
}

/// Terminal colour, only when stdout is one and `NO_COLOR` is unset.
pub struct Style {
    colour: bool,
}

impl Style {
    pub fn detect() -> Self {
        Self {
            colour: std::io::stdout().is_terminal() && env::var_os("NO_COLOR").is_none(),
        }
    }

    pub fn paint(&self, code: &str, text: &str) -> String {
        match self.colour {
            true => format!("\x1b[{code}m{text}\x1b[0m"),
            false => text.to_owned(),
        }
    }
}

pub fn run(command: &str, cargo: &[&str], project: &Project) -> Result<ExitCode, String> {
    let firmwares = project.firmwares().map_err(|error| error.to_string())?;
    let style = Style::detect();
    let width = firmwares
        .iter()
        .map(|firmware| firmware.name.len())
        .max()
        .unwrap_or(0)
        + "firmware/".len();

    let mut arguments = cargo.to_vec();
    // Cargo colours only a terminal, and here its output is captured. Asked
    // for explicitly when this output is going to one, unless the caller
    // already chose.
    if style.colour && !cargo.iter().any(|argument| argument.starts_with("--color")) {
        arguments.push("--color=always");
    }

    let (mut warned, mut failed) = (Vec::new(), Vec::new());
    for firmware in &firmwares {
        let outcome = match sync_firmware(firmware) {
            Err(message) => Outcome::Failed(message),
            Ok(_) if command == "sync" => Outcome::Clean,
            Ok(_) => cargo_in(&firmware.path, &arguments)?,
        };
        let label = format!("{:width$}", format!("firmware/{}", firmware.name));
        match outcome {
            Outcome::Clean => println!("{label}   {}", style.paint("32", "ok")),
            Outcome::Warned(output) => {
                println!("{label}   {}", style.paint("33", "warn"));
                print_indented(&output);
                warned.push(firmware.name.clone());
            }
            Outcome::Failed(output) => {
                println!("{label}   {}", style.paint("31", "FAILED"));
                print_indented(&output);
                failed.push(firmware.name.clone());
            }
        }
    }

    let done = match command {
        "sync" => "synced",
        "check" => "checked",
        _ => "built",
    };
    let mut summary = format!(
        "\n{} of {} {done}",
        firmwares.len() - failed.len(),
        firmwares.len()
    );
    if !warned.is_empty() {
        summary.push_str(&format!("; warnings in {}", warned.join(", ")));
    }
    if !failed.is_empty() {
        summary.push_str(&format!("; failed: {}", failed.join(", ")));
    }
    println!("{summary}");

    match failed.is_empty() {
        true => Ok(ExitCode::SUCCESS),
        false => Ok(ExitCode::FAILURE),
    }
}

/// Cargo in one firmware, output captured. Not being able to start Cargo at
/// all is an error for the whole command, not one firmware's failure.
fn cargo_in(firmware: &Path, arguments: &[&str]) -> Result<Outcome, String> {
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .args(arguments)
        .current_dir(firmware)
        .output()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    let mut text = String::from_utf8_lossy(&output.stdout).into_owned();
    text.push_str(&String::from_utf8_lossy(&output.stderr));

    Ok(if !output.status.success() {
        Outcome::Failed(text)
    } else if has_warning(&text) {
        Outcome::Warned(text)
    } else {
        Outcome::Clean
    })
}

/// Cargo replays a crate's warnings on every build, even when nothing was
/// recompiled, so a warning is visible here however warm the cache is.
fn has_warning(output: &str) -> bool {
    output
        .lines()
        .any(|line| strip_colour(line).trim_start().starts_with("warning"))
}

fn strip_colour(line: &str) -> String {
    let mut plain = String::with_capacity(line.len());
    let mut characters = line.chars();
    while let Some(character) = characters.next() {
        if character == '\x1b' {
            // `ESC [ ... m`: skip through the terminating letter.
            for inner in characters.by_ref() {
                if inner.is_ascii_alphabetic() {
                    break;
                }
            }
        } else {
            plain.push(character);
        }
    }
    plain
}

fn print_indented(output: &str) {
    for line in output.trim_end().lines() {
        println!("    {line}");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_coloured_warning_is_still_a_warning() {
        assert!(has_warning("\x1b[1m\x1b[33mwarning\x1b[0m: unused import"));
        assert!(has_warning("   Compiling x\nwarning: unused import\n"));
        assert!(!has_warning("   Compiling x\n    Finished `release`\n"));
    }
}
