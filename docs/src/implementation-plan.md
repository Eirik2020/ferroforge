# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, and a firmware carrying both task kinds checks and release-links for
`thumbv7em-none-eabihf`. What remains is the tooling around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## Acceptance Bar

Independently checked task source producing buildable real RTIC firmware,
followed by demonstrated reuse in a second firmware. Exhaustive coverage is not
required. Recording that a phase renders is not evidence; record exact paths,
targets, commands and results.

A compile-fail case must fail for the intended reason, not because a dependency
or file is missing, and the harness must distinguish an expected negative from a
broken run. IDE feedback does not replace compiler checks, and a build does not
claim flashing or hardware validation.

## Negative Coverage

Two opt-in tests assert that `tasks/blinky` checks independently and that
`firmware/nucleo-f401re` release-links. Both were verified to fail when a
binding is wrong, so they are load-bearing rather than decorative.

What they do not cover is rejection. Needed: a hardware task given inputs, a
software task given an interrupt, a configuration type disagreeing with its
definition, and a binding naming a resource that does not exist. Each must fail
for its own reason, not merely fail.

## Second Firmware

A second application selecting the same task crates with different instance
names, resources and configuration, on the same board. This proves reuse, not
portability to another MCU.

## Project Conventions and the Library Marker

G7 is untouched. A FerroForge library marks itself with `library = true` under
`[package.metadata.ferroforge]`, and selecting an unmarked crate fails with an
error naming the crate and the missing marker. Marker absence is the only
library check.

Still open: minimum project recognition inputs, new-project helper output,
library references, backend discovery, and command names.

## Separate the Superseded Macros

`ferroforge-macros` still contains the transplant-era `app!`, `composition!`,
`firmware!`, `dependency_registry!` and mock `task` expansions. They are unused
and share a file with `reusable` and `compose!`.

## Design Principle

Keep Rust as the source of truth. Generate only what needs whole-application
knowledge, and let the compiler check everything that can be named as a real
type.
