# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## Known Defects

Each of these has been observed. None is a design question.

- **A CLI test failed once with no known cause.** Before the race below was
  found, one test in `ferroforge-cli/tests/project.rs` failed once, and its name
  was not captured. Later, `build_all_reports_every_firmware` failed once with
  `no line for alpha`. Two suites in one checkout fail on every attempt, which
  [Workflow](workflow.md) rules out, but neither failure is known to have run
  beside another suite. Neither has been reproduced alone: CI's logs name a
  failing test, so the next one is caught running.

## OSD Hardware Validation

`examples/tasks/msp-displayport` and its host tests say the frames are well formed. On
hardware, a transmitter drew nothing, and the cause has not been looked for, so
no firmware selects it now. A firmware that does should report how many of the
transmitter's requests were answered: that separates a transmitter that is not
connected from one that is being talked to wrongly.

## A DMA UART That Transmits

The receive half of `examples/tasks/stm32f4-uart-dma` has been driven by real SBUS
traffic, which is what settled whether its four tasks divide the work correctly.
`on_tx` has had no such test: nothing starts a transmit, so it counts
completions that never happen, and it is the one member of the beacon's groups
that only compiles. The open question is whether a transmitting
firmware needs anything from these tasks that the receive side did not already
force into `Port`.

## Group Contracts

A [group contract](architecture.md#group-contracts) lists slots, vectors,
resources and the constructor. The FerroSmith design record (v3, 6.1) also puts
three things in it that the first cut leaves out: buffers, whose sizes are Rust
expressions a tool emits verbatim; configuration fields and the layer that
supplies each; and optional members behind a feature. `app!` reads none of the
contract yet, and checks a selection as it always has.

## FerroForge 0.5

[G2b](governing-requirements.md)'s helper, `ferroforge-stm32`, exists, on
FerroHAL. The backends still name `stm32f4xx-hal` and `stm32h7xx-hal`; 0.5
moves them to FerroHAL (chips f401re, f405rg, f407vg, h743vi and h753zi; f411re
drops) and the STM32 examples to `ferroforge-stm32`. FerroHAL and
`ferroforge-stm32` are published to crates.io before that release.

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
