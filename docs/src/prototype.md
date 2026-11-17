# Current State

What exists today, as distinct from the agreed design in
[architecture](architecture.md).

## Crates

| Crate | Role |
| --- | --- |
| `ferroforge-contracts` | Task and group declaration parsing and structural validation, for the macros and for host tools reading groups from source. No HAL dependency. |
| `ferroforge-macros` | `task`, `group` and `app!`, and nothing else. |
| `ferroforge` | Facade. Re-exports the three macros; no runtime types. |
| `ferroforge-cli` | The `ferroforge` binary. Carries the chip data, recognizes projects, and delegates to Cargo. |
| `msp` | MSP v1, DisplayPort and a text canvas. No dependencies, not FerroForge's - it is here because a task crate uses it, not because it is part of the tool. |

`msp` is a workspace member while the task crates are not, and the reason is
testing rather than layout. A task crate depends on RTIC and cannot run a host
test; a protocol crate can, so the parts worth testing were put where a test
can reach them. Keeping the two apart is what makes that possible.

The transplant-era crates - the renderer, the pipeline, the example composer,
the embedded and generated projects, and the two `systems/` firmwares - were
removed when the design changed. `fb92511`, in `main`'s history, is the last
commit that has them.

## Working Example

The worked example lives under `examples/`, which is itself a FerroForge project
in the expected layout: it holds the `firmware/` and `tasks/` a project has, so
CLI commands run from there. The repository root is deliberately not a project,
because FerroForge is the tool, not a firmware.

`examples/firmware/nucleo-f401re` is a Nucleo-F401RE firmware that checks and
release-links for `thumbv7em-none-eabihf`. It carries both task kinds: a
software blink with resource, configuration and spawn bindings, and a
synchronous handler bound to `TIM2`. Its reusable tasks live in
`examples/tasks/blinky`, which checks independently for the
same target with no FerroForge machinery beyond the `#[task]` attribute.

`examples/firmware/nucleo-f401re-beacon` is a second application on the same board,
selecting the same definitions with none of the same bindings, and the one that
exercises groups. It selects `blinky`'s `lights` group - two named copies of the
`light` group, each a light and the announcer it is wired to - and `light` a
third time on its own, each copy logging under its own label. Only PA5 carries
an LED; the other copies' pins are bare header pins, so they are observed
through their log lines.

It also selects `examples/tasks/pulse`'s `pulse_light`, a group of groups
across two crates: `blinky`'s `light` with a hardware task wired to its
announcer, bound to `TIM3` here. A software task pends that vector once a
second, so the hardware member runs on a bare board with no timer started.

`examples/firmware/nucleo-h753zi` is a Cortex-M7, another chip family under the
same HAL, with a part whose memory is more than the pair `cortex-m-rt` needs. It
selects `blink` and `report` from the portable crate unchanged: FerroHAL's pins
implement `embedded-hal` 1.0 on every chip, so one definition drives both
boards' LEDs. Only its `init` differs.

Every firmware here starts on FerroHAL, which is not on crates.io yet, so each
names its source through `[package.metadata.ferroforge.platform]` at the
revision `ferroforge-stm32` pins.

No task crate here names a HAL. HAL-specific tasks and groups - the hardware
ports and a timer tick - are in `ferroforge-stm32`, the helper
[G2b](governing-requirements.md) names, with their requirements and benches.

`examples/tasks/msp-displayport` is the OSD, and the portable case on something larger
than an LED: it names no HAL and no chip, takes no forwarding feature, and
checks for ARM on its own. No firmware selects it: on hardware it drew nothing.

It is one task, and the shape is the point. A DisplayPort
transmitter has to be answered before it hands over the canvas, so the traffic
runs both ways - but receiving is not a task here. The firmware's own handler
owns the port and already holds the shared state to drain the outgoing queue, so
a parser step costs it nothing, where a task spawned per received byte would
drop bytes whenever the previous one had not finished. What the library keeps is
the half that is genuinely periodic: the picture, pushed out on a timer.

Every one of these sits in the layout G5 requires, and opt-in tests in
`ferroforge-macros` assert it rather than leaving it to be run by hand: each
task crate checks independently, each firmware links, and defects planted in
copies of them are rejected for their own reasons. Because the defects are
applied to the real firmwares, they cannot drift into testing nothing - and one
has already caught a changed configuration value that would have left a case
asserting nothing. See [workflow](workflow.md) for how to run them.

## Implemented

