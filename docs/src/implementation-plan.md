# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

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

## ferro-wasp Adoption

What ferro-wasp needs before it can adopt FerroForge - two macro gaps and two
CLI gaps awaiting decisions, and the migration order - is planned in
[adopting FerroForge in ferro-wasp](ferro-wasp-adoption.md).

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
