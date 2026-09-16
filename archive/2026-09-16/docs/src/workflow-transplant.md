# workflow - Archived 2026-09-16 (transplant design)

Archived from `docs/src/.md` when FerroForge moved to the call-through
model, in which nothing is transplanted. This chapter describes the source-
transplant design and the tooling built for it, all of which was removed from
the active tree. Binding decisions were promoted into the rewritten chapters
and the decision record before archival.

The code it describes is recoverable from branch `main` (full transplantation)
and `test/new_task_method` (transplanted init).

---

# Build and Development Workflow

Commands below describe the current prototype, including the bounded two-system
pipeline. The required
[composition and unified-target repair](composition-repair-plan.md) is still
planning work; its proposed interfaces are not executable commands yet.

## Toolchain

`rust-toolchain.toml` selects stable Rust, `clippy`, `rust-analyzer`, `rustfmt`,
and the host/embedded targets used by this repository. The documented Windows
host is `x86_64-pc-windows-msvc`; the current firmware target is
`thumbv7em-none-eabihf`.

The commands assume Cargo is on `PATH`. Where it is not, invoke it through the
`CARGO` environment variable, which the pipeline itself uses, or by the absolute
path to `cargo` in your rustup installation.

## Check the Host Tooling

From the repository root:

```text
cargo test --workspace --all-targets --locked
```

The host workspace does not include the embedded check library or generated
firmware. Checking the root alone does not validate ARM code.

