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
    assert!(root.join("tasks/heartbeat/src/lib.rs").is_file());

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

/// A fresh project to add to, made by `new` so the two commands are tested
/// against each other rather than against a fixture.
fn created(name: &str, extra: &[&str]) -> PathBuf {
    let root = repository_root().join("target/project-tests").join(name);
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.parent().unwrap()).unwrap();
    let mut arguments = vec!["new", root.to_str().unwrap()];
    arguments.extend(extra);
    let output = ferroforge_in(repository_root(), &arguments);
    assert!(output.status.success(), "{}", stderr(&output));
    root
}

fn tasks_in(root: &Path) -> Vec<String> {
    let mut found = fs::read_dir(root.join("tasks"))
        .unwrap()
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    found.sort();
    found
}

/// `add` writes a firmware and nothing else: it selects the task `new` wrote
/// rather than writing another, and the convention recognizes the result.
#[test]
fn an_added_firmware_reuses_the_projects_task() {
    let root = created("added", &[]);
    let tasks_before = tasks_in(&root);

    // From a subdirectory, because the project is found the usual way.
    let output = ferroforge_in(
        &root.join("tasks"),
        &["add", "second", "--chip", "stm32h753zi"],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("firmware/second/"),
        "{}",
        stdout(&output)
    );

    let firmware = root.join("firmware/second");
    for file in [
        "Cargo.toml",
        "src/main.rs",
        "memory.x",
        ".cargo/config.toml",
        "Embed.toml",
    ] {
        assert!(firmware.join(file).is_file(), "{file} was not written");
    }
    assert_eq!(tasks_in(&root), tasks_before, "`add` must not create tasks");

    let manifest = fs::read_to_string(firmware.join("Cargo.toml")).unwrap();
    assert!(manifest.contains("chip = \"stm32h753zi\""), "{manifest}");
    assert!(manifest.contains("\"stm32h753v\""), "{manifest}");
    assert!(
        manifest.contains("heartbeat = { path = \"../../tasks/heartbeat\" }"),
        "{manifest}"
    );
    assert!(manifest.contains("ferroforge = \"0.1\""), "{manifest}");
    let main = fs::read_to_string(firmware.join("src/main.rs")).unwrap();
    assert!(main.contains("use heartbeat::heartbeat;"), "{main}");
    assert!(main.contains("stm32h7xx_hal::pac"), "{main}");

    // Already in the state `sync` would leave it.
    let before = fs::read_to_string(firmware.join("memory.x")).unwrap();
    let synced = ferroforge_in(&root, &["sync", "second"]);
    assert!(synced.status.success(), "{}", stderr(&synced));
    assert_eq!(
        fs::read_to_string(firmware.join("memory.x")).unwrap(),
        before
    );
}

