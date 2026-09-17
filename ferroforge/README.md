# FerroForge

The facade crate. It re-exports the two macros - `#[ferroforge::task]` for a
task definition, `ferroforge::app!` for the firmware that selects it - and
contributes nothing at runtime, which is why a task crate marks it check-only.

Start at the [table of contents](../docs/src/SUMMARY.md); the authoring model is
in [architecture](../docs/src/architecture.md), and what exists today is in
[current state](../docs/src/prototype.md).
