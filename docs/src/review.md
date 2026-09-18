# Review and Open Decisions

The decision record. Current decisions and the next open point are here;
requirements are in the [governing requirements](governing-requirements.md), and
what is left to build is in [remaining work](implementation-plan.md).

## Current Decisions

**Call-through model adopted, 2026-09-16.** FerroForge no longer transplants
source. A reusable task is an ordinary generic function in its own crate, and a
firmware's `app!` expands in place into a real `#[rtic::app]` whose handlers
call it. Three designs were built and measured before choosing:

| Design | Task bodies | Init | `.text` |
| --- | --- | --- | --- |
| Full transplantation | copied, rewritten | copied | 6,908 |
| Transplanted init | called | copied into a generated project | 7,268 |
| Call-through | called | authored in place | 7,088 |

The call-through model removes the renderer, the mock layer, generated checking
interfaces, the generated project and the second Cargo invocation, at a code
size within 3% of transplantation. Only it satisfies the confirmed scope today.
The superseded designs are recoverable from branch `main` and
`test/new_task_method`.

Accepted costs: the macro-generated context is a versioned ABI between task
crates and firmware, so changing it breaks consumers; task crates now depend on
`rtic` and `rtic-monotonics`; and there is no generated source file to read,
only `cargo expand`. FerroForge is not mature and is free to break that ABI
while the design settles. It will co-evolve with the `ferro-wasp` flight
controller framework, which exercises it.

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
`firmware/nucleo-f401re` checks and release-links with a software blink
and a `TIM2` handler.

**Library marker dropped, 2026-09-17.** G7 previously required a crate to mark
itself a FerroForge library. Under the call-through model a composition names
the selected crate's own `Context`, `Local` and `Config`, so an unmarked crate
fails to compile and a marked crate without `#[task]` definitions fails anyway: the
marker was neither necessary nor sufficient, and requiring the firmware to list
its libraries put one fact in two places. G7 was reworded to say FerroForge adds
no separate library check - the same correction already made to G4.

**Project conventions settled, 2026-09-17.** A project is the nearest parent
directory holding a `firmware/`, recognized by walking up as Cargo does. No
marker file: one was considered and rejected, because the arc of this design has
been deleting FerroForge-specific files and convention already gives what a
marker would. Each firmware declares its chip under
`[package.metadata.ferroforge]`, which is where the last unrecorded chip fact
lived - previously it existed only in whatever path was typed on the command
line. The CLI's verbs are Cargo's, and `check`, `build` and `run` sync before
delegating so a build cannot consume a stale derived file.

**Backends ship with FerroForge, 2026-09-17.** A chip's memory map is not a
property of anyone's application, so backend data is embedded in the binary
rather than carried per project - otherwise an installed CLI could not find its
own chip data. A project-local override was considered and deferred: the case is
rare, and adding a chip upstream is a small TOML. This settles the *backend
packaging* half of the governing document's open question.

**Macros renamed, 2026-09-17.** `compose!` became `app!` and `#[reusable]`
became `#[task]`, so FerroForge uses RTIC's words for RTIC's ideas. The old
names described the implementation - what the macro did - where the new ones
describe what the author is declaring. This also closed a long-standing
inconsistency: G5 and three chapters named a `composition!` macro that never
existed under that name.

**A HAL task crate's chip feature belongs to the firmware's platform crates,
2026-09-17.** Adding a second chip exposed a real duplication, and not the one
that had been written down. `device` is not a chip fact - the HAL aliases `pac`
to whichever chip its feature selects, so that path is identical across the
family. What *was* duplicated is the chip feature a firmware enabled on a
HAL-specific task crate, which sits outside the region the CLI owns: switching
chip left it behind and the HAL rejected two chip features from a build script,
as a panic with no cause. A firmware must not enable it at all, because Cargo's
feature unification already supplies the chip; `sync` now refuses one and names
the dependency, the feature and the chip.

**Second chip added, 2026-09-17.** STM32F411RE, so the backend registry, the
`chips` listing and chip-switching are exercised by more than one entry rather
than by synthetic fixtures. Changing one line of a firmware's `Cargo.toml`
changes all four derived artifacts, and the same firmware source builds for
either chip.