/// A project made against a checkout must stay on it, without repeating the
/// flag for every firmware added.
#[test]
fn an_added_firmware_depends_on_ferroforge_as_the_project_does() {
    let root = created("added-checkout", &["--ferroforge", "/some/checkout"]);
    let output = ferroforge_in(&root, &["add", "second", "--chip", "stm32f411re"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let manifest = fs::read_to_string(root.join("firmware/second/Cargo.toml")).unwrap();
    assert!(
        manifest.contains("ferroforge = { path = \"/some/checkout\" }"),
        "{manifest}"
    );

    // And the flag still wins when given.
    let output = ferroforge_in(
        &root,
        &[
            "add",
            "third",
            "--chip",
            "stm32f411re",
            "--ferroforge",
            "/other",
        ],
    );
    assert!(output.status.success(), "{}", stderr(&output));
    let manifest = fs::read_to_string(root.join("firmware/third/Cargo.toml")).unwrap();
    assert!(
        manifest.contains("ferroforge = { path = \"/other\" }"),
        "{manifest}"
    );
}

/// Every refusal happens before anything is written, and says why.
#[test]
fn a_refused_add_writes_nothing() {
    let root = created("add-refused", &[]);
    let cases: &[(&[&str], &str)] = &[
        (
            &["add", "add-refused", "--chip", "stm32f401re"],
            "already exists",
        ),
        (&["add", "board", "--chip", "stm32f999zz"], "stm32f401re"),
        (&["add", "board"], "--chip"),
        (
            &["add", "2board", "--chip", "stm32f401re"],
            "start with a letter",
        ),
        (
            &["add", "my.board", "--chip", "stm32f401re"],
            "letters, digits",
        ),
        (
            &["add", "--chip", "stm32f401re"],
            "expected a firmware name",
        ),
    ];
    for (arguments, expected) in cases {
        let output = ferroforge_in(&root, arguments);
        assert!(!output.status.success(), "{arguments:?} succeeded");
        let message = stderr(&output);
        assert!(message.contains(expected), "{arguments:?}: {message}");
    }
    let firmwares = fs::read_dir(root.join("firmware")).unwrap().count();
    assert_eq!(firmwares, 1, "a refused add left a firmware behind");
}

#[test]
fn adding_outside_a_project_says_so() {
    let elsewhere = std::env::temp_dir().join("ferroforge-add-not-a-project");
    let _ = fs::remove_dir_all(&elsewhere);
    fs::create_dir_all(&elsewhere).unwrap();
    let output = ferroforge_in(&elsewhere, &["add", "board", "--chip", "stm32f401re"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("not inside a FerroForge project"),
        "{}",
        stderr(&output)
    );
    assert!(!elsewhere.join("firmware").exists());
}

/// The table is for a person choosing a chip, so it has to carry what they
/// choose by, for every chip, from the same data a build uses.
#[test]
fn the_chip_table_describes_every_chip() {
    let output = ferroforge_in(repository_root(), &["chips"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let table = stdout(&output);
    let names = stdout(&ferroforge_in(repository_root(), &["chips", "--names"]));
    assert_eq!(table.lines().count(), names.lines().count() + 1, "{table}");
    assert!(table.starts_with("CHIP"), "{table}");
    let h7 = table
        .lines()
        .find(|line| line.starts_with("stm32h753zi"))
        .unwrap();
    assert!(h7.contains("stm32h7xx-hal") && h7.contains("2048K"), "{h7}");
}

/// The task crate is always `tasks/heartbeat`, so a project of that name would
/// be a firmware depending on a package with its own name.
#[test]
fn a_project_named_like_the_task_crate_is_refused() {
    let root = repository_root().join("target/project-tests/heartbeat");
    let _ = fs::remove_dir_all(&root);
    let output = ferroforge_in(repository_root(), &["new", root.to_str().unwrap()]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("task crate's name"),
        "{}",
        stderr(&output)
    );
    assert!(
        !root.exists(),
        "a refused name must not leave half a project behind"
    );
}

/// A stand-in for Cargo, so `--all` can be tested on outcomes rather than on
/// cross-compiles: it fails in a firmware named `bad*`, warns in one named
/// `noisy*`, and records the arguments it was given.
#[cfg(unix)]
fn fake_cargo(root: &Path) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;
    let script = root.join("fake-cargo");
    fs::write(
        &script,
        "#!/bin/sh\n\
         echo \"$(basename \"$PWD\") $*\" >> \"$(dirname \"$0\")/cargo.log\"\n\
         case \"$(basename \"$PWD\")\" in\n\
         bad*) echo 'error: it broke' >&2; exit 101 ;;\n\
         noisy*) echo 'warning: unused variable' >&2 ;;\n\
         esac\n\
         echo '    Finished' >&2\n",
    )
    .unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    script
}

#[cfg(unix)]
fn ferroforge_with_cargo(root: &Path, arguments: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ferroforge"))
        .args(arguments)
        .current_dir(root)
        .env("CARGO", fake_cargo(root))
        .env_remove("NO_COLOR")
        .output()
        .expect("the CLI binary must run")
}

/// Every firmware is attempted, a failure does not stop the ones after it, and
/// each gets the status it earned. Output is shown for a warning or a failure
/// and not for a clean build.
#[cfg(unix)]
#[test]
fn build_all_reports_every_firmware() {
    let root = scratch(
        "all-outcomes",
        &[
            ("alpha", Some("stm32f401re")),
            ("bad", Some("stm32f401re")),
            ("noisy", Some("stm32f411re")),
        ],
    );
    let output = ferroforge_with_cargo(&root, &["build", "--all", "--", "--locked"]);
    assert!(!output.status.success(), "a failure must fail the command");
    let text = stdout(&output);
    let status = |name: &str| {
        text.lines()
            .find(|line| line.starts_with(&format!("firmware/{name} ")))
            .unwrap_or_else(|| panic!("no line for {name}:\n{text}"))
            .split_whitespace()
            .last()
            .unwrap()
            .to_owned()
    };
    assert_eq!(status("alpha"), "ok", "{text}");
    assert_eq!(status("bad"), "FAILED", "{text}");
    assert_eq!(status("noisy"), "warn", "{text}");
    assert!(text.contains("    error: it broke"), "{text}");
    assert!(text.contains("    warning: unused variable"), "{text}");
    assert_eq!(
        text.matches("Finished").count(),
        1,
        "only the warned firmware's output is shown:\n{text}"
    );
    assert!(text.contains("2 of 3 built"), "{text}");
    assert!(text.contains("warnings in noisy"), "{text}");
    assert!(text.contains("failed: bad"), "{text}");
    assert!(
        !text.contains('\x1b'),
        "no colour when not a terminal:\n{text}"
    );

    // Release builds, with the forwarded arguments, in every firmware.
    let log = fs::read_to_string(root.join("cargo.log")).unwrap();
    for name in ["alpha", "bad", "noisy"] {
        assert!(
            log.contains(&format!("{name} build --release --locked")),
            "{log}"
        );
    }
}

/// Warnings alone are not a failure.
#[cfg(unix)]
#[test]
fn warnings_alone_succeed() {
    let root = scratch(
        "all-warned",
        &[
            ("alpha", Some("stm32f401re")),
            ("noisy", Some("stm32f401re")),
        ],
    );
    let output = ferroforge_with_cargo(&root, &["check", "--all"]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(
        stdout(&output).contains("2 of 2 checked"),
        "{}",
        stdout(&output)
    );
    let log = fs::read_to_string(root.join("cargo.log")).unwrap();
    assert!(
        log.contains("alpha check\n"),
        "`check` is not a release build: {log}"
    );
}

/// A firmware that cannot even be synced is that firmware's failure, and the
/// rest still run.
#[test]
fn a_firmware_that_cannot_sync_fails_alone() {
    let root = scratch(
        "all-sync",
        &[
            ("alpha", Some("stm32f999zz")),
            ("beta", Some("stm32f401re")),
        ],
    );
    let output = ferroforge_in(&root, &["sync", "--all"]);
    assert!(!output.status.success());
    let text = stdout(&output);
    assert!(text.contains("stm32f999zz"), "{text}");
    assert!(text.contains("1 of 2 synced"), "{text}");
    assert!(root.join("firmware/beta/memory.x").is_file());
}

#[test]
fn all_is_refused_where_it_does_not_apply() {
    let root = scratch("all-refused", &[("alpha", Some("stm32f401re"))]);
    let run = ferroforge_in(&root, &["run", "--all"]);
    assert!(!run.status.success());
    assert!(
        stderr(&run).contains("sync, check and build"),
        "{}",
        stderr(&run)
    );

    let both = ferroforge_in(&root, &["build", "alpha", "--all"]);
    assert!(!both.status.success());
    assert!(stderr(&both).contains("not both"), "{}", stderr(&both));
}

/// The real thing, against this repository's firmware.
#[test]
#[ignore = "checks every firmware in this repository; run with --ignored"]
fn check_all_passes_on_the_real_project() {
    let output = ferroforge_in(repository_root(), &["check", "--all"]);
    assert!(output.status.success(), "{}", stdout(&output));
    assert!(!stdout(&output).contains("warn"), "{}", stdout(&output));
}

/// A project whose firmwares have the given `src/main.rs` contents.
fn with_sources(name: &str, sources: &[(&str, &str)]) -> PathBuf {
    let firmwares = sources
        .iter()
        .map(|(firmware, _)| (*firmware, Some("stm32f401re")))
        .collect::<Vec<_>>();
    let root = scratch(name, &firmwares);
    for (firmware, source) in sources {
        fs::write(
            root.join("firmware").join(firmware).join("src/main.rs"),
            source,
        )
        .unwrap();
    }
    root
}

fn status_of<'a>(text: &'a str, region: &str) -> &'a str {
    text.lines()
        .find(|line| line.trim_start().starts_with(&format!("{region} ")))
        .unwrap_or_else(|| panic!("no line for {region}:\n{text}"))
        .split_whitespace()
        .nth(1)
        .unwrap()
}

/// Copies that differ only in layout and notes are the same code.
#[test]
fn copies_that_differ_only_in_layout_are_the_same() {
    let root = with_sources(
        "drift-same",
        &[
            (
                "alpha",
                "fn a() {\n    // ferroforge:begin control\n    let x = 1;\n    run(x);\n    // ferroforge:end control\n}\n",
            ),
            (
                "beta",
                "mod m {\n        // ferroforge:begin control\n\n        // tuned for beta's board\n        let x = 1;\n        run(x);\n        // ferroforge:end control\n}\n",
            ),
        ],
    );
    let output = ferroforge_in(&root, &["drift"]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(status_of(&text, "control"), "same", "{text}");
}

/// Drift names the region, both files, and the differing lines at the line
/// numbers someone would open.
#[test]
fn a_changed_copy_is_drift_with_the_lines_that_differ() {
    let root = with_sources(
        "drift-changed",
        &[
            (
                "alpha",
                "// ferroforge:begin control\nlet gain = 2;\nrun(gain);\n// ferroforge:end control\n",
            ),
            (
                "beta",
                "\n// ferroforge:begin control\nlet gain = 3;\nrun(gain);\n// ferroforge:end control\n",
            ),
        ],
    );
    let output = ferroforge_in(&root, &["drift"]);
    let text = stdout(&output);
    assert!(
        !output.status.success(),
        "drift must fail the command:\n{text}"
    );
    assert_eq!(status_of(&text, "control"), "DRIFT", "{text}");
    assert!(
        text.contains("firmware/alpha/src/main.rs (alpha) vs firmware/beta/src/main.rs (beta)"),
        "{text}"
    );
    assert!(text.contains("-    2  let gain = 2;"), "{text}");
    assert!(text.contains("+    3  let gain = 3;"), "{text}");
    assert!(
        !text.contains("run(gain)"),
        "unchanged lines are not shown:\n{text}"
    );
    assert!(text.contains("drift in control"), "{text}");
}

/// Nested regions are compared on their own, outer first, so drift in the
/// outer one is narrowed down by the inner results.
#[test]
fn nested_regions_are_reported_outer_first() {
    let source = |tail: &str| {
        format!(
            "// ferroforge:begin outer\n\
             setup();\n\
             // ferroforge:begin inner\n\
             step();\n\
             // ferroforge:end inner\n\
             {tail}\n\
             // ferroforge:end outer\n"
        )
    };
    let root = with_sources(
        "drift-nested",
        &[
            ("alpha", &source("finish(1);")),
            ("beta", &source("finish(2);")),
        ],
    );
    let output = ferroforge_in(&root, &["drift"]);
    let text = stdout(&output);
    assert_eq!(status_of(&text, "outer"), "DRIFT", "{text}");
    assert_eq!(status_of(&text, "inner"), "same", "{text}");
    let outer = text.find("outer ").unwrap();
    let inner = text
        .find("  inner ")
        .expect("inner is indented under outer");
    assert!(outer < inner, "outer must come first:\n{text}");
}

/// A region only one firmware marks has nothing to drift from. It is shown,
/// because a misspelt name looks exactly like this, but it is not a failure.
#[test]
fn a_region_in_one_firmware_is_alone_not_drift() {
    let root = with_sources(
        "drift-alone",
        &[
            (
                "alpha",
                "// ferroforge:begin control\nx();\n// ferroforge:end control\n",
            ),
            (
                "beta",
                "// ferroforge:begin contrl\nx();\n// ferroforge:end contrl\n",
            ),
        ],
    );
    let output = ferroforge_in(&root, &["drift"]);
    let text = stdout(&output);
    assert!(output.status.success(), "{text}");
    assert_eq!(status_of(&text, "control"), "alone", "{text}");
    assert!(text.contains("only in alpha"), "{text}");
    assert!(text.contains("only in beta"), "{text}");
}

/// Markers that cannot be read are errors with the line to fix, and nothing is
/// compared: drift reported past a broken marker would only be the marker.
#[test]
fn broken_markers_are_errors_at_their_line() {
    let cases: &[(&str, &str)] = &[
        (
            "x();\n// ferroforge:begin control\ny();\n",
            "main.rs:2: `ferroforge:begin control` has no `ferroforge:end control`",
        ),
        (
            "// ferroforge:end control\n",
            "main.rs:1: `ferroforge:end control` has no `ferroforge:begin control` before it",
        ),
        (
            "// ferroforge:begin a\n// ferroforge:begin b\n// ferroforge:end a\n// ferroforge:end b\n",
            "main.rs:3: `ferroforge:end a` closes `a`, but `b` is still open",
        ),
        (
            "// ferroforge:begin a\n// ferroforge:end a\n// ferroforge:begin a\n// ferroforge:end a\n",
            "main.rs:3: `a` is already marked at firmware/alpha/src/main.rs:1",
        ),
        (
            "// ferroforge:begin\n",
            "main.rs:1: a marker needs a region name",
        ),
    ];
    for (index, (source, expected)) in cases.iter().enumerate() {
        let root = with_sources(
            &format!("drift-broken-{index}"),
            &[("alpha", source), ("beta", "")],
        );
        let output = ferroforge_in(&root, &["drift"]);
        let text = stdout(&output);
        assert!(!output.status.success(), "{source}");
        assert!(text.contains(expected), "expected `{expected}` in:\n{text}");
        assert_eq!(text.lines().count(), 1, "one mistake, one error:\n{text}");
    }
}

#[test]
fn a_project_without_markers_says_how_to_add_them() {
    let root = with_sources("drift-none", &[("alpha", "fn main() {}\n")]);
    let output = ferroforge_in(&root, &["drift"]);
    assert!(output.status.success());
    assert!(
        stdout(&output).contains("ferroforge:begin"),
        "{}",
        stdout(&output)
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
    let output = ferroforge_in(repository_root(), &["chips", "--names"]);
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
            &[
                "new",
                root.to_str().unwrap(),
                "--chip",
                chip,
                "--ferroforge",
                &facade,
            ],
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

/// `add` on the other HAL family from the project's first firmware, then
/// `build`: the added firmware must build against the task `new` wrote.
#[test]
#[ignore = "cross-compiles an added firmware per HAL; run with --ignored"]
fn an_added_firmware_builds_on_every_hal_family() {
    let facade = repository_root().join("ferroforge");
    let facade = facade.to_str().unwrap().replace('\\', "/");
    let root = repository_root().join("target/project-tests/add-build");
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.parent().unwrap()).unwrap();
    let created = ferroforge_in(
        repository_root(),
        &[
            "new",
            root.to_str().unwrap(),
            "--chip",
            "stm32f401re",
            "--ferroforge",
            &facade,
        ],
    );
    assert!(created.status.success(), "{}", stderr(&created));

    for chip in ["stm32f411re", "stm32h753zi"] {
        let name = format!("on-{chip}");
        let added = ferroforge_in(&root, &["add", &name, "--chip", chip]);
        assert!(added.status.success(), "{chip}: {}", stderr(&added));

        let built = ferroforge_in(&root, &["build", &name]);
        assert!(built.status.success(), "{chip}: {}", stderr(&built));
        assert!(
            !stderr(&built).contains("warning:"),
            "{chip}: an added firmware must build without warnings:\n{}",
            stderr(&built)
        );
    }
}
