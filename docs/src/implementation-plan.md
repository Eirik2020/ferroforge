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

## Group Contracts

A [group contract](architecture.md#group-contracts) lists slots, vectors,
resources, buffers, configuration fields and the constructor's arguments. The
FerroSmith design record (v3, 6.1) also puts optional members behind a feature
in it, which waits for a group that has one. `app!` reads none of the contract
but a member serving every vector of its slots, which a selection may declare
once per vector; otherwise it checks a selection as it always has.

## FerroForge 0.5

What is left for 0.5 is the release itself: FerroHAL and
`ferroforge-stm32` are published to crates.io first, then the scaffold's
FerroHAL source (`UNPUBLISHED` in `ferroforge-cli`) is emptied, so a new
firmware takes FerroHAL by version like every other platform crate, and the
example firmwares drop their `platform` tables.

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
