# Current State

What exists today, as distinct from the agreed design in
[architecture](architecture.md).

## Crates

| Crate | Role |
| --- | --- |
| `ferroforge-contracts` | Task declaration parsing and structural validation. No HAL dependency. |
| `ferroforge-macros` | `reusable` and `compose!`. Also still holds the superseded `app!`, `composition!`, `firmware!`, `dependency_registry!` and mock `task` expansions. |
| `ferroforge` | Facade re-exporting the macros. |
| `ferroforge-cli` | The `ferroforge` binary. Reads chip-family data and emits a firmware's target files. |

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
same target with no FerroForge machinery beyond the `reusable` attribute.

`firmware/nucleo-f401re-beacon` is a second application on the same board,
selecting the same definitions with none of the same bindings. It instantiates
`blink` twice - different names, pins, counters, gates and periods - and binds
`on_tick` to `TIM3`, which is what a definition that never names an interrupt is
for. Adding it required no edit to `tasks/blinky`. It is a build, not a
hardware result: only PA5 carries an LED, and the second pin is a bare header
pin.

All three sit in the layout G5 requires, and opt-in tests in `ferroforge-macros`
assert it rather than leaving it to be run by hand: the task crate checks
independently, both firmwares link, and four defects planted in a copy of one
are each rejected for their own reason. Because those defects are applied to the
real firmware, they cannot drift into testing nothing. See
[workflow](workflow.md) for how to run them.

## Implemented

- `reusable` expands a task into a real generic context and an ordinary generic
  function: real trait bounds, no mock layer.
- Task kind is read from the signature, and the hardware rules are enforced -
  no inputs, no divergent return.
- `compose!` expands in place into `#[rtic::app]`, passing `Shared`, `Local`,
  `init` and ordinary items through untouched, and emitting a config impl and an
  adapter per instance.
- Configuration is an associated-const trait, so values stay compile-time and
  work in const positions such as array lengths.
- Interrupt bindings are checked by RTIC against the device's own enum, and
  `compose!` rejects an `async` task that binds one, which RTIC would otherwise
  report only as a signature complaint.
- Chip-family data is one TOML file per chip under `backends/`, read by the CLI
  and depended on by nothing. It yields `memory.x`, `.cargo/config.toml`,
  `Embed.toml` and the platform crates in a firmware's manifest, so no chip fact
  is maintained in two places.

## Not Implemented

- **Chip selection in the composition.** `device` and `dispatchers` are written
  into `compose!` by hand rather than derived from the selected chip, so the
  chip is named in the backend file and again in the firmware source.
- **Project conventions and the library marker.** G7 is untouched: no project
  recognition, no new-project helper, and no check for
  `library = true` under `[package.metadata.ferroforge]`.
- **A choice of tick rate.** `reusable` fixes the monotonic bound at
  `fugit::Duration<u32, 1, 1000>`, so every firmware selecting a task that
  delays must declare `monotonic_hz = 1000`. The declaration reads like a choice
  and is not one; a different rate fails to unify rather than being rejected on
  the authored line.

## Superseded Macros

`ferroforge-macros` still contains the transplant-era expansions. They are
unused by the call-through path and kept only because they share a file with
`reusable` and `compose!`. Separating them is outstanding work.
