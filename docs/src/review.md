# Review and Open Decisions

Implementation baseline reviewed on 2026-09-05; the decision record incorporates
the subsequent walkthrough agreements. The findings below distinguish agreed
design, current behavior, and remaining implementation details. The
[implementation plan](implementation-plan.md#development-order) records the
agreed initial sequence and phase-completion gates.

## Current Decision - Governing Requirements (2026-09-16)

The user's final goal overrides conflicting earlier design. The canonical
[governing requirements](governing-requirements.md) state their own size limit
and the authority order used to resolve conflicts. Keep that page as the
requirements authority; detailed plans and implementation status belong in the
[repair plan](composition-repair-plan.md), and open decisions belong here.

**Interrupt binding resolved, 2026-09-16:** the user confirmed that "software"
was a spelling error. Hardware tasks require an interrupt binding; software
tasks do not. The typed interrupt enum requirement remains agreed. The governing
page now also includes the agreed per-firmware file structure; its implementation
remains pending.

**Backend split agreed, 2026-09-16:** separate the family platform backend
required by FerroForge for the selected hardware from optional reusable hardware
task crates. The platform backend owns chip definitions and required integration;
task crates own reusable hardware task definitions and task-specific helpers.
The backend must remain usable without those task crates. This supersedes the
earlier combined-backend ownership below; implementation remains pending.

**Shared HAL helpers agreed, 2026-09-16:** shared helpers and types live in their
own family crate, such as `ferroforge-stm32f4`, separate from platform backends
and reusable hardware task crates. Init and hardware tasks use the same shared
crate to preserve concrete type identity. Implementation remains pending.

**Component purposes and CLI agreed, 2026-09-16:** software task libraries are
intended for all hardware and projects; hardware task libraries are HAL-specific
and reusable across projects. A project's `firmware/` groups all its firmware
apps. Backends contain the minimum information FerroForge requires for family
support. `ferroforge-stm32f4` is a FerroForge-oriented STM32F4 HAL helper library.
FerroForge operates as a CLI, like Cargo, with expected project conventions and
an explicit library marker, as clarified below. These responsibilities are
agreed; the exact CLI, remaining file/manifest conventions, and marker support
are not implemented.

**Structure clarification agreed, 2026-09-16:** project structure is a convention
FerroForge expects, supported by a new-project helper command analogous to
`cargo generate`. It is not a general layout policing mechanism. Project layout
changes cause errors when required inputs can no longer be recognized; unrelated
additions or changes that preserve recognition do not warrant layout errors.
The earlier request for a stricter, explicitly invoked library compatibility
check is deferred by the subsequent decision below.

**Library marker agreed, 2026-09-16:** a library must explicitly identify itself
as a FerroForge library. Attempting to use a library without that marker must
produce an error. Per G7b, marker absence is the only library check and broader
library checks are out of scope. The marker requirement is agreed, while its
location, spelling, and implementation remain pending.

**Initial scope confirmed, 2026-09-16:** the first proof is a working F401
example carrying both task kinds, native synchronous interrupt handlers and
async software tasks. Other boards follow afterwards.

**Target ownership and format agreed, 2026-09-16:** the resolved host model is
the load-bearing artifact and every consumer uses it, so the authoring format
stays changeable. Minimal TOML board facts and a Rust interrupt enum may
coexist as long as they never state the same fact - the backend's Rust owns the
interrupt enum and per-chip validity, and the TOML does not name interrupts.
The definition belongs to a family backend crate per G2a, named distinctly from
the G2b HAL helper. A firmware's authored package declares only what its init
and composition need to compile; the backend supplies the platform-required
remainder. Chip- and architecture-derived features are target-owned, while the
logging and panic backends remain the firmware's choice. This closes the earlier
Cargo-centralization question: source manifests stay authoritative for their own
requirements and are checked against the resolved target.

**Interrupt enum and library marker agreed, 2026-09-16:** the interrupt enum is
typed per chip and owned by that chip's G2a platform backend, which re-exports
the PAC's existing per-chip `Interrupt` enum rather than maintaining a second
list of names. Validity needs no FerroForge check: the authored composition
names that type and is compiled for the selected chip, so an interrupt the chip
lacks fails the authored package's own check. No interrupt data is needed
host-side, in the target TOML, or in the resolved target model.
A FerroForge library marks itself with `library = true` under
`[package.metadata.ferroforge]`, reusing the manifest table that already carries
`check-only-dependencies`; a source comment was considered and rejected because
Rust discards `//` comments before the parser sees them. The declaration grammar
is Rust-native and RTIC-idiomatic, meaning Rust syntax parsed declaratively
rather than host Rust evaluated to build the graph.

**Next open point:** freeze the `composition!` declaration grammar with a
complete worked example before implementing a parser. Then resolve interrupt
enum ownership and selected-chip validation, then IDE preparation/refresh
mechanics, then the library marker and project recognition inputs. Open items
are tracked in the [repair plan](composition-repair-plan.md#decisions-still-open).

## Binding Decisions Carried Forward

Decisions from the initial walkthrough and the per-system composition discussion
that remain in force. Each is specified in full in the chapter named. This list
records that the decision was agreed and where it lives; it is not a second copy
of the rule. Rationale and evidence are in
`archive/2026-09-16/docs/src/review-historical.md`.

| Decision | Specified in |
| --- | --- |
| Resource-keyed bounds (`bounds = [led: StatefulOutputPin]`) with `local`/`shared` claims; no separate requirement structs | [architecture](architecture.md#software-task-clarifications) |
| RTIC-familiar `cx.local` and `cx.shared.<name>.lock(...)` access; both categories from the first SW scope | [architecture](architecture.md#local-and-shared-resources-in-the-initial-sw-scope) |
| Task-local `CONFIG.FIELD` reads, rendered as typed constants grouped per composed instance | [architecture](architecture.md#task-local-configuration-access) |
| Inputs as ordinary parameters; inline `spawn = [report(value: u32)]`; RTIC 2 result shapes, not `SpawnError::QueueFull` | [architecture](architecture.md#task-inputs-and-spawning) |
| SysTick 1 kHz / `u32` profile, supporting only `Mono::delay` and `Mono::start(SYST, u32)` | [architecture](architecture.md#systick-mock-direction) |
| Generic checking expansion behind a plain task-context signature; local refs and shared lock proxies | [architecture](architecture.md#generated-checking-contexts) |
| Related tasks share a source module with imports declared once at module scope | [architecture](architecture.md#modules-group-related-tasks) |
| Native HAL init; check-only interfaces generated from composition; complete body transplanted | [architecture](architecture.md#system-owned-initialization) |
| Logical-module supporting source carried whole; support namespaces separate per source module | [architecture](architecture.md#source-transplant-boundary) |
| Ordinary RTT and defmt syntax supported, including composition-dependent arguments | [architecture](architecture.md#native-rtt-and-defmt-logging) |
| Build only the API coverage concrete uses need; target-aware check and final build remain the verification | [architecture](architecture.md#incremental-coverage-and-early-checks) |
| Cargo manifests are the source of dependency requirements; conservative inclusion | [dependencies](dependencies.md#agreed-manifest-based-requirements) |
| `check-only-dependencies` declared in `[package.metadata.ferroforge]` | [dependencies](dependencies.md#agreed-check-only-dependency-setting) |
| Exact source, version, and default-feature match required to merge; differences diagnose | [dependencies](dependencies.md#agreed-initial-merging-and-conflict-policy) |
| Firmware layout, one authoritative target, and per-firmware `composition!` with its own init | G5; [repair plan](composition-repair-plan.md#confirmed-requirements) |
| Existing spawn mocks and init checking interfaces are reused, not redesigned | [repair plan](composition-repair-plan.md#existing-implementation-to-reuse) |

## Archived History

The per-system composition discussion and the initial walkthrough and backend
proofs were archived on 2026-09-16 to
`archive/2026-09-16/docs/src/review-historical.md`. That file holds the
rationale, the six-item walkthrough record, phase evidence, and implementation
status as they stood. Binding decisions from it are indexed above; the archive
is not active design authority and requires explicit permission to read.
