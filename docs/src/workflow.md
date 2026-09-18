# Build and Development Workflow

There is no generation step. The work is: refresh what a firmware's chip
implies, check a reusable task crate on its own, and build the firmware.

## Toolchain

`rust-toolchain.toml` selects stable Rust with `clippy`, `rust-analyzer` and
`rustfmt`, plus the host and embedded targets. The current firmware target is
`thumbv7em-none-eabihf`. Where Cargo is not on `PATH`, invoke it through the
`CARGO` environment variable or by absolute path.

## Check the Host Crates

From the repository root:

```text
cargo test --workspace --locked
```

This covers task declaration parsing, macro expansion, and the backend data the
CLI emits from. It does not build any firmware.

The tests that do are opt-in, because each drives a cross-compile:

```text
cargo test --workspace --locked -- --ignored
```

That checks every task crate on its own, links every firmware, and plants eight
defects in copies of them - a hardware task given inputs, an `async` task given
an interrupt, a configuration type disagreeing with its definition, a binding
naming a resource that does not exist, a resource whose type is not what a
HAL-specific task declares, a task needing a monotonic the application lacks, a
dispatcher that is also bound, and too few dispatchers - asserting the message
each one must fail with. Each unmutated copy is checked first, so a broken
harness cannot pass as a rejection. It also runs `ferroforge new` and then
`ferroforge build` for one chip per HAL family, and fails on any warning,
because that is the first thing a new user sees.

Run them when changing an expansion. A case that starts failing with the wrong
message is the point: it means a defect stopped being diagnosable at the
authored line.

## The CLI

The verbs are Cargo's, because a firmware crate is an ordinary Cargo package:

```text
ferroforge new <path> [--chip <name>]   a project that runs, see below
ferroforge sync  [<firmware>]           refresh what the chip implies
ferroforge check [<firmware>]           sync, then cargo check
ferroforge build [<firmware>]           sync, then cargo build --release
ferroforge run   [<firmware>]           sync, then cargo run --release
ferroforge chips                        the chips this build knows
```

Anything after `--` is passed to Cargo untouched.

`new` writes one firmware and one task crate, and the firmware runs as soon as
it is flashed: it selects a `heartbeat` task that logs over RTT, starts from the
internal oscillator and uses no pins, so it runs on any board carrying the chip.
Its clock setup is the only HAL-specific code in it. `new` holds one per HAL,
taken from a firmware that has run on hardware, and refuses a chip whose HAL it
has none for rather than writing one that cannot start. The project depends on
the published `ferroforge`; `--ferroforge <path>` points it at a checkout
instead, for working on FerroForge itself.

A project is the nearest parent directory holding a `firmware/`. That is the
whole convention - there is no marker file and nothing to initialize, so a
project written by hand is indistinguishable from one `new` produced. Outside
one, commands say so rather than guessing.

Which application a command applies to is resolved the way Cargo resolves a
package: a name if you give one, otherwise the firmware you are standing in,
otherwise the only one there is. Anything else is ambiguous and lists the
candidates.

Each firmware declares its chip, and everything the chip implies follows from
that alone:

```toml
[package.metadata.ferroforge]
chip = "stm32f401re"
defmt-log = "info"
```

`defmt-log` is optional and defaults to `info`. It takes anything `DEFMT_LOG`
takes, including a per-crate filter such as `info,noisy_crate=off`. It is
declared rather than passed on the command line because it is written into an
emitted file: a flag would leave that file disagreeing with the manifest, which
is the drift the derived files exist to prevent.

`sync` rewrites `memory.x`, `.cargo/config.toml` and `Embed.toml`, and replaces
the region of that firmware's `Cargo.toml` between
`# ferroforge:platform-dependencies` and `# ferroforge:end`. Nothing else in the
manifest is touched, and a manifest without those markers is an error rather
than a guess. `check`, `build` and `run` sync first, so a build cannot consume a
stale file.

Re-syncing is how drift is caught: if it changes anything, a chip fact was
edited in the wrong place. `cargo test -p ferroforge-cli` asserts exactly that,
against every application under `firmware/`.

## Check a Reusable Task Crate

A task crate compiles on its own, against its declared bounds, with no firmware
and no generated interfaces:

```text
cd tasks/blinky
cargo check --lib --target thumbv7em-none-eabihf --locked
```

A failure here is a fault in the task, not in a composition that selects it.

A HAL-specific crate needs a chip to compile at all, so name one of its
forwarding features:

```text
cd tasks/stm32f4-timer
cargo check --lib --target thumbv7em-none-eabihf --features stm32f401
```

That choice is the check's, not the crate's: a firmware selecting this crate
unifies its own chip feature with it.

## Build Firmware

The firmware crate is the binary, so one invocation checks and links it:

```text
ferroforge build nucleo-f401re
```

Cargo works directly too, but must be run from inside the firmware directory so
it picks up that firmware's `.cargo/config.toml`, which selects the target and
linker arguments; `--manifest-path` alone does not change configuration
discovery. See the
[Cargo configuration reference](https://doc.rust-lang.org/cargo/reference/config.html).
`ferroforge build` runs Cargo there for exactly this reason.

```text
cd firmware/nucleo-f401re
cargo build --release --bin nucleo-f401re
```

Each application under `firmware/` is its own workspace. They do not share a
target directory, so building one does not rebuild another.

A successful build does not imply the firmware has been flashed or tested on
hardware.

## Inspect What Was Generated

There is no generated project to read. To see what `app!` produced:

```text
cargo expand --bin nucleo-f401re
```

This is post-expansion output and includes RTIC's own expansion, so it is
harder to read than a generated source file would be. That is a deliberate
trade for deleting the generation step.

## Rust Analyzer

The firmware crate is an ordinary Cargo package, so Rust Analyzer sees the real
application with no generated interfaces to refresh. Check the editor's active
target when examining ARM-only source: a host check that omits hardware code is
not evidence that the hardware source type-checks.

## Build and Read the Book

```text
mdbook build docs
```

Missing summary chapters fail the build. mdBook does not validate anchors, so
check links against the built HTML separately.
