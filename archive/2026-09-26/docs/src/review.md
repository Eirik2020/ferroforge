# Review and Open Decisions - archived entries

Archived on 2026-09-26 from `docs/src/review.md`. These entries recorded
progress, release notes and early decisions whose content the governing
requirements and the active chapters now state. Binding decisions were promoted
out of them before archiving: task groups into `architecture.md` (Firmware
Composition), and the versioning rule and publishing order into `workflow.md`
(Release), each indexed in `review.md`'s binding decisions. Everything else
binding was already specified elsewhere. This file is not active design
authority. The entries follow verbatim, in their original order.

**Interrupt binding, 2026-09-16.** Hardware tasks require an interrupt binding;
software tasks do not. Task kind follows the authored signature, as in RTIC.

**Interrupt selection closed, 2026-09-16.** Nothing to build. The firmware's
`device =` selects the PAC, RTIC resolves `binds` against that device's own
`Interrupt` enum, and an interrupt the chip does not have is a compile error on
the authored line - verified against `NOT_A_REAL_INTERRUPT`. FerroForge neither
owns nor validates an interrupt list. This supersedes the earlier position that
a backend owns a per-chip enum; G4 was reworded to match.

**Backend is data, not a crate, 2026-09-16.** With nothing transplanted there is
no compile-time consumer of chip data, so the resolved host model and the
backend crate both fall away. A backend is build-time data read by the CLI, per
G2a: memory layout, probe identity, Rust target triple, device path and the
platform crate selections. TOML is sufficient and must not name interrupts.
This closes the earlier Cargo-centralization question: source manifests stay
authoritative for their own requirements, and Cargo resolves them.

**Component purposes and CLI, 2026-09-16.** Software task libraries are intended
for all hardware and projects; hardware task libraries are HAL-specific and
reusable across projects using that HAL. A project's `firmware/` groups its
applications. FerroForge operates as a CLI, like Cargo, with expected project
conventions. The explicit library marker agreed here was later dropped; see the
superseded table below.

**Project conventions, 2026-09-16.** Project structure is a convention
FerroForge expects, supported by a new-project helper analogous to
`cargo generate`. It is not a layout policing mechanism: changes that preserve
recognition are allowed, and an operation errors only when required inputs can
no longer be recognized.

**Initial scope confirmed, 2026-09-16.** The first proof is a working F401
example carrying both task kinds. Other boards follow afterwards. This is met:
`examples/firmware/nucleo-f401re` checks and release-links with a software blink
and a `TIM2` handler.

**Library marker dropped, 2026-09-17.** G7 previously required a crate to mark
itself a FerroForge library. Under the call-through model a composition names
the selected crate's own `Context`, `Local` and `Config`, so an unmarked crate
fails to compile and a marked crate without `#[task]` definitions fails anyway: the
marker was neither necessary nor sufficient, and requiring the firmware to list
its libraries put one fact in two places. G7 was reworded to say FerroForge adds
no separate library check - the same correction already made to G4.

**Macros renamed, 2026-09-17.** `compose!` became `app!` and `#[reusable]`
became `#[task]`, so FerroForge uses RTIC's words for RTIC's ideas. The old
names described the implementation - what the macro did - where the new ones
describe what the author is declaring. This also closed a long-standing
inconsistency: G5 and three chapters named a `composition!` macro that never
existed under that name.

**Second chip added, 2026-09-17.** STM32F411RE, so the backend registry, the
`chips` listing and chip-switching are exercised by more than one entry rather
than by synthetic fixtures. Changing one line of a firmware's `Cargo.toml`
changes all four derived artifacts, and the same firmware source builds for
either chip.

**`monotonic_hz` replaced by `monotonic`, 2026-09-17.** It had no RTIC
counterpart, invented a frequency, and made every application claim SysTick
whether or not anything read time. A firmware now declares its own monotonic
above `app!` and starts it in `init`, exactly as an RTIC user does, and names it
so adapters can hand the type to tasks. That last part is not RTIC's, and cannot
be: a task crate is generic over the clock, so something must supply the type.
An application declaring none gets an uninhabited `NoMonotonicDeclared` in the
slot, so a task that needs a clock fails against a name that says why. The 1 kHz
bound is unchanged, but now visible where it is chosen.

