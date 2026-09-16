//! The CLI's job is to be the only place chip facts live. These tests pin that:
//! the files it emits must match what the firmware actually builds with, every
//! one of them must change when the chip does, and backend data must not restate
//! anything the compiler already owns.

use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

const EMITTED_FILES: [&str; 3] = ["memory.x", ".cargo/config.toml", "Embed.toml"];
const MARKER_BEGIN: &str = "# ferroforge:platform-dependencies";
const MARKER_END: &str = "# ferroforge:end";

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the CLI crate sits directly below the repository root")
}

fn ferroforge(arguments: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_ferroforge"))
        .args(arguments)
        .current_dir(repository_root())
        .output()
        .expect("the CLI binary must run")
}

fn shipped_backend() -> PathBuf {
    repository_root().join("backends/stm32f4/stm32f401re.toml")
}

/// A firmware directory holding only what emitting needs: a manifest with the
/// markers that say where the platform crates go.
fn scaffold(name: &str) -> PathBuf {
    let directory = repository_root().join("target").join(name);
    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(&directory).unwrap();
    fs::write(
        directory.join("Cargo.toml"),
        format!("[dependencies]\n{MARKER_BEGIN}\n{MARKER_END}\n"),
    )
    .unwrap();
    directory
}

/// The platform crates the CLI owns, markers included.
fn platform_block(manifest: &Path) -> String {
    let text = fs::read_to_string(manifest).unwrap();
    let begin = text
        .find(MARKER_BEGIN)
        .expect("the manifest must be marked");
    let end = text.find(MARKER_END).expect("the manifest must be marked");
    text[begin..end].to_owned()
}

