# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## Checks That Run Themselves

There is no CI configuration in this repository; nothing runs the checks but a
person. Every defect found since the call-through model was adopted was found by
converting a real consumer rather than by the test suite, and one of them - the
CLI writing a firmware's derived files before reading its manifest - reached
flashed hardware and changed an image that had already flown. A suite that only
runs when someone remembers is not what protects the CLI.

This comes first because the intermittent failure below cannot be found any
other way.

## Known Defects

Each of these has been observed. None is a design question.

- **A resource or configuration entry accepts no attribute but `#[lock_free]`.**
  Every other attribute is rejected where the entry is parsed, and a doc comment
  is an attribute, so a configuration entry cannot be documented where it is
  declared and a resource cannot be `#[cfg]`-gated. Both are ordinary Rust on an
  ordinary field, which is what [G6](governing-requirements.md) asks for.
- **One CLI test fails intermittently.** Seen once in
  `ferroforge-cli/tests/project.rs`, not reproduced, and the name was not
  captured. Fixture directories are uniquely named, the only cargo-invoking test
  is `#[ignore]`d, and the derived files the idempotency test reads are
  committed, so the three obvious causes are ruled out and it has to be caught
  running. It guards the file-writing order above, so it is worth pinning.
- **A build is tied to the directory it was built in.** Cargo hashes the
  absolute package path into `-C metadata`, so one commit built from two paths
  produces two different images. `trim-paths` removes the path strings and not
  the hashing, which was measured rather than assumed. An exact-image claim is
  therefore only valid for one checkout location. Whether the CLI should offer
  anything here is undecided.

The tick rate a task may read is fixed at 1 kHz, which [current
state](prototype.md) records among what is not implemented. It belongs in view
here too: a consumer now runs its control loop at 2 kHz against 1 ms timestamps,
so the cost is being paid rather than anticipated.

## OSD Hardware Validation

`examples/tasks/msp-displayport` and its host tests say the frames are well formed. On
hardware, a transmitter drew nothing, and the cause has not been looked for. The number to watch is `answered` in the firmware's
status line: it counts requests replied to, so it separates a transmitter that
is not connected from one that is being talked to wrongly.

## A DMA UART That Transmits

The receive half of `examples/tasks/stm32f4-uart-dma` has been driven by real SBUS
traffic, which is what settled whether its four tasks divide the work correctly.
`on_tx` has had no such test: nothing starts a transmit, so it counts
completions that never happen. The open question is whether a transmitting
firmware needs anything from these tasks that the receive side did not already
force into `Port`.

## Drift Settings

`ferroforge drift` compares marked copies one to one. Differences that are
expected between firmwares - interrupt bindings, pins, peripheral names - need
settings, and they will live in one project-wide file; a project without one
keeps the one-to-one comparison. Undecided: the file's name, and what a setting
can express. "Ignore interrupt bindings" and "only look for a keyword" are the
first two wanted. This would be the first project-level file, where so far a
project has been only a directory holding `firmware/`.

## The STM32F4 Helper

[G2b](governing-requirements.md) names `ferroforge-stm32f4` as the
FerroForge-oriented STM32F4 helper sharing helpers and types between init and
hardware tasks. No such crate exists. The consumer carries its own equivalent,
so what is undecided is whether the helper is extracted from it or the
requirement is met by each project owning one.

## ferro-wasp Adoption

What ferro-wasp needs before it can adopt FerroForge, and the migration order,
is planned in [adopting FerroForge in ferro-wasp](ferro-wasp-adoption.md). Both
the macro and the CLI gaps are closed, so what is left there is migration rather
than design.

## Evidence Rules

Recording that something builds is not evidence; record exact paths, targets,
commands and results.

A compile-fail case must fail for the intended reason, not because a dependency
or file is missing, and the harness must distinguish an expected negative from a
broken run. IDE feedback does not replace compiler checks, and a build does not
claim flashing or hardware validation.

## Design Principle

Keep Rust as the source of truth. Generate only what needs whole-application
knowledge, and let the compiler check everything that can be named as a real
type.
