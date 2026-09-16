# Build and Development Workflow

There is no generation step, so the work is: emit a firmware's target files from
its chip, check a reusable task crate on its own, and build the firmware.

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

This covers task declaration parsing and macro expansion. It does not build any
firmware.

## Emit a Firmware's Target Files

Everything a firmware needs that follows from its chip comes from one backend
file:

```text
cargo run -p ferroforge-cli -- target backends/stm32f4/stm32f401re.toml firmware/nucleo-f401re
```

This overwrites `memory.x`, `.cargo/config.toml` and `Embed.toml`, and replaces
the region of that firmware's `Cargo.toml` between
`# ferroforge:platform-dependencies` and `# ferroforge:end`. Nothing else in the
manifest is touched, and a manifest without those markers is an error rather
than a guess. `--defmt-log <level>` sets `DEFMT_LOG`; it defaults to `info`.

`ferroforge platform-deps <backend.toml>` prints the same dependency lines
without writing anything.

Re-emitting is how drift is caught: if the output differs from what the firmware
holds, a chip fact was edited in the wrong place. `cargo test -p ferroforge-cli`
asserts exactly that against `firmware/nucleo-f401re`.

## Check a Reusable Task Crate

A task crate compiles on its own, against its declared bounds, with no firmware
and no generated interfaces:

```text
cd tasks/blinky
cargo check --lib --target thumbv7em-none-eabihf --locked
```

A failure here is a fault in the task, not in a composition that selects it.

## Build Firmware

The firmware crate is the binary, so one invocation checks and links it:

```text
cd firmware/nucleo-f401re
cargo check --bin nucleo-f401re
cargo build --release --bin nucleo-f401re
```

Run these from the firmware directory so Cargo picks up its
`.cargo/config.toml`, which selects the target and linker arguments;
`--manifest-path` alone does not change configuration discovery. See the
[Cargo configuration reference](https://doc.rust-lang.org/cargo/reference/config.html).

A successful build does not imply the firmware has been flashed or tested on
hardware.

## Inspect What Was Generated

There is no generated project to read. To see what `compose!` produced:

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
