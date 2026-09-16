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

Both sit in the layout G5 requires. Two opt-in tests in `ferroforge-macros`
assert this rather than leaving it to be run by hand: one checks the task crate
independently, the other builds the firmware and asserts the binary links. Run
them with `cargo test -p ferroforge-macros --test firmware -- --ignored`.

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
- Interrupt bindings are checked by RTIC against the device's own enum.
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
- **A second firmware** demonstrating reuse across applications.
- **Broader coverage.** The two opt-in tests prove the example builds. Negative
  cases do not exist yet: a hardware task given inputs, a software task given an
  interrupt, or a configuration type disagreeing with its definition.

## Superseded Macros

`ferroforge-macros` still contains the transplant-era expansions. They are
unused by the call-through path and kept only because they share a file with
`reusable` and `compose!`. Separating them is outstanding work.