**Vector tables and PAC siblings, 2026-09-17.** Two corrections came out of
carrying a firmware to an STM32F405RG, a chip with its own flash size, RAM size
and vector table. The board itself was dropped before it was ever flashed; the
corrections outlived it. The vector table is sized by the PAC's
`__INTERRUPTS` array, not by the highest `Interrupt` variant, and a released PAC
can differ from its `-staging` sibling: reading the wrong one put the F405's
`text-offset` 32 bytes too high. Every offset is now taken from `__INTERRUPTS` and
confirmed against linked binaries. Separately, the chip-feature check scanned the
generated block too, so changing a firmware's chip tripped it on the block that
was about to be rewritten - which would have made the one workflow derived files
exist for impossible.

**A second HAL, 2026-09-17.** `examples/firmware/nucleo-h753zi` is a Cortex-M7 on
`stm32h7xx-hal`, with `examples/tasks/stm32h7-timer` as the F4 timer task's counterpart.
Three things came out of it.

`device` is confirmed a HAL fact and not a constant: it is `stm32h7xx_hal::pac`
here, where every F4 chip shares `stm32f4xx_hal::pac`.

The memory model needed extending, as expected. An H7 has DTCM, AXI SRAM, four
SRAMs, backup SRAM and ITCM; `[[memory.region]]` now carries whatever a part has
beyond the `FLASH`/`RAM` pair `cortex-m-rt` requires. Nothing is placed in them
automatically - which memory suits which data is the application's decision, and
a backend choosing would choose for every firmware on that chip.

The unexpected finding is a limit on G1 rather than on this design.
`examples/tasks/blinky` bounds on `embedded-hal` 1.0 and `stm32h7xx-hal` 0.16 implements
only 0.2, so a portable task cannot be selected on an H7 at all, while `report`,
which names no HAL, is selected there unchanged. "Reusable across all hardware"
means across the hardware whose HALs have migrated. That is the ecosystem's to
fix, and worth knowing before ferro-wasp depends on it.

**Two protocols on one firmware, 2026-09-17.** `nucleo-f401re-beacon` receives
SBUS and drives an MSP DisplayPort OSD at once. The second port is a plain
interrupt-driven UART rather than a second use of the DMA UART tasks, because
those cannot serve a second port: `examples/tasks/stm32f4-uart-dma` names `USART1` and
`Stream2<DMA2>` as concrete types, and the streams a second port would need are
different types again. Tasks written against concrete peripherals serve one
port by construction.

Three things came out of building it. A dispatcher is an interrupt vector RTIC
borrows for software tasks, so a peripheral the firmware actually uses cannot
share one - USART6 stopped being a dispatcher before it could be a serial port.
A synchronous definition reads as a hardware task and so takes no inputs, which
is what rejected an earlier design that spawned a task per received byte; the
version that survived puts the parser step in the handler that already holds the
lock, where it costs nothing. And the protocol was split out of the task crate
into `msp`, an ordinary dependency-free library, because a crate that depends on
RTIC cannot carry a host test and framing is exactly the thing worth testing
before wiring anything up.

**Groups removed, 2026-09-18.** Task groups were an experiment, and the first
release carries none of their machinery: no `#[group]` block, no `Wiring`
trait, and no `app!` mirroring of priorities into a library. Tasks that only
work as a set are declared one by one like any others, and priorities are the
firmware's choice, unchecked, exactly as in RTIC. The experiment's limits were
part of the reason: a group named concrete peripherals and so could be selected
once, a protocol that did not own its transport did not fit, and the block
saved only the repeated `from`, `shared` and default priority.

**First release on crates.io, 2026-09-18.** FerroForge 0.1 is published to
crates.io rather than installed from git, so a tester's first step is
`cargo install ferroforge-cli` and a new project's dependency is
`ferroforge = "0.1"`. Four crates are published, in dependency order:
`ferroforge-contracts`, `ferroforge-macros`, `ferroforge` and `ferroforge-cli`.
Preparing it forced three corrections. The CLI's chip data lived outside its
crate, so a published CLI would not have compiled; it now lives in
`ferroforge-cli/backends/`. The declared minimum Rust was 1.85 while the macros
use let-chains, which need 1.88. And `new` wrote a firmware whose `init` was
`todo!()`, which would have panicked on first flash; it now writes one that
runs, specified in [workflow](workflow.md#the-cli).

A published version cannot be changed, only superseded, so from here a change
to the context a task receives, to `app!`'s grammar or to the CLI's verbs
reaches users only as a new version - 0.2 for anything that breaks them.

**0.2, 2026-09-18.** Breaking for 0.1 firmware: `app!` no longer accepts
`monotonic =` in its header, and the monotonic is declared inside `app!`
instead. A project made by `new` depends on `ferroforge = "0.2"`, because the
firmware it writes uses that form. The CLI gains `add`, `--all`, `drift` and the
chip table, and names the scaffolded task crate `tasks/heartbeat`.
