use std::{
    env, fs,
    path::{Path, PathBuf},
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

fn cargo() -> std::ffi::OsString {
    env::var_os("CARGO").unwrap_or_else(|| "cargo".into())
}

fn fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sw")
}

fn rtic_layout_fixture() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/rtic-layout")
}

#[test]
fn standalone_sw_fixture_checks_in_its_own_workspace() {
    let status = Command::new(cargo())
        .args(["check", "--lib", "--locked", "--offline"])
        .current_dir(fixture())
        .status()
        .unwrap();
    assert!(status.success(), "standalone SW fixture must check");
}

#[test]
fn real_rtic_layout_preserves_private_helpers_and_shared_type_identity() {
    let status = Command::new(cargo())
        .args([
            "check",
            "--bin",
            "ferroforge-rtic-layout-fixture",
            "--target",
            "thumbv7em-none-eabihf",
            "--locked",
            "--offline",
        ])
        .current_dir(rtic_layout_fixture())
        .status()
        .unwrap();
    assert!(status.success(), "real RTIC layout fixture must check");
}

#[test]
#[ignore = "requires the rust-analyzer CLI; see the mdBook workflow"]
fn rust_analyzer_reports_errors_on_authored_task_body_lines() {
    let fixture = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/ra-diagnostics");
    let output = Command::new("rust-analyzer")
        .args(["diagnostics", ".", "--severity", "error"])
        .env("CARGO_TARGET_DIR", fixture.join("target"))
        .env("RUST_BACKTRACE", "0")
        .current_dir(&fixture)
        .output()
        .expect("rust-analyzer must be installed to run this ignored test");

    assert!(
        !output.status.success(),
        "the intentionally invalid fixture produced no Rust Analyzer errors"
    );

    let diagnostics = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    for (zero_based_line, expected) in [
        (8, "no method `missing` on type"),
        (13, "expected u32, found bool"),
    ] {
        assert!(
            diagnostics.contains("ra-diagnostics\\src\\lib.rs")
                || diagnostics.contains("ra-diagnostics/src/lib.rs"),
            "expected the authored fixture path in:\n{diagnostics}"
        );
        assert!(
            diagnostics.contains(&format!("LineCol {{ line: {zero_based_line},")),
            "expected a diagnostic on authored zero-based line {zero_based_line}:\n{diagnostics}"
        );
        assert!(
            diagnostics.contains(expected),
            "expected diagnostic `{expected}`:\n{diagnostics}"
        );
    }
}

struct CompileFailPackage {
    root: PathBuf,
}

impl CompileFailPackage {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = env::temp_dir().join(format!(
            "ferroforge-standalone-check-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("src")).unwrap();
        let ferroforge = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("ferroforge")
            .canonicalize()
            .unwrap()
            .display()
            .to_string();
        let ferroforge = ferroforge
            .strip_prefix(r"\\?\")
            .unwrap_or(&ferroforge)
            .replace('\\', "/");
        fs::write(
            root.join("Cargo.toml"),
            format!(
                "[package]\nname = \"standalone-compile-fail\"\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\nferroforge = {{ path = \"{ferroforge}\" }}\n\n[workspace]\n"
            ),
        )
        .unwrap();
        Self { root }
    }

    fn check_failure(&self, source: &str, expected: &str, expected_line: usize) {
        fs::write(self.root.join("src/lib.rs"), source).unwrap();
        let output = Command::new(cargo())
            .args(["check", "--lib", "--offline", "--color", "never"])
            .env("CARGO_TARGET_DIR", self.root.join("target"))
            .current_dir(&self.root)
            .output()
            .unwrap();
        assert!(!output.status.success(), "source unexpectedly checked");
        let diagnostics = String::from_utf8_lossy(&output.stderr);
        assert!(
            diagnostics.contains(expected),
            "expected diagnostic `{expected}` in:\n{diagnostics}"
        );
        assert!(
            diagnostics.contains(&format!("src/lib.rs:{expected_line}:"))
                || diagnostics.contains(&format!("src\\lib.rs:{expected_line}:")),
            "expected the diagnostic to point to authored src/lib.rs line {expected_line}:\n{diagnostics}"
        );
    }
}

impl Drop for CompileFailPackage {
    fn drop(&mut self) {
        // Only this test-owned unique directory is removed.
        let _ = fs::remove_dir_all(&self.root);
    }
}

#[test]
fn standalone_contexts_reject_invalid_supported_uses() {
    let package = CompileFailPackage::new();
    for (source, expected, expected_line) in [
        (
            "use ferroforge::task;\ntrait Pin { fn toggle(&mut self); }\n#[task(bounds = [led: Pin], local = [led])]\nasync fn run(mut cx: run::Context) { cx.local.led.missing(); }\n",
            "no method named `missing`",
            4,
        ),
        (
            "use ferroforge::task;\n#[task(local = [count: u32])]\nasync fn run(mut cx: run::Context) { *cx.local.count = true; }\n",
            "expected `u32`, found `bool`",
            3,
        ),
        (
            "use ferroforge::task;\n#[task(shared = [enabled: bool])]\nasync fn run(mut cx: run::Context) { let escaped = cx.shared.enabled.lock(|value| value); let _ = escaped; }\n",
            "lifetime may not live long enough",
            3,
        ),
        (
            "use ferroforge::task;\n#[task(spawn = [report(value: u32)])]\nasync fn run(cx: run::Context) { let _ = cx.spawn.report(); }\n",
            "this method takes 1 argument but 0 arguments were supplied",
            3,
        ),
        (
            "use ferroforge::task;\n#[task(config = [period_ms: u32])]\nasync fn run(cx: run::Context) { let _: bool = CONFIG.PERIOD_MS; }\n",
            "expected `bool`, found `u32`",
            3,
        ),
        (
            "use ferroforge::task;\n#[task(config = [period_ms: u32])]\nasync fn run(cx: run::Context) { let _ = CONFIG.UNKNOWN; }\n",
            "no field `UNKNOWN`",
            3,
        ),
        (
            "use ferroforge::task;\n#[task(config = [period_ms: u32])]\nasync fn run(cx: run::Context) { let CONFIG = 1_u32; let _ = CONFIG; }\n",
            "`CONFIG` is reserved for standalone task configuration",
            3,
        ),
        (
            "use ferroforge::{mock::systick::Mono, task, MillisDurationU64};\n#[task(monotonic = Mono)]\nasync fn run(cx: run::Context) { Mono::delay(MillisDurationU64::from_ticks(1)).await; }\n",
            "expected `Duration<u32,",
            3,
        ),
    ] {
        package.check_failure(source, expected, expected_line);
    }
}
