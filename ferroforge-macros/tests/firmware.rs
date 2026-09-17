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
fn checks_standalone(crate_path: &str, extra: &[&str]) {
    let mut arguments = vec![
        "check",
        "--lib",
        "--target",
        "thumbv7em-none-eabihf",
        "--offline",
    ];
    arguments.extend_from_slice(extra);
    let output = run(&repository_root().join(crate_path), &arguments);
    assert!(
        output.status.success(),
        "{crate_path} must check on its own:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn a_reusable_task_crate_checks_independently() {
    checks_standalone("tasks/blinky", &[]);
}

/// A portable crate that is a protocol rather than a pin. It names no HAL and
/// no chip, and it takes no forwarding feature to check - which is the claim
/// G1 makes about software task crates, on something more substantial than an
/// LED.
#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn a_portable_protocol_task_crate_checks_independently() {
    checks_standalone("tasks/msp-displayport", &[]);
}

/// The HAL-specific case. It has to name a chip to compile at all - a HAL cannot
/// be built without one - so the chip comes from a forwarding feature here, which
/// a firmware's own selection unifies with rather than fights.
#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn a_hal_specific_task_crate_checks_independently() {
    checks_standalone("tasks/stm32f4-timer", &["--features", "stm32f401"]);
}

/// The firmware crate is the binary, so one invocation proves the whole model:
/// `app!` expanded into a real `#[rtic::app]`, the adapters type-checked
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

/// A second HAL, and a Cortex-M7. The task crate pattern is the same; the HAL's
/// API is not, which is the point of a HAL-specific crate.
#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn a_task_crate_for_another_hal_checks_independently() {
    checks_standalone("tasks/stm32h7-timer", &["--features", "stm32h753v"]);
}

/// The first firmware here that is not an STM32F4: a different HAL, a different
/// PAC path, and a part with more memory regions than the pair `cortex-m-rt`
/// requires. It reuses `report` from the portable crate unchanged.
#[test]
#[ignore = "cross-compiles and links; run with --ignored"]
fn a_firmware_on_another_hal_links() {
    release_links("nucleo-h753zi");
}
