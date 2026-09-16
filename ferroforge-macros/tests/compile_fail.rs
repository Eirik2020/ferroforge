//! Rejection evidence.
//!
//! Each case is the working firmware with exactly one deliberate defect, so a
//! failure can only come from the defect: the unmutated copy is checked first as
//! a positive control, and every case asserts the message that names its own
//! reason rather than merely asserting that something went wrong.
//!
//! Mutating a copy rather than keeping four hand-written fixtures means the
//! cases cannot rot into compiling for unrelated reasons - they track whatever
//! `firmware/nucleo-f401re` actually says.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// A defect, the authored text it replaces, and the reason it must fail for.
struct Case {
    name: &'static str,
    from: &'static str,
    to: &'static str,
    expected: &'static str,
}

const CASES: &[Case] = &[
    // RTIC owns the shape of a bound handler, and its message names the handler.
    Case {
        name: "hardware-task-given-inputs",
        from: "fn tick(cx: tick::Context);",
        to: "fn tick(cx: tick::Context, value: u32);",
        expected: "this task handler must have type signature `fn(tick::Context)`",
    },
    // `compose!` owns this one: the contradiction is visible in the authored
    // declaration, so it does not reach RTIC as a signature complaint.
    Case {
        name: "software-task-given-an-interrupt",
        from: "from = blink,",
        to: "from = blink, binds = TIM3,",
        expected: "an `async` task cannot bind an interrupt",
    },
    // The composition states the type because the macro never reads the
    // definition; disagreeing with it fails against the `Config` trait.
    Case {
        name: "configuration-type-disagrees-with-definition",
        from: "config = [period_ms: u32 = 500],",
        to: "config = [period_ms: u64 = 500],",
        expected: "implemented const `PERIOD_MS` has an incompatible type for trait",
    },
    Case {
        name: "binding-names-a-resource-that-does-not-exist",
        from: "local = [led = status_led, count = blink_count],",
        to: "local = [led = status_led, count = no_such_resource],",
        expected: "this local resource has NOT been declared",
    },
];

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the macro crate sits directly below the repository root")
        .to_path_buf()
}

/// Copy the firmware beside the real one and apply one replacement. Path
/// dependencies are made absolute because the copy sits at a different depth.
fn prepare(name: &str, mutation: Option<&Case>) -> PathBuf {
    let root = repository_root();
    let source = root.join("firmware/nucleo-f401re");
    let directory = root.join("target/compile-fail").join(name);

    let _ = fs::remove_dir_all(&directory);
    fs::create_dir_all(directory.join("src")).unwrap();
    fs::create_dir_all(directory.join(".cargo")).unwrap();

    for file in ["memory.x", "Cargo.lock", ".cargo/config.toml"] {
        fs::copy(source.join(file), directory.join(file))
            .unwrap_or_else(|error| panic!("copying {file}: {error}"));
    }

    // Forward slashes: a Windows path in a TOML basic string would read its
    // separators as escape sequences.
    let absolute = root.display().to_string().replace('\\', "/");
    let manifest = fs::read_to_string(source.join("Cargo.toml"))
        .unwrap()
        .replace("path = \"../../", &format!("path = \"{absolute}/"));
    fs::write(directory.join("Cargo.toml"), manifest).unwrap();

    let mut main = fs::read_to_string(source.join("src/main.rs")).unwrap();
    if let Some(case) = mutation {
        assert!(
            main.contains(case.from),
            "`{}` no longer appears in the firmware, so case `{}` would test nothing",
            case.from,
            case.name
        );
        main = main.replacen(case.from, case.to, 1);
    }
    fs::write(directory.join("src/main.rs"), main).unwrap();

    directory
}

fn check(directory: &Path) -> (bool, String) {
    let output = Command::new(cargo())
        .args(["check", "--bin", "nucleo-f401re", "--offline"])
        .current_dir(directory)
        // One target directory across every case, so the dependency tree is
        // built once. The cases run in sequence for the same reason: concurrent
        // cargo invocations would queue on the build lock instead.
        .env(
            "CARGO_TARGET_DIR",
            repository_root().join("target/compile-fail/shared"),
        )
        .output()
        .expect("cargo must be runnable");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

/// Signs that a case failed for an uninteresting reason. Without this, deleting
/// a dependency would turn every case green. The control above catches the same
/// breakage first; this is the net for anything that breaks only one case.
///
/// These are messages observed by deliberately breaking the harness, not
/// guesses at what Cargo might say.
fn is_broken_run(stderr: &str) -> Option<&'static str> {
    [
        "no matching package",
        "failed to load manifest",
        "can't find crate",
        "could not find `Cargo.toml`",
        "The system cannot find the",
    ]
    .into_iter()
    .find(|symptom| stderr.contains(symptom))
}

/// One test, run in sequence: the positive control must pass before any
/// rejection is believed, and sharing a target directory across cases only
/// works without concurrent cargo invocations.
#[test]
#[ignore = "cross-compiles the firmware once per case; run with --ignored"]
fn each_rejection_fails_for_its_own_reason() {
    let control = prepare("control", None);
    let (succeeded, stderr) = check(&control);
    assert!(
        succeeded,
        "the unmutated copy must check, or every rejection below proves nothing:\n{stderr}"
    );

    for case in CASES {
        let directory = prepare(case.name, Some(case));
        let (succeeded, stderr) = check(&directory);

        assert!(
            !succeeded,
            "`{}` compiled but must not:\n{stderr}",
            case.name
        );
        if let Some(symptom) = is_broken_run(&stderr) {
            panic!(
                "`{}` failed on `{symptom}`, not on its defect:\n{stderr}",
                case.name
            );
        }
        assert!(
            stderr.contains(case.expected),
            "`{}` must fail with `{}`, but reported:\n{stderr}",
            case.name,
            case.expected
        );
    }
}