- `#[task]` expands a definition into a real generic context and an ordinary generic
  function: real trait bounds, no mock layer.
- Task kind is read from the signature, and the hardware rules are enforced -
  no inputs, no divergent return.
- `app!` expands in place into `#[rtic::app]`, passing `Shared`, `Local`,
  `init` and ordinary items through untouched, and emitting a config impl and an
  adapter per instance.
- Every RTIC task shape ferro-wasp's flight apps use is expressible as a
  definition: spawn aliases of any arity, timestamps from `Mono::now()`,
  shared resources bounded by a trait, `#[lock_free]` shared resources, and
  task-local initial values - on the definition, or on the instance when they
  depend on the board. One compiled fixture holds all of them. The Foxeer
  flight app is built from these definitions for every task but its USB and
  storage tasks.
- Task groups select tasks that only work as a set in one declaration, with
  named copies, repeated selection and nesting across crates
  ([task groups](architecture.md#task-groups)). The beacon runs a group of
  groups on hardware.
- A task reading time takes the firmware's monotonic at any rate, counting in
  `u64`: SysTick, which every backend builds 64-bit, or a hardware timer
  chosen with `monotonic-timer`.
- Entries take documentation, which reaches the field or constant each becomes,
  and a task-local with an initial value takes `#[cfg]`. An instance keeps its
  own attributes, and `app!` passes through any item RTIC accepts.
- Configuration is an associated-const trait read as `CONFIG::FIELD`, so values
  stay compile-time and work inside macro calls. They do not work in const
  positions such as array lengths: the trait reaches the body as a generic
  parameter, and stable Rust refuses a generic's constant there.
- Selecting a crate that is not a FerroForge library fails to compile: the
  composition names that crate's own `Context`, `Local` and `Config`, so there
  is nothing for a separate library check to catch.
- Interrupt bindings are checked by RTIC against the device's own enum, and
  `app!` rejects an `async` task that binds one, which RTIC would otherwise
  report only as a signature complaint.
- A monotonic bound is emitted only when the task declares a monotonic, so a
  task that never reads time depends on neither `rtic-monotonics` nor `fugit`.
  The slot itself stays unconditional, as an unbounded type parameter.
- Chip-family data is one TOML file per chip under `ferroforge-cli/backends/`,
  inside the crate so a published CLI carries it, embedded in the binary and
  depended on by nothing at compile time. A firmware names its chip and
  everything else follows: `memory.x`, `.cargo/config.toml`, `Embed.toml` and
  the platform crates in its manifest. No chip fact is maintained twice.
- A project is the nearest parent holding a `firmware/`, with no marker file.
  `ferroforge new` writes that layout, `ferroforge add` adds a firmware to it,
  and the CLI's verbs are Cargo's.
- `ferroforge drift` compares code marked as copied between firmwares, one to
  one unless the project's `ferroforge.toml` forgives interrupt bindings or
  words matching a pattern.
- `app!`'s header is RTIC's: parsed as a loop so order does not matter, with
  `dispatchers` and `peripherals` optional and defaulting to RTIC's own
  behaviour. The firmware declares its monotonic inside `app!`, as in ordinary
  RTIC, and `app!` takes the type tasks are handed from that declaration.
- Five chips are known, across two families under one HAL, and switching one
  line of a firmware's `Cargo.toml` rewrites every derived artifact. A part with more
  memory than the `FLASH`/`RAM` pair lists the rest as extra regions, which
  reach the linker script and nothing else.
- A chip feature enabled outside the generated block is refused, because the HAL
  would otherwise reject it from a build script as a panic with no cause.

## Not Implemented

- **Transmit.** The DMA UART's `on_tx` counts completed transfers and nothing else.
  Only the receive half has been driven by real traffic.
- **A project-local backend.** Chip data ships with FerroForge, embedded in the
  binary. A project needing a chip FerroForge does not know cannot add one
  without upstreaming it.
- **An image that rebuilds identically elsewhere.** Cargo hashes a path
  dependency into `-C metadata` relative to the workspace root when it lies
  inside the workspace, and by absolute path when it does not. A firmware is its
  own workspace ([G5](governing-requirements.md)) and a project's task crates
  lie outside it, so one commit built in two directories gives two images;
  `trim-paths` removes the source paths in panic locations but not this. The
  preserved binary is therefore an image's record, never a rebuild. Stable
  hashes would need the task crates inside the firmware's workspace, which G5
  rules out, or every build made at one fixed path.
