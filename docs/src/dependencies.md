# Dependency Management

Cargo resolves dependencies. FerroForge neither merges nor diagnoses them,
because under the call-through model nothing is copied between crates: a
firmware depends on its task crates normally, and their requirements resolve
transitively.

## Where Requirements Live

Task and init code declare dependencies once, in their own `Cargo.toml`. There
is no per-task dependency list and no version registry. A reusable task crate
declares what its body uses - `embedded-hal`, `fugit`, `defmt` - plus whichever
call-through interfaces its own declarations reach for:

- `rtic`, for the `Mutex` bound, when the task declares shared resources.
- `rtic-monotonics` and `fugit`, for the `Monotonic` bound, when the task
  declares a monotonic.

Each is required only by the declaration that needs it. Every task does carry a
monotonic slot whether or not its body uses a clock, so that the adapter stays
inferable, but the slot alone is an unbounded type parameter and costs nothing: a
task crate that declares no shared resources and no monotonic needs neither
crate. `examples/tasks/stm32f4-timer` is one, and depends on nothing but its HAL.

The cost that remains is narrower than it looks: a task declaring shared
resources is tied to RTIC, not only to `embedded-hal`.

## HAL-Specific Task Crates

A crate whose tasks touch a HAL cannot compile without a chip selected, because
a HAL cannot. It must not be the crate making that choice, so it forwards the
decision instead:

```toml
[features]
stm32f401 = ["stm32f4xx-hal/stm32f401"]
```

That feature exists for one purpose: letting the crate be checked on its own,
where nothing else has selected a chip. Adding one is adding a chip the crate
can be checked against, not narrowing where it can be used.

**A firmware must not enable it.** Cargo features are additive, so the task
crate's own HAL dependency takes its chip from the firmware's platform crates
automatically. Enabling the forwarding feature as well names the chip a second
time, outside the region the CLI owns - and then changing the firmware's chip
leaves that copy behind, giving the HAL two mutually exclusive chip features. It
reports that from a build script, as a panic with no cause, so `sync` refuses it
first and says which dependency and which feature.

Such a crate usually needs no bound at all. A resource may carry a concrete
inline type, so `local = [timer: CounterUs<TIM3>]` names what the firmware will
hold and the body is ordinary HAL code. Reach for a bound only when a definition
must serve several peripherals, and weigh it: making the timer generic means
bounding on what it can do, and the HAL puts its flag traits on `Timer`/`FTimer`
while a firmware holds a `Counter`, so such a bound has to carry a `Deref` step
and a trait to name it. One concrete definition per peripheral type is the
cheaper default.

## Check-Only Dependencies

`ferroforge` provides the attribute macros and contributes nothing at runtime,
so a task crate marks it check-only in the ordinary manifest:

```toml
[package.metadata.ferroforge]
check-only-dependencies = ["ferroforge"]
```

Classification is explicit, never inferred from crate names or from whether a
dependency provides procedural macros.

## Memory Beyond FLASH and RAM

`cortex-m-rt` requires a `FLASH` and a `RAM` region and places `.data`, `.bss`
and the stack itself. A part offering more - an STM32H7 has DTCM, AXI SRAM, four
SRAMs, backup SRAM and ITCM - lists the rest as extra regions, which reach the
linker script and nothing else:

```toml
[[memory.region]]
name = "AXISRAM"
origin = 0x24000000
size = 524288
```

Nothing is placed in them automatically. Which memory suits which data is the
application's decision, and a backend that chose would be choosing for every
firmware on that chip. `RAM` itself is the backend's one choice, because
`cortex-m-rt` has to be told something: for the H7 it is DTCM, the
lowest-latency option and what the HAL's own reference map uses.

## Firmware Manifest

The firmware declares what it compiles against: RTIC, the HAL its init uses, the
Cortex-M runtime, its logging and panic backends, and each task crate it
selects. Chip- and architecture-derived features such as `stm32f401` and
`thumbv7-backend` belong to the selected target; the logging and panic backends
are the firmware's choice.

Per G2a the chip-derived part is not authored. The CLI owns the region between
`# ferroforge:platform-dependencies` and `# ferroforge:end`; the task crates, the
logging and panic backends and the profile are the application's. See
[workflow](workflow.md) for the command.

## What This Gives Up

The superseded design merged task, init and system requirements, and diagnosed
conflicts by naming the contributing manifests. Cargo reports version conflicts
instead, in its own terms rather than as "task X requires...". That is simpler
and less tailored. If the diagnostics prove inadequate in practice, the place to
add them is the CLI, not a merge step.
