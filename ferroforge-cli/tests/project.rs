//! The conventions, exercised through the binary.
//!
//! Recognition, selection and scaffolding are what a person meets first, so
//! these assert the messages as well as the outcomes: an error that does not
//! say what to do next is a failure even when the exit code is right.

use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
};

fn repository_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the CLI crate sits directly below the repository root")
}

fn ferroforge_in(directory: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferroforge"))
        .args(arguments)
        .current_dir(directory)
        .output()
        .expect("the CLI binary must run")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

/// A bare project: manifests only, no sources. Enough to be recognized and
/// selected from, which is all these tests are about.
fn scratch(name: &str, firmwares: &[(&str, Option<&str>)]) -> PathBuf {
    let root = repository_root().join("target/project-tests").join(name);
    let _ = fs::remove_dir_all(&root);
    for (firmware, chip) in firmwares {
        let directory = root.join("firmware").join(firmware);
        fs::create_dir_all(directory.join("src")).unwrap();
        let metadata = match chip {
            Some(chip) => {
                format!("\n[package.metadata.ferroforge]\nchip = \"{chip}\"\n")
            }
            None => String::new(),
        };
        fs::write(
            directory.join("Cargo.toml"),
            format!(
                "[package]\nname = \"{firmware}\"\nversion = \"0.1.0\"\n{metadata}\n\
                 [dependencies]\n\
                 # ferroforge:platform-dependencies\n# ferroforge:end\n"
            ),
        )
        .unwrap();
    }
    fs::create_dir_all(&root).unwrap();
    root
}

/// Re-syncing the real project must change nothing. If it does, a chip fact has
/// drifted between what a firmware declares and what it holds.
#[test]
fn syncing_the_real_project_changes_nothing() {
    let firmware = repository_root().join("firmware");
    let before: Vec<(PathBuf, String)> = fs::read_dir(&firmware)
        .unwrap()
        .flatten()
        .flat_map(|entry| {
            let path = entry.path();
            ["memory.x", ".cargo/config.toml", "Embed.toml", "Cargo.toml"]
                .into_iter()
                .filter_map(move |file| {
                    let file = path.join(file);
                    fs::read_to_string(&file).ok().map(|text| (file, text))
                })
        })
        .collect();
    assert!(before.len() >= 8, "expected two firmwares of four files");

    for entry in fs::read_dir(&firmware).unwrap().flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let output = ferroforge_in(repository_root(), &["sync", &name]);
        assert!(output.status.success(), "{}", stderr(&output));
    }

    for (path, text) in before {
        assert_eq!(
            fs::read_to_string(&path).unwrap(),
            text,
            "{} changed when re-synced",
            path.display()
        );
    }
}

