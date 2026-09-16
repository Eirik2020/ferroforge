# Review and Open Decisions

The decision record. Current decisions and the next open point are here;
requirements are in the [governing requirements](governing-requirements.md), and
what is left to build is in [remaining work](implementation-plan.md).

## Current Decisions

**Call-through model adopted, 2026-09-16.** FerroForge no longer transplants
source. A reusable task is an ordinary generic function in its own crate, and a
firmware's `compose!` expands in place into a real `#[rtic::app]` whose handlers
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
conventions and an explicit library marker. None of the CLI is implemented.

**Project conventions, 2026-09-16.** Project structure is a convention
FerroForge expects, supported by a new-project helper analogous to
`cargo generate`. It is not a layout policing mechanism: changes that preserve
recognition are allowed, and an operation errors only when required inputs can
no longer be recognized.

**Library marker, 2026-09-16.** A library marks itself with `library = true`
under `[package.metadata.ferroforge]`, reusing the table that already carries
`check-only-dependencies`. A source comment was considered and rejected because
Rust discards `//` comments before the parser sees them. Per G7b marker absence
is the only library check.

**Initial scope confirmed, 2026-09-16.** The first proof is a working F401
example carrying both task kinds. Other boards follow afterwards. This is met:
`firmware/nucleo-f401re` checks and release-links with a software blink
and a `TIM2` handler.

**Next open point:** backend data and the CLI, which is what makes target files
and the firmware manifest derived rather than hand-written. Then the agreed G5
layout, then end-to-end test coverage to replace what was removed with the
transplant path. Tracked in [remaining work](implementation-plan.md).

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
| A FerroForge library marks itself with `library = true` in the same table | [remaining work](implementation-plan.md#project-conventions-and-the-library-marker) |
| Firmware layout, one authoritative target, per-firmware `composition!` with its own init | G5 |

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

## Archived History

The transplant-era chapters were archived on 2026-09-16 under
`archive/2026-09-16/docs/src/`, and the code they describe is recoverable from
branch `main` (full transplantation) and `test/new_task_method` (transplanted
init). The archive is not active design authority and requires explicit
permission to read.
