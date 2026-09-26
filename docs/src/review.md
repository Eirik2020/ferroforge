# Review and Open Decisions

The decision record. Current decisions and the next open point are here;
requirements are in the [governing requirements](governing-requirements.md), and
what is left to build is in [remaining work](implementation-plan.md).

## Current Decisions

**Any tick rate, counting in `u64`, 2026-09-26.** A task reading time accepts
whichever monotonic the firmware declares; the rate is inferred, and the width
is fixed at `u64` so every task runs under every firmware. It replaces the 1 kHz
`u32` profile, which ruled out every hardware-timer monotonic, since those count
in `u64`, and gave a 2 kHz control loop 1 ms timestamps. Breaking for 0.3 task
crates that read time. A firmware counts on SysTick, which every backend builds
64-bit, or on a hardware timer with `monotonic-timer`, whose chip and timer
features the backend supplies. Specified in [task
authoring](architecture.md#task-authoring) and [the CLI](workflow.md#the-cli).

**`ferroforge-stm32f4` deferred, 2026-09-26.** ferro-wasp uses its own STM32F4
helper, `ferrowasp-stm32f4`, not a FerroForge one. [G2b](governing-requirements.md)
is unchanged and the crate it names is still owed, but no consumer is waiting
for it ([the STM32F4 helper](implementation-plan.md#the-stm32f4-helper)).

**The firmware owns what goes into the files the CLI writes, 2026-09-26.** The
two open CLI decisions are settled the same way, because they were the same
question: a firmware records in `[package.metadata.ferroforge]` anything that
ends up in a file `sync` generates, and the CLI writes it. That is already why
`defmt-log` and `defmt-location` are declared rather than passed, so neither
decision needed a new mechanism, a project-level file, or a parent
`.cargo/config.toml`.

- **Probe arguments and environment variables.** `probe-command`, `probe-args`
  and `env`, specified in [the CLI](workflow.md#the-cli). The runner is emitted
  as Cargo's argument list instead of one string, so a firmware's arguments need
  no quoting rules.
- **A platform crate's source.** `[package.metadata.ferroforge.platform.<crate>]`
  takes any source Cargo accepts, specified in [where a platform crate comes
  from](workflow.md#where-a-platform-crate-comes-from).

[G2a](governing-requirements.md) is unchanged and was the reason for drawing the
line where it is drawn: a backend still decides *which* platform crates a chip
needs and which chip features they carry, and a firmware says only *where* one is
fetched from. Which crates and which chip features are chip-family data; a git
revision is a project's supply chain. So the chip feature survives an override,
and selecting a different chip still rewrites the block correctly - the property
the generated block exists for.

Settings the CLI itself writes are refused rather than merged, naming the setting
that owns each, and an unknown key under `[package.metadata.ferroforge]` is an
error: a misspelled setting that is silently ignored looks applied and is not.

Checked against the case that raised both questions rather than against a
fixture: the Foxeer app's hand-written runner and its pinned HAL revision are
reproduced argument for argument and feature for feature. Two deliberate
differences remain - the generated config adds `-L.`, which makes ferro-wasp's
`memory.x` copy redundant, and `CHIPSERIE` is dropped, because nothing reads it.

**FCU3 retired, 2026-09-22.** ferro-wasp's second flight board is obsolete.
Its test gates are retired and its procedure archived; its firmware stays in
tree and keeps compiling so a second board can be revived cheaply. The FCU3
drift table is gone from [ferro-wasp adoption](ferro-wasp-adoption.md), and
with it the last open question about whose actuator validation is right:
Foxeer's is. The cost is that nothing in ferro-wasp now exercises one
definition across two boards, which is [G1](governing-requirements.md)'s
central claim - FerroForge's own task crates and tests carry it instead.

Consequently Foxeer's `usb_fs` and `flash_manager_task` become definitions
shaped by Foxeer alone, rather than waiting for a second board that is not
coming.

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
The superseded designs are recoverable from `main`'s history; see [archived
history](#archived-history).

Accepted costs: the macro-generated context is a versioned ABI between task
crates and firmware, so changing it breaks consumers; task crates now depend on
`rtic` and `rtic-monotonics`; and there is no generated source file to read,
only `cargo expand`. FerroForge is not mature and is free to break that ABI
while the design settles. It will co-evolve with the `ferro-wasp` flight
controller framework, which exercises it.

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

**An instance may supply a local as an RTIC task-local, 2026-09-18.** For a
local the definition leaves to the firmware, the instance may write RTIC's
own `name: Type = value` instead of routing it through `#[local]` and `init`.
This complements, and does not reverse, initial values belonging to the
definition: that rule is for values intrinsic to the task, and a definition's
constant cannot depend on the board. Foxeer's ADC task needed it - its battery
cell detector starts from the board's calibration profile. Specified in
[architecture](architecture.md#task-authoring).

**A bare binding names the resource of the same name, 2026-09-18.** An
instance writes `local = [osd_uart]` for `local = [osd_uart = osd_uart]`, as
RTIC writes a claim. Converting ferro-wasp's Foxeer app showed why: its OSD
task binds 21 resources, each under the name the firmware already uses.
Specified in [architecture](architecture.md#firmware-composition).

**Lock-free shared resources are marked `#[lock_free]`, 2026-09-18.** A
definition writes `shared = [#[lock_free] rx: T]` and receives `&mut T`. RTIC's
word, as an attribute on the resource, the way RTIC marks the `Shared` field;
a separate `lock_free = [..]` list was rejected because RTIC has none. Needed
for ferro-wasp's UART receive handlers. Specified in
[architecture](architecture.md#task-authoring).

**Configuration is read as `CONFIG::FIELD`, 2026-09-18.** It was
`CONFIG.FIELD`, rewritten by `#[task]` into an associated constant - the last
body rewrite, and one that could not reach inside a macro call, so
`defmt::info!("{=u32}", CONFIG.PERIOD_MS)` did not compile. The configuration's
type parameter is now named `CONFIG`, and a read is its associated constant,
written as one. Two alternatives were weighed: a `let CONFIG` value opening
every body, which keeps the old spelling at the cost of generated code in each
task, and `cx.config.field` on the context, which is closest to RTIC but stores
the values in each task's state. Neither sizes an array: a generic's constant
cannot, on stable Rust, however it is spelled. Breaking for 0.2 task crates.
Specified in [architecture](architecture.md#task-authoring).

**Task-local initial values belong to the definition, 2026-09-18.** A
definition may declare `local = [name: Type = value]`, RTIC's own form, and
the value is the task's: a firmware selecting it binds only the locals that
have none. Putting the values on each instance instead was rejected, because
a task like ferro-wasp's `flash_manager_task` would have its twenty initial
values repeated in every firmware that selects it. Specified in
[architecture](architecture.md#task-authoring).

**The monotonic is declared inside `app!`, 2026-09-18.** The `monotonic = Mono`
header argument is gone; RTIC has no such argument, and the declaration already
names the type. `app!` reads it from any `<timer>_monotonic!(Name, ..)` item in
the application, refuses a second, and points a header that still names one at
the declaration. It replaced a monotonic declared above `app!` and named in its
header. See [architecture](architecture.md#firmware-composition).

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

**If RTIC does not check it, neither does FerroForge, 2026-09-18.** Binding a
task to an interrupt that belongs to a different peripheral builds - the DMA
receive task on `DMA2_STREAM3`, the `TIM3` timer task on `TIM2` - and that is
not a gap, because plain RTIC accepts it too. Specified in
[architecture](architecture.md#what-checking-guarantees).

**Next open point:** none blocking. [Remaining work](implementation-plan.md)
holds one undecided design, drift settings, and the deferred STM32F4 helper
[G2b](governing-requirements.md) names; neither blocks the known defects or the
ferro-wasp migration.

## Binding Decisions Carried Forward

Decisions that remain in force. Each is specified in full in the chapter named;
this list records that it was agreed and where it lives, not a second copy of
the rule.

| Decision | Specified in |
| --- | --- |
| Resource-keyed bounds (`bounds = [led: StatefulOutputPin]`) with `local`/`shared` claims; no separate requirement structs | [architecture](architecture.md#task-authoring) |
| RTIC-familiar `cx.local` and `cx.shared.<name>.lock(...)` access; both categories in scope | [architecture](architecture.md#task-authoring) |
| Task-local `CONFIG::FIELD` reads, declared `config = [period_ms: u32]` | [architecture](architecture.md#task-authoring) |
| Inputs as ordinary parameters; inline `spawn = [report(value: u32)]`; RTIC 2 result shapes | [architecture](architecture.md#task-authoring) |
| The firmware's monotonic, at any rate, counting in `u64` | [architecture](architecture.md#task-authoring) |
| Related tasks share a source module with imports declared once at module scope | [architecture](architecture.md#task-authoring) |
| Task kind follows the signature: `async fn` software, `fn` hardware with a composition-supplied interrupt | [architecture](architecture.md#task-authoring) |
| Native HAL init, authored in the firmware and never moved | [architecture](architecture.md#firmware-composition) |
| No task groups; priorities are the firmware's, as in RTIC | [architecture](architecture.md#firmware-composition) |
| A published version never changes; a breaking change is a new minor version under 0.x | [workflow](workflow.md#release) |
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
`archive/2026-09-16/docs/src/`. The code they describe is in `main`'s history:
full transplantation at `fbe54a4`, the last commit before work on the
replacement began, and transplanted init at `fb92511`, the last before its
removal and the tip of `test/new_task_method`, which holds nothing `main` does
not. The archive is not active design authority and requires explicit
permission to read.
