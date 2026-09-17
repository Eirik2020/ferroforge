# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## Hardware Validation

Every result here is a build. Nothing has been flashed, and two facts in
`firmware/foxeer-f405v2` are marked unverified in its source: which pin drives
the status LED, and whether to run from the board crystal rather than the
internal oscillator.

Four boards build; none has been run. The Nucleo-F401RE, Nucleo-H753ZI and
Foxeer F405 V2 are all to hand, so this is the next thing that can actually be
closed.

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