**The `app!` header is RTIC's, 2026-09-17.** It was three fixed positions, all
mandatory; RTIC parses a loop with defaults. Swapping two arguments reported
"expected `device`", blaming the wrong one. Now: any order, duplicates named,
`dispatchers` optional and defaulting to empty as RTIC's does, and `peripherals`
passed through when stated rather than restated here where it could drift.

**Drift checking for code that must be duplicated, 2026-09-18.** Some code
cannot be a reusable task and has to be copied between firmwares. Copies marked
`// ferroforge:begin <name>` / `// ferroforge:end <name>` are compared by
`ferroforge drift`, one to one after ignoring layout and `//` notes. It is not
a way to share code: anything that can be a task should be one. Nested regions
are compared individually, outer first; unmatched markers are errors. Settings
such as ignoring interrupt bindings will come from one project-wide file, and
without it the comparison stays one to one - see
[remaining work](implementation-plan.md). Specified in
[workflow](workflow.md#the-cli).

**The monotonic is declared inside `app!`, 2026-09-18.** The `monotonic = Mono`
header argument is gone; RTIC has no such argument, and the declaration already
names the type. `app!` reads it from any `<timer>_monotonic!(Name, ..)` item in
the application, refuses a second, and points a header that still names one at
the declaration. This replaces "above `app!`" and the header naming in the
decision below; the rest of it stands. See
[architecture](architecture.md#firmware-composition).

**`monotonic_hz` replaced by `monotonic`, 2026-09-17.** It had no RTIC
counterpart, invented a frequency, and made every application claim SysTick
whether or not anything read time. A firmware now declares its own monotonic
above `app!` and starts it in `init`, exactly as an RTIC user does, and names it
so adapters can hand the type to tasks. That last part is not RTIC's, and cannot
be: a task crate is generic over the clock, so something must supply the type.
An application declaring none gets an uninhabited `NoMonotonicDeclared` in the
slot, so a task that needs a clock fails against a name that says why. The 1 kHz
bound is unchanged, but now visible where it is chosen.

**Dispatchers are hand-selected, 2026-09-17.** Which interrupts are free depends
on the application's own peripheral use, so the author is the only one who can
choose; `app!` passes the list through unchanged and adds nothing. A backend pool
of candidates was considered and rejected as machinery for a choice that is not
FerroForge's to make.

The concern that recorded this as a gap - that naming a dispatcher the
application also uses would be a silent conflict - was wrong. RTIC diagnoses both
mistakes on the authored line: a dispatcher that is also bound reports
"dispatcher interrupts can't be used as hardware tasks" at the `binds`, and too
few report "not enough interrupts to dispatch all software tasks (need: 2;
given: 1)". Both are pinned in the compile-fail suite, because the pass-through
is what keeps them reaching the author.

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

**A second HAL, 2026-09-17.** `firmware/nucleo-h753zi` is a Cortex-M7 on
`stm32h7xx-hal`, with `tasks/stm32h7-timer` as the F4 timer task's counterpart.
Three things came out of it.

`device` is confirmed a HAL fact and not a constant: it is `stm32h7xx_hal::pac`
here, where every F4 chip shares `stm32f4xx_hal::pac`.

The memory model needed extending, as expected. An H7 has DTCM, AXI SRAM, four
SRAMs, backup SRAM and ITCM; `[[memory.region]]` now carries whatever a part has
beyond the `FLASH`/`RAM` pair `cortex-m-rt` requires. Nothing is placed in them
automatically - which memory suits which data is the application's decision, and
a backend choosing would choose for every firmware on that chip.

The unexpected finding is a limit on G1 rather than on this design.
`tasks/blinky` bounds on `embedded-hal` 1.0 and `stm32h7xx-hal` 0.16 implements
only 0.2, so a portable task cannot be selected on an H7 at all, while `report`,
which names no HAL, is selected there unchanged. "Reusable across all hardware"
means across the hardware whose HALs have migrated. That is the ecosystem's to
fix, and worth knowing before ferro-wasp depends on it.

**Two protocols on one firmware, 2026-09-17.** `nucleo-f401re-beacon` receives
SBUS and drives an MSP DisplayPort OSD at once. The second port is a plain
interrupt-driven UART rather than a second use of the DMA UART tasks, because
those cannot serve a second port: `tasks/stm32f4-uart-dma` names `USART1` and
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

**If RTIC does not check it, neither does FerroForge, 2026-09-18.** Binding a
task to an interrupt that belongs to a different peripheral builds - the DMA
receive task on `DMA2_STREAM3`, the `TIM3` timer task on `TIM2` - and that is
not a gap, because plain RTIC accepts it too. Specified in
[architecture](architecture.md#what-checking-guarantees).

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

**Next open point:** none in FerroForge itself. What is left in
[remaining work](implementation-plan.md) concerns the example task crates and
firmware, not the tool.

## Binding Decisions Carried Forward

Decisions that remain in force. Each is specified in full in the chapter named;
this list records that it was agreed and where it lives, not a second copy of
the rule.

| Decision | Specified in |
| --- | --- |
| Resource-keyed bounds (`bounds = [led: StatefulOutputPin]`) with `local`/`shared` claims; no separate requirement structs | [architecture](architecture.md#task-authoring) |
| RTIC-familiar `cx.local` and `cx.shared.<name>.lock(...)` access; both categories in scope | [architecture](architecture.md#task-authoring) |
| Task-local `CONFIG.FIELD` reads, declared `config = [period_ms: u32]` | [architecture](architecture.md#task-authoring) |
| Inputs as ordinary parameters; inline `spawn = [report(value: u32)]`; RTIC 2 result shapes | [architecture](architecture.md#task-authoring) |
| SysTick 1 kHz / `u32` profile | [architecture](architecture.md#task-authoring) |
| Related tasks share a source module with imports declared once at module scope | [architecture](architecture.md#task-authoring) |
| Task kind follows the signature: `async fn` software, `fn` hardware with a composition-supplied interrupt | [architecture](architecture.md#task-authoring) |
| Native HAL init, authored in the firmware and never moved | [architecture](architecture.md#firmware-composition) |
| Build only the coverage concrete uses need; the final target build remains the verification | [architecture](architecture.md#incremental-coverage) |
| Cargo manifests are the source of dependency requirements | [dependencies](dependencies.md#where-requirements-live) |
| `check-only-dependencies` declared in `[package.metadata.ferroforge]` | [dependencies](dependencies.md#check-only-dependencies) |
| Firmware layout, one authoritative target, per-firmware `app!` with its own init | G5 |

## Superseded by the Call-Through Model

These were agreed under the source-transplant design and no longer apply. They
are recorded so a reader meeting them in older material knows they are dead.

| Decision | Why it no longer applies |
| --- | --- |
| Generic checking expansion behind a mock task context | The context is real; there is no mock layer. |
| Logical-module supporting source carried whole, with separate generated namespaces | Nothing is copied between crates, so no namespace isolation is needed. |
| Ordinary RTT and defmt syntax must survive rewriting | Arguments are never rewritten, so logging works natively. |
| Conservative dependency inclusion, exact-match merging and conflict diagnostics | Cargo resolves task crate requirements transitively. |
| Check-only interfaces generated from composition for init checking | Init is authored in place; RTIC generates the real context. |
| A per-chip interrupt enum owned by the platform backend | RTIC resolves `binds` against the device's own enum. |
| Existing spawn mocks and init checking interfaces are reused, not redesigned | Both were removed with the transplant path. |
| An explicit library marker, checked before a crate is selected | The marker told the renderer whose source it could parse. A composition now names the crate's own `Context`, `Local` and `Config`, so an unmarked crate already fails to compile and a marked one without `#[task]` definitions still does; the marker was neither necessary nor sufficient. G7 rewritten 2026-09-17. |

## Archived History

The transplant-era chapters were archived on 2026-09-16 under
`archive/2026-09-16/docs/src/`, and the code they describe is recoverable from
branch `main` (full transplantation) and `test/new_task_method` (transplanted
init). The archive is not active design authority and requires explicit
permission to read.