This command also runs the shared-contract tests, Cargo-aware standalone
source-discovery and composition tests, and a real independent check of the SW
fixture. Its compile-fail harness verifies that invalid resource, spawn,
configuration, shared-borrow, and monotonic uses fail for the intended compiler
reason. Manifest tests also exercise conservative normal-dependency
collection, explicit check-only exclusion, feature merging, and conflict
diagnostics. See the
[standalone discovery foundation](prototype.md#standalone-discovery-foundation)
for what this slice currently covers.

To check only the standalone portable SW package from the repository root:

```text
cd ferroforge-renderer/tests/fixtures/sw
cargo check --lib --locked --offline
```

This is a host compile-only check with no system dependency. It does not render
or validate a real RTIC application.

The compiler-failure harness asserts that each focused diagnostic points to its
authored task-body line. A separate opt-in check records Rust Analyzer evidence:

```text
cargo test -p ferroforge-renderer --test standalone_check rust_analyzer_reports_errors_on_authored_task_body_lines --locked --offline -- --ignored --exact
```

This command requires the `rust-analyzer` executable. It analyzes the
intentionally invalid `tests/fixtures/ra-diagnostics` package and succeeds only
when the invalid method and concrete value diagnostics identify their authored
lines. Rust Analyzer 1.98.0 does not report the shared-lock lifetime case; the
always-run rustc regression is authoritative there. The fixture is outside the
workspace because it must remain invalid.

The focused real-RTIC namespace proof is a separate ARM fixture:

```text
cd ferroforge-renderer/tests/fixtures/rtic-layout
cargo check --bin ferroforge-rtic-layout-fixture --target thumbv7em-none-eabihf --locked --offline
```

It proves one logical source module can retain private helpers and one shared
support-type identity across complete RTIC handlers. The root test suite also
generates source through the early transplant renderer, rewrites resource
bindings, isolates colliding support names from sibling modules, and ARM-checks
the repeated-instance and multi-module results. Additional generated ARM cases
prove different configuration values on repeated instances and direct spawn
alias translation for zero, one, and multiple inputs. A further ARM case emits
the real 1 kHz/`u32` SysTick monotonic, starts it with `SYST`, and checks a
rewritten task delay. Another case checks qualified and imported/aliased defmt
and rtt-target calls with mapped configuration/resource expressions and
unchanged lookalike literals. The RTIC fixture therefore declares
`rtic-monotonics`, `defmt`, `rtt-target`, and the task source's direct `fugit`
dependency. A further generated ARM case consumes the independently checked init
package, retains its real STM32F4 clock/GPIO setup and resource construction,
rewrites `Mono::start`, and checks it beside selected real RTIC tasks. Each
generated program uses its own temporary Cargo target directory so a fingerprint
from another source case cannot satisfy its check. The suite also covers
contained `self::`, `crate::`, and descendant `super::` paths plus escape
diagnostics. Relative imports, other macro-token paths or mapped expressions,
cross-boundary references, and overlapping module selection remain unsupported.

## Check Standalone System Init

The focused init suite discovers the native system package, generates its
composition-specific checking interfaces, and checks the result for Cortex-M:

```text
cargo test -p ferroforge-renderer --test init_check --locked --offline
```

The positive case uses real STM32F4 clock and GPIO APIs with generated
`init::Context`, selected-instance spawn signatures, and
`Mono::start(SYST, u32)`. Negative cases regenerate after removing a selected
instance and verify that the stale init call fails, then check wrong spawn and
startup arguments plus a missing profile. Generated packages use unique
temporary directories and do not execute hardware initialization.

The authored-file IDE proof is opt-in because it invokes the external Rust
Analyzer CLI:

```text
cargo test -p ferroforge-renderer --test init_check rust_analyzer_reports_init_errors_on_authored_lines --locked --offline -- --ignored --exact
```

The regression temporarily places the generated interface and Cargo
configuration beside the invalid init fixture, runs Rust Analyzer 1.98.0, and
removes the generator-owned files afterward. It verifies that the wrong spawn
arity and startup clock type are reported on their authored init lines.

The initial checker accepts one qualified crate-root `#[ferroforge::init]` with
self-contained root support. It does not yet provide configuration access,
`cx.cs`, init-local storage, child support modules, or targets beyond STM32F401.
Manifest and orchestration integration do exist: `render_standalone_project`
merges init dependencies into the generated manifest, and both Nucleo pipelines
run the init check as a stage. The renderer transplants that initial checked
scope into a real-RTIC ARM program.

## Check the Bounded Standalone Project

The Phase 4 project regression combines the discovered task and init packages,
explicit system runtime selections, the validated composition, and the native
init transplant:

```text
cargo test -p ferroforge-renderer --test standalone_project --locked --offline
```

It emits `src/main.rs`, a manifest derived from conservative task/init/system
requirements, and the initial complete STM32F401RE target package in a unique
temporary project. The target package contains `.cargo/config.toml`, `memory.x`,
and `Embed.toml`, including the probe-rs runner, linker scripts, memory regions,
and defmt configuration. The positive case runs the configured ARM `cargo
check` and an optimized release build, proving that the generated firmware
links. Negative cases verify that malformed target memory data and a
system/source version conflict are rejected without leaving a partial project.
This regression is not the Phase 5 check/generate/build orchestration command.

## Render and Verify the Centralized Nucleo Systems

The two actual standalone systems keep reusable tasks separate from
system-owned init and composition:

```text
tasks/blinky/
systems/nucleo-f401re/
|-- init/
|-- app_composition/
`-- gen_app/
systems/nucleo-f401re-fast-blink/
|-- init/
|-- app_composition/
`-- gen_app/
```

From the repository root, run the host-only system pipeline:

```text
cargo run -p ferroforge-nucleo-f401re-composer --locked --offline
```

This one command checks `tasks/blinky` independently on ARM, validates the
composition, writes and checks the composition-specific init interface under
`systems/nucleo-f401re/.ferroforge/init-check`, renders the complete firmware
under `systems/nucleo-f401re/gen_app`, refreshes its lockfile offline, checks the
real RTIC target, and completes an optimized release build. Target task/init
code is compiled only by child Cargo processes, never linked into the host
composer.

Run the second reuse system from the same repository root:

```text
cargo run -p ferroforge-nucleo-f401re-fast-blink-composer --locked --offline
```

It follows the same stage order but reads its own init and composition. The
unchanged `tasks/blinky` definitions become `heartbeat` and `diagnostics`;
their resources become `activity_led`, `pulse_count`, and
`heartbeat_enabled`, and the period is 125 ms. This system intentionally uses
the same Nucleo-F401RE target, so the result proves system-level reuse but not
portability to another MCU family.

For focused diagnosis, stages can still be run from their
configuration-owning directories:

```text
cd tasks/blinky
cargo check --lib --target thumbv7em-none-eabihf --locked --offline

cd ../../systems/nucleo-f401re/.ferroforge/init-check
cargo check --lib --manifest-path ../../init/Cargo.toml --locked --offline

cd ../../gen_app
cargo check --locked --offline
cargo build --release --locked --offline
```

The pipeline centralizes native init, composition, target/runtime dependencies,
memory layout, and probe selection with the Nucleo system, while
`tasks/blinky` remains system-agnostic. A failed Cargo stage is reported by
name and prevents all dependent later stages. The default regressions simulate
failure at each command boundary and exercise real invalid task, composition,
init, firmware-generation, generated-check, and linker cases. They assert both
the reported stage and that later Cargo stages were not executed. The second
system's command and its unchanged-source regression cover reuse.

## Check Embedded Source

From the repository root, enter the source workspace and check it:

```text
cd embedded
cargo check --lib --target thumbv7em-none-eabihf --locked
```

This library has no firmware entry point and does not produce runnable
firmware. Mock contexts check actual HAL resources, task bodies, and init.

## Render the Application

From the repository root:

```text
cargo run -p ferroforge-example-composer --locked
```

The composer reads `embedded` source through `load_application`, applies
`composer/src/composition.rs`, and writes `generated/nucleo-f401re`. Rendering
overwrites generator-owned files there. Edit the source/composition and render
again when changing application behavior.

The current composer does not run the ARM checks or firmware build itself.

## Check and Build Firmware

From the repository root, enter the generated project:

```text
cd generated/nucleo-f401re
cargo check --all-targets --target thumbv7em-none-eabihf --locked
cargo build --release --target thumbv7em-none-eabihf --locked
```

Run these commands from the shown directory so Cargo discovers the local
linker flags as well as the target selection. `--manifest-path` on its own
does not change Cargo's configuration discovery directory. See
[Cargo configuration](https://doc.rust-lang.org/cargo/reference/config.html).

Use `--offline` too when dependencies are already cached. The checked-in
lockfiles make `--locked` suitable for reproducing the current example. A
newly generated project or intentional dependency change needs its lockfile
created or updated before using `--locked`.

The generated configuration includes a `probe-rs` runner, `Embed.toml`, and
defmt logging over RTT. A successful build does not imply the firmware has
been flashed or tested on hardware.

## Rust Analyzer

`.vscode/settings.json` links only three manifests: the host root, `embedded`,
and `generated/nucleo-f401re`. `tasks/blinky`, both `systems/*/init`, and both
`systems/*/gen_app` are not linked, so the editor does not index them without
opening them as their own projects. The generated `src/main.rs` is ordinary
source that the editor can index directly.

Committed `.cargo/config.toml` files exist only for `embedded`,
`generated/nucleo-f401re`, and the two `gen_app` directories. `tasks/blinky` and
the init packages have none: init's configuration is generated under
`.ferroforge/init-check`, which is also where `FERROFORGE_INIT_INTERFACES` is
set. Checking init from the editor therefore requires that generated directory
to exist and to be current.

The focused command above checks diagnostic span mapping through the standalone
task attribute. It is ignored in the default suite because the Rust Analyzer
CLI is an external, unstable test dependency; run it explicitly when changing
the standalone expansion or supported toolchain.

Verify the editor's active target when examining ARM-only source. A host check
that omits hardware code is not evidence that the hardware source type-checks.
The mock declarations provide compile-time feedback, not executable simulation.

## Build and Read the Book

The repository uses one mdBook at `docs/book.toml`. These commands were
validated with mdBook 0.5.2:

```text
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The build produces `docs/book/index.html`. While the serve command runs, open
`http://127.0.0.1:3000` in a browser. It watches the active book source; archived
documents are outside the book source and are not included in the build.

See [Maintaining This Book](documentation.md) for editing conventions and the
archive access rule.