/// The convention is the whole of recognition, so failing to find it must say
/// what was looked for.
#[test]
fn outside_a_project_the_error_says_what_was_looked_for() {
    // Outside the repository: anywhere under it is legitimately inside a
    // project, `target/` included, because recognition walks up.
    let elsewhere = std::env::temp_dir().join("ferroforge-not-a-project");
    let _ = fs::remove_dir_all(&elsewhere);
    fs::create_dir_all(&elsewhere).unwrap();

    let output = ferroforge_in(&elsewhere, &["sync"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(
        message.contains("not inside a FerroForge project"),
        "{message}"
    );
    assert!(message.contains("firmware"), "{message}");
}

/// A project is the nearest parent with a `firmware/`, so a command works from
/// anywhere inside it.
#[test]
fn a_project_is_found_from_a_subdirectory() {
    let root = scratch("nested", &[("only", Some("stm32f401re"))]);
    let deep = root.join("firmware/only/src");
    let output = ferroforge_in(&deep, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("firmware/only"),
        "{}",
        stdout(&output)
    );
}

/// With one application there is nothing to disambiguate.
#[test]
fn a_lone_firmware_needs_no_naming() {
    let root = scratch("lone", &[("solo", Some("stm32f401re"))]);
    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

/// With several, guessing would be worse than asking - and the error has to
/// list them, or the author has to go and look.
#[test]
fn several_firmwares_are_ambiguous_and_listed() {
    let root = scratch(
        "several",
        &[
            ("alpha", Some("stm32f401re")),
            ("beta", Some("stm32f401re")),
        ],
    );
    let output = ferroforge_in(&root, &["sync"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("alpha"), "{message}");
    assert!(message.contains("beta"), "{message}");
}

/// Standing inside one resolves the ambiguity, exactly as Cargo resolves a
/// package from the working directory.
#[test]
fn standing_inside_one_resolves_the_ambiguity() {
    let root = scratch(
        "stood-in",
        &[
            ("alpha", Some("stm32f401re")),
            ("beta", Some("stm32f401re")),
        ],
    );
    let output = ferroforge_in(&root.join("firmware/beta"), &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(stdout(&output).contains("beta"), "{}", stdout(&output));
}

#[test]
fn naming_a_firmware_that_does_not_exist_lists_the_ones_that_do() {
    let root = scratch("misnamed", &[("alpha", Some("stm32f401re"))]);
    let output = ferroforge_in(&root, &["sync", "alfa"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("alfa"), "{message}");
    assert!(message.contains("alpha"), "{message}");
}

/// Guessing a chip would produce a firmware that links and cannot run, so the
/// error names the table to add and the key to put in it.
#[test]
fn a_firmware_without_a_chip_is_told_what_to_add() {
    let root = scratch("chipless", &[("solo", None)]);
    let output = ferroforge_in(&root, &["sync"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("does not say which chip"), "{message}");
    assert!(
        message.contains("[package.metadata.ferroforge]"),
        "{message}"
    );
    assert!(message.contains("chip = "), "{message}");
}

#[test]
fn an_unknown_chip_lists_what_ferroforge_ships() {
    let root = scratch("unknown-chip", &[("solo", Some("stm32f999zz"))]);
    let output = ferroforge_in(&root, &["sync"]);
    assert!(!output.status.success());
    let message = stderr(&output);
    assert!(message.contains("stm32f999zz"), "{message}");
    assert!(message.contains("stm32f401re"), "{message}");
}

/// `new` writes the same convention the rest of the CLI recognizes. If these
/// ever disagree, a scaffolded project would not be a project.
#[test]
fn a_scaffolded_project_is_recognized_by_the_convention() {
    let root = repository_root().join("target/project-tests/scaffolded");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.parent().unwrap()).unwrap();

    let output = ferroforge_in(repository_root(), &["new", root.to_str().unwrap()]);
    assert!(output.status.success(), "{}", stderr(&output));

    assert!(root.join("firmware/scaffolded/Cargo.toml").is_file());
    assert!(root.join("firmware/scaffolded/src/main.rs").is_file());
    assert!(root.join("firmware/scaffolded/memory.x").is_file());
    assert!(
        root.join("firmware/scaffolded/.cargo/config.toml")
            .is_file()
    );
    assert!(root.join("tasks/scaffolded-tasks/src/lib.rs").is_file());

    // A new project depends on the published crate unless told otherwise.
    let manifest = fs::read_to_string(root.join("firmware/scaffolded/Cargo.toml")).unwrap();
    assert!(manifest.contains("ferroforge = \"0.1\""), "{manifest}");

    // And the convention must recognize what it just wrote.
    let resynced = ferroforge_in(&root, &["sync"]);
    assert!(resynced.status.success(), "{}", stderr(&resynced));
}

#[test]
fn creating_over_something_that_exists_refuses() {
    let root = repository_root().join("target/project-tests/occupied");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(&root).unwrap();

    let output = ferroforge_in(repository_root(), &["new", root.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("already exists"),
        "{}",
        stderr(&output)
    );
}

#[test]
fn creating_for_an_unknown_chip_writes_nothing() {
    let root = repository_root().join("target/project-tests/bad-chip");
    let _ = fs::remove_dir_all(&root);

    let output = ferroforge_in(
        repository_root(),
        &["new", root.to_str().unwrap(), "--chip", "stm32f999zz"],
    );
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("stm32f401re"),
        "{}",
        stderr(&output)
    );
    assert!(
        !root.exists(),
        "a rejected chip must not leave half a project behind"
    );
}

/// A chip feature enabled outside the generated block is a chip named twice.
/// The HAL does reject two at once, but from a build script, as a panic with no
/// cause - so this must be caught before Cargo is ever reached.
#[test]
fn a_chip_feature_outside_the_generated_block_is_refused() {
    let root = repository_root().join("target/project-tests/feature-conflict");
    let _ = fs::remove_dir_all(&root);
    let firmware = root.join("firmware/board");
    fs::create_dir_all(&firmware).unwrap();
    let authored = "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
         [package.metadata.ferroforge]\nchip = \"stm32f411re\"\n\n\
         [dependencies]\n\
         some-task-crate = { path = \"../x\", features = [\"stm32f401\"] }\n\
         # ferroforge:platform-dependencies\n# ferroforge:end\n";
    fs::write(firmware.join("Cargo.toml"), authored).unwrap();

    let output = ferroforge_in(&root, &["sync"]);
    assert!(!output.status.success(), "{}", stdout(&output));
    let message = stderr(&output);
    assert!(message.contains("stm32f401"), "{message}");
    assert!(message.contains("some-task-crate"), "{message}");
    assert!(message.contains("STM32F411RE"), "{message}");
    assert!(
        !message.contains(r"\?\"),
        "a verbatim path is not something to print: {message}"
    );
    assert_eq!(
        fs::read_to_string(firmware.join("Cargo.toml")).unwrap(),
        authored,
        "a refused sync must leave the manifest alone"
    );
}

/// The feature the selected chip *does* want must not be flagged - it is the
/// generated block's own content.
#[test]
fn the_selected_chips_own_feature_is_not_a_conflict() {
    let root = repository_root().join("target/project-tests/own-feature");
    let _ = fs::remove_dir_all(&root);
    let firmware = root.join("firmware/board");
    fs::create_dir_all(&firmware).unwrap();
    fs::write(
        firmware.join("Cargo.toml"),
        "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
         [package.metadata.ferroforge]\nchip = \"stm32f401re\"\n\n\
         [dependencies]\n# ferroforge:platform-dependencies\n# ferroforge:end\n",
    )
    .unwrap();

    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let written = fs::read_to_string(firmware.join("Cargo.toml")).unwrap();
    assert!(written.contains("\"stm32f401\""), "{written}");
}

/// Both chips must be reachable, or the registry is a list of one with extra
/// steps.
#[test]
fn every_known_chip_can_be_selected() {
    let output = ferroforge_in(repository_root(), &["chips"]);
    assert!(output.status.success());
    let listed: Vec<String> = stdout(&output).lines().map(str::to_owned).collect();
    assert!(listed.len() >= 2, "{listed:?}");

    for chip in listed {
        let root = repository_root()
            .join("target/project-tests")
            .join(format!("chip-{chip}"));
        let _ = fs::remove_dir_all(&root);
        let firmware = root.join("firmware/board");
        fs::create_dir_all(&firmware).unwrap();
        fs::write(
            firmware.join("Cargo.toml"),
            format!(
                "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
                 [package.metadata.ferroforge]\nchip = \"{chip}\"\n\n\
                 [dependencies]\n# ferroforge:platform-dependencies\n# ferroforge:end\n"
            ),
        )
        .unwrap();

        let output = ferroforge_in(&root, &["sync"]);
        assert!(output.status.success(), "{chip}: {}", stderr(&output));
        assert!(
            firmware.join("memory.x").is_file(),
            "{chip} emitted no memory.x"
        );
    }
}

/// Changing a firmware's chip is the workflow the derived files exist for. The
/// block still names the previous chip until it is rewritten, so scanning it
/// for a conflict would make exactly this impossible.
#[test]
fn a_firmware_can_change_its_chip() {
    let root = repository_root().join("target/project-tests/chip-change");
    let _ = fs::remove_dir_all(&root);
    let firmware = root.join("firmware/board");
    fs::create_dir_all(&firmware).unwrap();
    let manifest = firmware.join("Cargo.toml");

    let with_chip = |chip: &str, block: &str| {
        format!(
            "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
             [package.metadata.ferroforge]\nchip = \"{chip}\"\n\n\
             [dependencies]\n\
             # ferroforge:platform-dependencies\n{block}# ferroforge:end\n"
        )
    };

    fs::write(&manifest, with_chip("stm32f401re", "")).unwrap();
    let first = ferroforge_in(&root, &["sync"]);
    assert!(first.status.success(), "{}", stderr(&first));
    let after_first = fs::read_to_string(&manifest).unwrap();
    assert!(after_first.contains("\"stm32f401\""), "{after_first}");

    // Now the only edit a person makes: one line.
    let switched = after_first.replace("chip = \"stm32f401re\"", "chip = \"stm32f411re\"");
    fs::write(&manifest, switched).unwrap();

    let second = ferroforge_in(&root, &["sync"]);
    assert!(
        second.status.success(),
        "changing the chip must sync, not trip the conflict check:\n{}",
        stderr(&second)
    );
    let after_second = fs::read_to_string(&manifest).unwrap();
    assert!(after_second.contains("\"stm32f411\""), "{after_second}");
    assert!(
        !after_second.contains("\"stm32f401\""),
        "the previous chip's feature must be gone:\n{after_second}"
    );

    // And every other derived file must have followed.
    let memory = fs::read_to_string(firmware.join("memory.x")).unwrap();
    assert!(memory.contains("128K"), "F411 has 128K of RAM:\n{memory}");
    let embed = fs::read_to_string(firmware.join("Embed.toml")).unwrap();
    assert!(embed.contains("STM32F411RE"), "{embed}");
}

/// A part with more memory than the FLASH/RAM pair must have all of it in the
/// linker script, or a firmware cannot place anything there.
#[test]
fn extra_memory_regions_reach_the_linker_script() {
    let root = repository_root().join("target/project-tests/regions");
    let _ = fs::remove_dir_all(&root);
    let firmware = root.join("firmware/board");
    fs::create_dir_all(&firmware).unwrap();
    fs::write(
        firmware.join("Cargo.toml"),
        "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
         [package.metadata.ferroforge]\nchip = \"stm32h753zi\"\n\n\
         [dependencies]\n# ferroforge:platform-dependencies\n# ferroforge:end\n",
    )
    .unwrap();

    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let memory = fs::read_to_string(firmware.join("memory.x")).unwrap();
    for region in ["AXISRAM", "SRAM1", "SRAM4", "BSRAM", "ITCM"] {
        assert!(memory.contains(region), "{region} is missing:\n{memory}");
    }
    // The pair cortex-m-rt requires must keep its own names.
    assert!(memory.contains("FLASH "), "{memory}");
    assert!(memory.contains("RAM  "), "{memory}");
}

/// A chip with no extra regions must not gain an empty section or stray
/// whitespace: the single-region case is the common one.
#[test]
fn a_chip_without_extra_regions_is_unchanged() {
    let root = scratch("no-regions", &[("solo", Some("stm32f401re"))]);
    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let memory = fs::read_to_string(root.join("firmware/solo/memory.x")).unwrap();
    assert_eq!(
        memory,
        "MEMORY\n{\n  FLASH : ORIGIN = 0x08000000, LENGTH = 512K\n  \
         RAM   : ORIGIN = 0x20000000, LENGTH = 96K\n}\n\n\
         _stext = ORIGIN(FLASH) + 0x198;\n"
    );
}

/// `DEFMT_LOG` ends up in an emitted file, so the firmware has to declare it.
/// A command-line flag would leave that file disagreeing with everything that
/// records why, which is the drift the derived files exist to prevent.
#[test]
fn a_firmware_declares_its_log_filter() {
    let root = repository_root().join("target/project-tests/defmt-log");
    let _ = fs::remove_dir_all(&root);
    let firmware = root.join("firmware/board");
    fs::create_dir_all(&firmware).unwrap();
    fs::write(
        firmware.join("Cargo.toml"),
        "[package]\nname = \"board\"\nversion = \"0.1.0\"\n\n\
         [package.metadata.ferroforge]\nchip = \"stm32f401re\"\n\
         defmt-log = \"warn,chatty_crate=off\"\n\n\
         [dependencies]\n# ferroforge:platform-dependencies\n# ferroforge:end\n",
    )
    .unwrap();

    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let config = fs::read_to_string(firmware.join(".cargo/config.toml")).unwrap();
    assert!(
        config.contains("DEFMT_LOG = \"warn,chatty_crate=off\""),
        "{config}"
    );
}

/// Without one, `info` - the level a person wants when they have just flashed
/// something and want to know whether it works.
#[test]
fn a_firmware_without_a_log_filter_gets_info() {
    let root = scratch("defmt-log-default", &[("solo", Some("stm32f401re"))]);
    let output = ferroforge_in(&root, &["sync"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let config = fs::read_to_string(root.join("firmware/solo/.cargo/config.toml")).unwrap();
    assert!(config.contains("DEFMT_LOG = \"info\""), "{config}");
}

/// What a new user runs first, end to end: `new`, then `build`, on one chip
/// per HAL family. A project that is created but does not build would be the
/// first thing anyone saw of FerroForge.
///
/// Against this checkout rather than the published crate, so it tests the
/// code in front of it.
#[test]
#[ignore = "cross-compiles a scaffolded project per HAL; run with --ignored"]
fn a_new_project_builds_on_every_hal_family() {
    let facade = repository_root().join("ferroforge");
    let facade = facade.to_str().unwrap().replace('\\', "/");
    for chip in ["stm32f401re", "stm32h753zi"] {
        let root = repository_root()
            .join("target/project-tests")
            .join(format!("new-{chip}"));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.parent().unwrap()).unwrap();

        let created = ferroforge_in(
            repository_root(),
            &["new", root.to_str().unwrap(), "--chip", chip, "--ferroforge", &facade],
        );
        assert!(created.status.success(), "{chip}: {}", stderr(&created));

        let built = ferroforge_in(&root, &["build"]);
        assert!(built.status.success(), "{chip}: {}", stderr(&built));
        assert!(
            !stderr(&built).contains("warning:"),
            "{chip}: a new project must build without warnings:\n{}",
            stderr(&built)
        );
    }
}
