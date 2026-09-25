# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## Open Defects

Every known defect and unexercised claim, in the order to fix them. An entry
that has its own section here or elsewhere links there instead of restating it.

1. **No CI.** The repository has no workflow, so nothing runs the test suite on
   a push. It comes first because every defect found while converting ferro-wasp
   was one the suite did not catch.
2. **An intermittent CLI test failure.** The failing test has not been
   identified, and repeated local runs have not reproduced it; CI running the
   suite repeatedly is meant to catch it. The fake cargo that CLI tests rewrite
   and immediately execute, while other tests spawn processes, is one
   unconfirmed suspect. `syncing_the_real_project_changes_nothing` is the one
   test that writes into the checked-in tree.
3. **A resource or config entry takes no attribute but `#[lock_free]`.** A doc
   comment on a config entry is therefore a compile error, and so is `#[cfg]`
   on a resource, which is the half that blocks real code.
4. **The monotonic profile is fixed at 1 kHz** ([task
   authoring](architecture.md#task-authoring)). A firmware declaring another
   rate fails inside the expansion rather than on its authored line, and a
   control loop faster than the tick cannot timestamp its own iterations.
5. **Overriding a backend's platform crate source** and **where a firmware
   records probe arguments and environment variables** are undecided; both
   block the CLI managing ferro-wasp ([CLI gaps](ferro-wasp-adoption.md#cli-gaps)).
6. **OSD on hardware** drew nothing ([OSD hardware
   validation](#osd-hardware-validation)).
7. **The DMA UART's transmit half** is untested ([a DMA UART that
   transmits](#a-dma-uart-that-transmits)).
8. **Images are reproducible only from one checkout path.** Cargo hashes the
   absolute package path into symbol metadata, and `trim-paths` does not remove
   that effect, so exact-image evidence holds for one checkout location only.
9. **A project cannot add a chip.** Backends are compiled into the CLI, so a
   chip FerroForge does not ship cannot be used without upstreaming it.
10. **Drift has no settings** ([drift settings](#drift-settings)).
11. **`ferroforge-stm32f4` does not exist**, though
    [G2b](governing-requirements.md) names it as the STM32F4 helper.
12. **G1 has no live witness.** Nothing in ferro-wasp runs one definition across
    two boards any more ([FCU3](ferro-wasp-adoption.md#fcu3)); FerroForge's own
    example tasks and tests carry the claim.
13. **Bounds on `embedded-hal` 1.0 exclude HALs still at 0.2**, which limits
    what G1's "all hardware" covers ([task
    authoring](architecture.md#task-authoring)).

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