fn emit(backend: &Path, into: &Path) {
    let output = ferroforge(&["target", backend.to_str().unwrap(), into.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "emitting must succeed:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Regenerating must not change what the firmware builds with. If this fails, a
/// chip fact has drifted between the backend data and the firmware - the whole
/// failure mode the backend exists to prevent.
#[test]
fn emitted_files_match_the_firmware() {
    let emitted_into = scaffold("backend-emit-test");
    emit(&shipped_backend(), &emitted_into);

    let firmware = repository_root().join("firmware/nucleo-f401re");
    for file in EMITTED_FILES {
        let emitted = fs::read_to_string(emitted_into.join(file)).unwrap();
        let in_use = fs::read_to_string(firmware.join(file))
            .unwrap_or_else(|_| panic!("firmware is missing {file}"));
        assert_eq!(
            emitted, in_use,
            "{file} differs between the backend data and the firmware"
        );
    }

    assert_eq!(
        platform_block(&emitted_into.join("Cargo.toml")),
        platform_block(&firmware.join("Cargo.toml")),
        "the firmware's platform crates differ from the ones its chip declares"
    );
}

/// The other half of the gate: every emitted file must be derived, so selecting
/// a different chip must change all of them. A file that stays put is one whose
/// content is hardcoded in the CLI rather than read from the backend.
#[test]
fn a_different_chip_changes_every_emitted_file() {
    let variant = repository_root().join("target/backend-variant.toml");
    let original = fs::read_to_string(shipped_backend()).unwrap();
    let altered = original
        .replace("STM32F401RE", "STM32F411RE")
        .replace("thumbv7em-none-eabihf", "thumbv7m-none-eabi")
        .replace("524288", "262144")
        .replace("stm32f401", "stm32f411");
    assert_ne!(original, altered, "the variant must actually differ");
    fs::write(&variant, &altered).unwrap();

    let first = scaffold("backend-chip-a");
    let second = scaffold("backend-chip-b");
    emit(&shipped_backend(), &first);
    emit(&variant, &second);

    for file in EMITTED_FILES {
        assert_ne!(
            fs::read_to_string(first.join(file)).unwrap(),
            fs::read_to_string(second.join(file)).unwrap(),
            "{file} did not change with the chip, so it is not derived from the backend"
        );
    }
    assert_ne!(
        platform_block(&first.join("Cargo.toml")),
        platform_block(&second.join("Cargo.toml")),
        "the manifest's platform crates did not change with the chip"
    );
}

/// The manifest is authored, so the CLI replaces a marked region and touches
/// nothing else. Guessing where the platform crates belong would be worse than
/// reporting that the markers are missing.
#[test]
fn an_unmarked_manifest_is_an_error_not_a_guess() {
    let directory = scaffold("backend-unmarked");
    let authored = "[dependencies]\ndefmt = \"1\"\n";
    fs::write(directory.join("Cargo.toml"), authored).unwrap();

    let output = ferroforge(&[
        "target",
        shipped_backend().to_str().unwrap(),
        directory.to_str().unwrap(),
    ]);
    assert!(!output.status.success(), "an unmarked manifest must fail");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains(MARKER_BEGIN), "{stderr}");
    assert_eq!(
        fs::read_to_string(directory.join("Cargo.toml")).unwrap(),
        authored,
        "a failed emit must leave the authored manifest alone"
    );
}

/// Only the marked region is the CLI's. Everything around it is the
/// application's and must survive regeneration untouched.
#[test]
fn regenerating_leaves_authored_manifest_lines_alone() {
    let directory = scaffold("backend-authored-lines");
    let manifest = directory.join("Cargo.toml");
    fs::write(
        &manifest,
        format!(
            "[package]\nname = \"x\"\n\n[dependencies]\ndefmt = \"1\"\n\
             {MARKER_BEGIN}\nstale = \"0\"\n{MARKER_END}\n\n\
             [profile.release]\nlto = true\n"
        ),
    )
    .unwrap();

    emit(&shipped_backend(), &directory);
    let updated = fs::read_to_string(&manifest).unwrap();

    for authored in [
        "[package]",
        "name = \"x\"",
        "defmt = \"1\"",
        "[profile.release]",
        "lto = true",
    ] {
        assert!(updated.contains(authored), "lost `{authored}`:\n{updated}");
    }
    assert!(
        !updated.contains("stale = \"0\""),
        "the previous block survived:\n{updated}"
    );
    assert!(updated.contains("stm32f4xx-hal"), "{updated}");
}

/// Per G4 the device's own enum is the only interrupt list, so backend data
/// naming one is an error rather than something quietly ignored.
#[test]
fn backend_data_may_not_name_interrupts() {
    let path = repository_root().join("target/backend-with-interrupts.toml");
    fs::write(
        &path,
        "[chip]\n\
         name = \"X\"\n\
         rust-target = \"thumbv7em-none-eabihf\"\n\
         probe-rs-chip = \"X\"\n\
         device = \"x::pac\"\n\n\
         [memory]\n\
         flash-origin = 0\nflash-size = 1024\nram-origin = 0\nram-size = 1024\ntext-offset = 0\n\n\
         [interrupts]\n\
         TIM2 = 28\n",
    )
    .unwrap();

    let output = ferroforge(&["target", path.to_str().unwrap(), "target/unused"]);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "naming interrupts must fail");
    assert!(stderr.contains("names interrupts"), "{stderr}");
    assert!(stderr.contains("G4"), "the error must say why: {stderr}");
}

/// A comment explaining the interrupt rule must not trip the check on it.
#[test]
fn a_comment_about_interrupts_is_not_interrupt_data() {
    let output = ferroforge(&["platform-deps", shipped_backend().to_str().unwrap()]);
    assert!(
        output.status.success(),
        "the shipped backend documents the rule in a comment and must still load:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("stm32f4xx-hal"), "{stdout}");
    // Logging and panic backends are the firmware's choice, not the chip's.
    assert!(!stdout.contains("defmt-rtt"), "{stdout}");
    assert!(!stdout.contains("panic-probe"), "{stdout}");
}
