//! Rejection evidence.
//!
//! Each case is the working firmware with exactly one deliberate defect, so a
//! failure can only come from the defect: the unmutated copy is checked first as
//! a positive control, and every case asserts the message that names its own
//! reason rather than merely asserting that something went wrong.
//!
//! Mutating a copy rather than keeping four hand-written fixtures means the
//! cases cannot rot into compiling for unrelated reasons - they track whatever
//! the firmware it names actually says.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

/// A defect, the firmware it is planted in, the authored text it replaces, and
/// the reason it must fail for.
struct Case {
    name: &'static str,
    firmware: &'static str,
    from: &'static str,
    to: &'static str,
    expected: &'static str,
}

const BASE: &str = "nucleo-f401re";
const BEACON: &str = "nucleo-f401re-beacon";

const CASES: &[Case] = &[
    // RTIC owns the shape of a bound handler, and its message names the handler.
    Case {
        name: "hardware-task-given-inputs",
        firmware: BASE,
        from: "fn tick(cx: tick::Context);",
        to: "fn tick(cx: tick::Context, value: u32);",
        expected: "this task handler must have type signature `fn(tick::Context)`",
    },
    // `app!` owns this one: the contradiction is visible in the authored
    // declaration, so it does not reach RTIC as a signature complaint.
    Case {
        name: "software-task-given-an-interrupt",
        firmware: BASE,
        from: "from = blink,",
        to: "from = blink, binds = TIM3,",
        expected: "an `async` task cannot bind an interrupt",
    },
    // The composition states the type because the macro never reads the
    // definition; disagreeing with it fails against the `Config` trait.
    Case {
        name: "configuration-type-disagrees-with-definition",
        firmware: BASE,
        from: "config = [period_ms: u32 = 500],",
        to: "config = [period_ms: u64 = 500],",
        expected: "implemented const `PERIOD_MS` has an incompatible type for trait",
    },
    Case {
        name: "binding-names-a-resource-that-does-not-exist",
        firmware: BASE,
        from: "local = [led = status_led, count = blink_count],",
        to: "local = [led = status_led, count = no_such_resource],",
        expected: "this local resource has NOT been declared",
    },
    // A HAL-specific task names the concrete type it needs, so a firmware whose
    // resource is something else fails as ordinary Rust, at the line in `init`
    // that produced the wrong thing.
    Case {
        name: "resource-type-disagrees-with-the-hal-definition",
        firmware: BEACON,
        from: "pulse_timer: CounterUs<TIM3>,",
        to: "pulse_timer: u32,",
        expected: "expected `u32`, found `Counter",
    },
    // A task that needs a clock, in an application that declares no monotonic.
    // The slot is filled with a stand-in named for exactly that.
    Case {
        name: "task-needs-a-monotonic-the-application-lacks",
        firmware: BASE,
        from: "    monotonic = Mono,
",
        to: "",
        expected: "NoMonotonicDeclared",
    },
    // Dispatchers are the author's choice, passed through unchanged - so RTIC's
    // own validation of that choice must still reach the authored line.
    Case {
        name: "dispatcher-is-also-a-bound-interrupt",
        firmware: BASE,
        from: "dispatchers = [USART1]",
        to: "dispatchers = [TIM2]",
        expected: "dispatcher interrupts can't be used as hardware tasks",
    },
    Case {
        name: "too-few-dispatchers-for-the-priorities-used",
        firmware: BEACON,
        from: "dispatchers = [USART2, USART6]",
        to: "dispatchers = [USART2]",
        expected: "not enough interrupts to dispatch all software tasks",
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
fn prepare(firmware: &str, name: &str, mutation: Option<&Case>) -> PathBuf {
    let root = repository_root();
    let source = root.join("firmware").join(firmware);
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

fn check(firmware: &str, directory: &Path) -> (bool, String) {
    let output = Command::new(cargo())
        .args(["check", "--bin", firmware, "--offline"])
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
    for firmware in [BASE, BEACON] {
        let control = prepare(firmware, &format!("control-{firmware}"), None);
        let (succeeded, stderr) = check(firmware, &control);
        assert!(
            succeeded,
            "the unmutated copy of {firmware} must check, \
             or every rejection below proves nothing:\n{stderr}"
        );
    }

    for case in CASES {
        let directory = prepare(case.firmware, case.name, Some(case));
        let (succeeded, stderr) = check(case.firmware, &directory);

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
