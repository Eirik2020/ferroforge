//! End-to-end evidence for the call-through model.
//!
//! The macros are unit-tested by expansion, but expansion proving nothing about
//! whether the result compiles was exactly the gap that made the previous
//! design's mock layer unreliable. These tests run the real compiler over the
//! real crates for the real target.
//!
//! They are `#[ignore]`d because each drives a cross-compile that takes tens of
//! seconds; run them with `--ignored` when changing an expansion.

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
};

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the macro crate sits directly below the repository root")
        .to_path_buf()
}

fn run(directory: &Path, arguments: &[&str]) -> std::process::Output {
    Command::new(cargo())
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("cargo must be runnable")
}

/// A reusable task crate must compile on its own, for the embedded target, with
/// no firmware and no generated interfaces. This is what independent checking
/// means now that there is no mock layer.
#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn a_reusable_task_crate_checks_independently() {
    let output = run(
        &repository_root().join("tasks/blinky"),
        &[
            "check",
            "--lib",
            "--target",
            "thumbv7em-none-eabihf",
            "--offline",
        ],
    );
    assert!(
        output.status.success(),
        "tasks/blinky must check on its own:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// The firmware crate is the binary, so one invocation proves the whole model:
/// `compose!` expanded into a real `#[rtic::app]`, the adapters type-checked
/// against the task crate, and the result linked for the target.
fn release_links(name: &str) {
    let firmware = repository_root().join("firmware").join(name);
    let output = run(
        &firmware,
        &["build", "--release", "--bin", name, "--offline"],
    );
    assert!(
        output.status.success(),
        "firmware/{name} must release-link:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        firmware
            .join("target/thumbv7em-none-eabihf/release")
            .join(name)
            .exists(),
        "the linked binary for {name} must exist"
    );
}

#[test]
#[ignore = "cross-compiles and links; run with --ignored"]
fn the_firmware_checks_and_release_links() {
    release_links("nucleo-f401re");
}

/// Reuse, not portability: a second application on the same board, selecting the
/// same definitions. It instantiates `blink` twice with different names, pins,
/// counters, gates and periods, and binds `on_tick` to `TIM3` rather than
/// `TIM2`. If this links while `tasks/blinky` is unchanged, a definition really
/// is reusable across applications rather than written for one of them.
#[test]
#[ignore = "cross-compiles and links; run with --ignored"]
fn a_second_firmware_reuses_the_same_definitions() {
    release_links("nucleo-f401re-beacon");
}
