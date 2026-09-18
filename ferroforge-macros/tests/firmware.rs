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

/// Shapes the example firmware does not use, compiled for real rather than
/// asserted from an expansion: spawn aliases taking zero, one and two inputs,
/// a timestamp from `Mono::now()`, configuration read inside a macro call,
/// local resources whose initial values the definition owns beside one the
/// firmware supplies, a shared resource known only by a trait bound, and a
/// lock-free shared resource used by two interrupt handlers at one priority -
/// by type in one and by bound in the other. Each was a defect that expanded cleanly
/// and failed only in the compiler, which is why this builds instead.
///
/// Defined and selected in one crate, on the F401RE firmware's manifest and
/// lock file, so it adds no example task crate and builds offline.
const SHAPES: &str = r#"
#![no_std]
#![no_main]

use defmt_rtt as _;
use panic_probe as _;

#[ferroforge::task(
    spawn = [takes_none(), takes_one(value: u32), takes_two(a: u32, b: bool)],
    config = [period_ms: u32],
    monotonic = Mono,
)]
pub async fn source(cx: source::Context) {
    defmt::info!("period {=u32}", CONFIG::PERIOD_MS);
    let _: Result<(), ()> = cx.spawn.takes_none();
    let _: Result<(), u32> = cx.spawn.takes_one(1);
    let _: Result<(), (u32, bool)> = cx.spawn.takes_two(1, true);
    let _: u32 = Mono::now().duration_since_epoch().to_micros();
}

#[ferroforge::task(
    local = [
        page: Option<[u8; 4]> = None,
        retries: u8 = 3,
        count: u32,
    ],
)]
pub async fn takes_none(cx: takes_none::Context) {
    *cx.local.retries -= 1;
    *cx.local.page = Some([0; 4]);
    *cx.local.count += u32::from(*cx.local.retries);
}

#[ferroforge::task]
pub async fn takes_one(_cx: takes_one::Context, _value: u32) {}

#[ferroforge::task]
pub async fn takes_two(_cx: takes_two::Context, _a: u32, _b: bool) {}

pub trait Sink {
    fn put(&mut self, value: u8);
}

pub struct Counter(pub u32);

impl Sink for Counter {
    fn put(&mut self, value: u8) {
        self.0 += u32::from(value);
    }
}

#[ferroforge::task(bounds = [out: Sink], shared = [out])]
pub async fn writer(mut cx: writer::Context) {
    cx.shared.out.lock(|out| out.put(1));
}

#[ferroforge::task(shared = [#[lock_free] rx: Counter])]
pub fn on_dma(cx: on_dma::Context) {
    cx.shared.rx.put(1);
}

#[ferroforge::task(bounds = [rx: Sink], shared = [#[lock_free] rx])]
pub fn on_idle(cx: on_idle::Context) {
    cx.shared.rx.put(2);
}

ferroforge::app! {
    device = stm32f4xx_hal::pac,
    dispatchers = [USART1],

    use rtic_monotonics::systick::prelude::*;
    systick_monotonic!(Mono, 1000);

    use super::{Counter, on_dma, on_idle, source, takes_none, takes_one, takes_two, writer};

    #[shared]
    struct Shared {
        sink: Counter,
        #[lock_free]
        uart_rx: Counter,
    }

    #[local]
    struct Local {
        zero_count: u32,
    }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        Mono::start(cx.core.SYST, 16_000_000);
        first::spawn().unwrap();
        (
            Shared {
                sink: Counter(0),
                uart_rx: Counter(0),
            },
            Local { zero_count: 0 },
        )
    }

    #[task(
        from = source,
        priority = 1,
        spawn = [takes_none = zero, takes_one = one, takes_two = two],
        config = [period_ms: u32 = 5],
    )]
    async fn first(cx: first::Context);

    // `page` and `retries` are the definition's; only `count` is bound here.
    #[task(from = takes_none, priority = 1, local = [count = zero_count])]
    async fn zero(_cx: zero::Context);

    #[task(from = writer, priority = 1, shared = [out = sink])]
    async fn write(cx: write::Context);

    #[task(from = on_dma, binds = EXTI0, priority = 2, shared = [rx = uart_rx])]
    fn dma(cx: dma::Context);

    #[task(from = on_idle, binds = EXTI1, priority = 2, shared = [rx = uart_rx])]
    fn idle_line(cx: idle_line::Context);

    #[task(from = takes_one, priority = 1)]
    async fn one(cx: one::Context, value: u32);

    #[task(from = takes_two, priority = 1)]
    async fn two(cx: two::Context, a: u32, b: bool);
}
"#;

#[test]
#[ignore = "cross-compiles; run with --ignored"]
fn shapes_the_example_firmware_does_not_use_compile() {
    let root = repository_root();
    let source = root.join("firmware/nucleo-f401re");
    let directory = root.join("target/fixtures/shapes");
    let _ = std::fs::remove_dir_all(&directory);
    std::fs::create_dir_all(directory.join("src")).unwrap();
    std::fs::create_dir_all(directory.join(".cargo")).unwrap();
    for file in ["memory.x", "Cargo.lock", ".cargo/config.toml"] {
        std::fs::copy(source.join(file), directory.join(file)).unwrap();
    }
    // The copy sits at a different depth, so its path dependencies are made
    // absolute - with forward slashes, which a TOML string reads literally.
    let absolute = root.display().to_string().replace('\\', "/");
    let manifest = std::fs::read_to_string(source.join("Cargo.toml"))
        .unwrap()
        .replace("path = \"../../", &format!("path = \"{absolute}/"))
        // A task that declares a monotonic names `fugit` in its bound, and
        // these definitions live in the firmware crate, so it needs the
        // dependency a task crate would have. Already in the lock file.
        .replacen("[dependencies]\n", "[dependencies]\nfugit = \"0.3\"\n", 1);
    std::fs::write(directory.join("Cargo.toml"), manifest).unwrap();
    std::fs::write(directory.join("src/main.rs"), SHAPES).unwrap();

    let output = Command::new(cargo())
        .args(["check", "--bin", "nucleo-f401re", "--offline"])
        .current_dir(&directory)
        .env("CARGO_TARGET_DIR", root.join("target/fixtures/shared"))
        .output()
        .expect("cargo must be runnable");
    assert!(
        output.status.success(),
        "every spawn arity, `Mono::now()`, `CONFIG` in a macro, task-owned \
         locals, a bounded shared resource and lock-free sharing must \
         compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
