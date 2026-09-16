# Dependency Management

Cargo resolves dependencies. FerroForge neither merges nor diagnoses them,
because under the call-through model nothing is copied between crates: a
firmware depends on its task crates normally, and their requirements resolve
transitively.

## Where Requirements Live

Task and init code declare dependencies once, in their own `Cargo.toml`. There
is no per-task dependency list and no version registry. A reusable task crate
declares what its body uses - `embedded-hal`, `fugit`, `defmt` - plus the
interfaces the call-through contract requires:

- `rtic`, for the `Mutex` bound on shared resources.
- `rtic-monotonics`, for the `Monotonic` bound. Every task carries a monotonic
  slot whether or not its body uses a clock, so the adapter stays inferable.

That is a real cost of this design: a portable software task is tied to RTIC,
not only to `embedded-hal`.

## Check-Only Dependencies

`ferroforge` provides the attribute macros and contributes nothing at runtime,
so a task crate marks it check-only in the ordinary manifest:

```toml
[package.metadata.ferroforge]
check-only-dependencies = ["ferroforge"]
```

Classification is explicit, never inferred from crate names or from whether a
dependency provides procedural macros.

## Firmware Manifest

The firmware declares what it compiles against: RTIC, the HAL its init uses, the
Cortex-M runtime, its logging and panic backends, and each task crate it
selects. Chip- and architecture-derived features such as `stm32f401` and
`thumbv7-backend` belong to the selected target; the logging and panic backends
are the firmware's choice.

Per G2a this manifest should be scaffolded by the CLI from chip-family data.
Today it is authored by hand.

## What This Gives Up

The superseded design merged task, init and system requirements, and diagnosed
conflicts by naming the contributing manifests. Cargo reports version conflicts
instead, in its own terms rather than as "task X requires...". That is simpler
and less tailored. If the diagnostics prove inadequate in practice, the place to
add them is the CLI, not a merge step.
