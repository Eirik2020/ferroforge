# Current State

What exists today, as distinct from the agreed design in
[architecture](architecture.md).

## Crates

| Crate | Role |
| --- | --- |
| `ferroforge-contracts` | Task declaration parsing and structural validation. No HAL dependency. |
| `ferroforge-macros` | `task` and `app!`, and nothing else. |
| `ferroforge` | Facade. Re-exports the two macros; no runtime types. |
| `ferroforge-cli` | The `ferroforge` binary. Carries the chip data, recognizes projects, and delegates to Cargo. |

The transplant-era crates - the renderer, the pipeline, the example composer,
the embedded and generated projects, and the two `systems/` firmwares - were
removed when the design changed. They are recoverable from branch `main` and
`test/new_task_method`.

## Working Example

`firmware/nucleo-f401re` is a Nucleo-F401RE firmware that checks and
release-links for `thumbv7em-none-eabihf`. It carries both task kinds: a
software blink with resource, configuration and spawn bindings, and a
synchronous handler bound to `TIM2`. Its reusable tasks live in
`tasks/blinky`, which checks independently for the
same target with no FerroForge machinery beyond the `#[task]` attribute.

`firmware/nucleo-f401re-beacon` is a second application on the same board,
selecting the same definitions with none of the same bindings. It instantiates
`blink` twice - different names, pins, counters, gates and periods - and the
HAL-specific `on_timer` once against `TIM3`. Adding it required no edit to
either task crate. It is a build, not a hardware result: only PA5
carries an LED, and the second pin is a bare header pin.

`firmware/foxeer-f405v2` is a third application, on a flight controller rather
than a Nucleo: a different chip, with its own flash size, RAM size and vector
table. It selects the same definitions with neither task crate changing. Two
board facts in it are marked unverified in its source - the LED pin and whether
to use the crystal - and nothing here has been flashed.

`firmware/nucleo-h753zi` is a Cortex-M7 on a different HAL, with a part whose
memory is more than the pair `cortex-m-rt` needs. It reuses `report` from the
portable crate unchanged and pairs it with `tasks/stm32h7-timer`, the F4 timer
task's counterpart. It does **not** use `blink`: that bounds on `embedded-hal`
1.0 and `stm32h7xx-hal` 0.16 implements only 0.2, so the LED is driven by a task
in the firmware instead.

`tasks/stm32f4-timer` is the HAL-specific case. Reading and clearing a timer's
update flag cannot be written against `embedded-hal`, so the task names the HAL
type its resource has - `CounterUs<TIM3>` - and its body is the handler anyone
would write by hand. It declares no bounds and depends on `stm32f4xx-hal` and
nothing else but the check-only `ferroforge`.

All four sit in the layout G5 requires, and opt-in tests in `ferroforge-macros`
assert it rather than leaving it to be run by hand: both task crates check
independently, both firmwares link, and five defects planted in copies of them
are each rejected for their own reason. Because those defects are applied to the
real firmwares, they cannot drift into testing nothing. See
[workflow](workflow.md) for how to run them.

## Implemented

- `#[task]` expands a definition into a real generic context and an ordinary generic
  function: real trait bounds, no mock layer.
- Task kind is read from the signature, and the hardware rules are enforced -
  no inputs, no divergent return.
- `app!` expands in place into `#[rtic::app]`, passing `Shared`, `Local`,
  `init` and ordinary items through untouched, and emitting a config impl and an
  adapter per instance.
- Configuration is an associated-const trait, so values stay compile-time and
  work in const positions such as array lengths.
- Selecting a crate that is not a FerroForge library fails to compile: the
  composition names that crate's own `Context`, `Local` and `Config`, so there
  is nothing for a separate library check to catch.
- Interrupt bindings are checked by RTIC against the device's own enum, and
  `app!` rejects an `async` task that binds one, which RTIC would otherwise
  report only as a signature complaint.
- A monotonic bound is emitted only when the task declares a monotonic, so a
  task that never reads time depends on neither `rtic-monotonics` nor `fugit`.
  The slot itself stays unconditional, as an unbounded type parameter.
- Chip-family data is one TOML file per chip under `backends/`, embedded in the
  CLI and depended on by nothing at compile time. A firmware names its chip and
  everything else follows: `memory.x`, `.cargo/config.toml`, `Embed.toml` and
  the platform crates in its manifest. No chip fact is maintained twice.
- A project is the nearest parent holding a `firmware/`, with no marker file.
  `ferroforge new` writes that layout, and the CLI's verbs are Cargo's.
- `app!`'s header is RTIC's: parsed as a loop so order does not matter, with
  `dispatchers` and `peripherals` optional and defaulting to RTIC's own
  behaviour. The firmware declares its monotonic itself, as in ordinary RTIC.
- Four chips are known, across two HAL families, and switching one line of a
  firmware's `Cargo.toml` rewrites every derived artifact. A part with more
  memory than the `FLASH`/`RAM` pair lists the rest as extra regions, which
  reach the linker script and nothing else.
- A chip feature enabled outside the generated block is refused, because the HAL
  would otherwise reject it from a build script as a panic with no cause.

## Not Implemented

- **Portability past embedded-hal 1.0.** A task bounding on it cannot be
  selected on a HAL that still implements 0.2, which `stm32h7xx-hal` does. That
  is the ecosystem's to fix, not FerroForge's, but it bounds what G1's "all
  hardware" means today.
- **A project-local backend.** Chip data ships with FerroForge, embedded in the
  binary. A project needing a chip FerroForge does not know cannot add one
  without upstreaming it.
- **A choice of tick rate.** `#[task]` fixes the monotonic bound at
  `fugit::Duration<u32, 1, 1000>`, so a firmware selecting any task that reads
  time must declare a 1 kHz monotonic. It now declares that itself, where the
  rate is at least visible, but a different rate still fails to unify rather
  than being rejected on the authored line.
