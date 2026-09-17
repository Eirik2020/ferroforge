# Remaining Work

The call-through model is proved end to end: a reusable task crate checks on its
own, two applications select the same definitions without changing them, and
both release-link for `thumbv7em-none-eabihf`. What remains is the tooling
around it.

[Current state](prototype.md) records what exists; this chapter records what
does not, in the order that unblocks the rest.

## A Group Generic Over Its Peripherals

`tasks/stm32f4-uart-dma` names `USART1` and `Stream2<DMA2>`, so it serves one
port on one chip and a second serial port has no way to select it. That is what
sent the OSD down a separate, plainer transport. Making a group generic over the
peripherals it drives would make one library serve every UART on a family; the
cost is that the concrete types are exactly what turn a mis-wiring into an
ordinary Rust error at the authored line. Whether that wants generics, a macro
or a group per port is open.

## OSD Hardware Validation

`tasks/msp-displayport` and its host tests say the frames are well formed. No
transmitter has drawn one. The number to watch is `answered` in the firmware's
status line: it counts requests replied to, so it separates a transmitter that
is not connected from one that is being talked to wrongly.

## A DMA UART That Transmits

The group's receive half has been driven by real SBUS traffic, which is what
settled whether the four tasks divide the work correctly rather than merely
wiring consistently. `on_tx` has had no such test: nothing starts a transmit, so
it counts completions that never happen. The open question is whether a
transmitting firmware needs anything from the group that the receive side did
not already force into `Port`.

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
