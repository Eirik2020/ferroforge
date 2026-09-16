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
produce an error. Broader library checks are deferred and will be added as
needed; a comprehensive checker is not a prerequisite for this work. The marker
requirement is agreed, while its location, spelling, and implementation remain
pending.

**Next open point:** choose the library marker's location and syntax. Then settle
minimum project recognition inputs, new-project helper output, and library
references. The proposed separation of project, library,
and tool locations is in the [repair plan](composition-repair-plan.md#cli-projects-and-reusable-libraries).
Then resolve interrupt enum ownership/validation, review concrete RTIC-like
authoring examples, and settle IDE preparation/refresh mechanics. Compatible
earlier requirements below continue to apply.

## Binding Decisions Carried Forward

Promoted on 2026-09-16 from the initial walkthrough and the per-system
composition discussion when those sections were archived. These remain in
force. Their rationale, evidence, and implementation status are in
`archive/2026-09-16/docs/src/review-historical.md`.

### Task Authoring

- Resource-keyed bounds: `bounds = [led: StatefulOutputPin]` alongside
  `local = [led]` or `shared = [led]`. Concrete resources carry inline types
  such as `toggle_count: u32`. This supersedes the earlier `led: Trait`
  shorthand and the type-placeholder proposals.
- Keep RTIC-familiar access: `local = [...]`, `shared = [...]`, `cx.local`,
  and `cx.shared.<name>.lock(...)`. Both local and shared resources are in
  scope, including state that persists across invocations.
- Incoming inputs are ordinary Rust parameters after the context. There is no
  separate input declaration.
- Outgoing spawns use inline signatures such as `spawn = [report(value: u32)]`.
  Spawn results follow RTIC 2: `()` for no inputs, the value for one input, a
  tuple for several. The prototype's `SpawnError::QueueFull` is not kept.
- RTIC 2's pending/running-instance restriction holds. Compile-only mocks do
  not simulate scheduling.
- Configuration is read task-locally as `CONFIG.FIELD`, not
  `task::Config::FIELD`. A generated immutable typed view supplies the fields.
- Related tasks share a source module with imports declared once at module
  scope; each task keeps its own typed requirements and context.
- Resource requirements stay inline with each reusable task. Explicit
  `Local`/`Shared` requirement structs were considered and not selected.

### Checking Mechanism

- Macro-generated generic functions and contexts are the checking mechanism.
  The author keeps a plain task-context signature; expansion introduces the
  resource type parameters, lifetimes, and declared trait bounds.
- Local fields generate as ordinary mutable references; shared fields as typed
  borrowing proxies with a checked `lock` closure.
- SysTick profile: 1 kHz, `u32`-backed time values, core clock source, using
  real `fugit::Duration<u32, 1, 1_000>` and `ExtU32`. Init checks
  `Mono::start(SYST, core_clock_hz)`. `now()`, `delay_until()`, timeouts,
  alternative clock sources, and 64-bit profiles are deferred.
- Configuration constants collect in one marked namespace ahead of the
  generated app's resource structs and init, grouped by composed task instance
  with instance-qualified paths. Composition stays the source of truth; values
  are not inlined as task-body literals.

### Init

- Initialization uses native HAL types, traits, and methods with real PAC and
  Cortex-M peripherals. FerroForge introduces no hardware-trait replacements or
  HAL wrapper API for initialization.
- Check-only init interfaces are generated from composition: a mock
  `init::Context` carrying real peripheral types, typed spawn entry points for
  selected instances, and the selected monotonic startup API.
- The host reads source and metadata without linking target implementations.
- Transplantation keeps the complete init body with native HAL calls intact;
  generated firmware uses the real RTIC and monotonic equivalents.

### Supporting Source

- A logical Rust module groups related tasks with their imports and supporting
  code. File-backed modules, inline modules, and a crate root used as one group
  are all valid. No FerroForge module wrapper or grouping annotation is needed.
- Tasks are selected individually; selecting one does not instantiate its
  siblings or merge their resources.
- The module's supporting imports, helpers, types, implementations, and
  constants carry across together, without a handwritten helper list. Unused
  supporting items may be retained and must still compile.
- Support namespaces stay separate across source modules. Tasks and instances
  from one module share supporting type identity; instantiating a task twice
  does not duplicate module-level state.
- Complete task bodies stay in real RTIC handlers. Reusable tasks do not become
  runtime library callbacks, and authors do not expose private helpers purely
  for transplantation.

### Logging

- Ordinary RTT and defmt syntax is supported scope, including
  composition-dependent expressions inside logging arguments. FerroForge
  logging wrappers are not required, and arguments are not moved into
  handwritten temporaries. Format strings, hints, native macro calls, argument
  evaluation, and module-level imports and aliases are preserved.

### Dependencies

- Task and init crates' normal Cargo manifests are the source of requirements.
  There is no second per-task dependency list and no duplicated Rust version
  registry.
- Include dependencies needed by retained supporting code, not only by selected
  task bodies. Carry each participating source crate's applicable normal
  dependencies conservatively; precise pruning is deferred.
- Check-only dependencies are named in `[package.metadata.ferroforge]` as
  `check-only-dependencies = [...]`. They stay available to the source check and
  are excluded from generated manifests. This role is not inferred from crate
  names or procedural-macro status. Manifest exclusion alone is insufficient:
  the corresponding imports and attributes are removed or translated during
  transplantation.
- The system supplies HAL, RTIC, monotonic, logging-backend, and panic choices
  without silently overriding task or init requirements. Matching sources,
  version requirement strings, and effective default-feature settings permit
  combining requested features; differences produce diagnostics and require
  alignment. Differently written but potentially compatible requirements are
  rejected initially.

### Composition and Firmware

- Each firmware authors `composition!` together with its hardware init.
- The family backend owns the concrete chip target definitions that firmware
  selects.
- One target-checked authored package per firmware workspace holds composition
  and handwritten init, with shared host generation tooling rather than a
  per-firmware host composer executable.
- Betaflight's target organization is the requested structural reference for
  firmware grouping.
- Rust Analyzer type checking of authored source is a requirement, not an
  optional convenience.

### Scope Limits

- Build only the API coverage current examples need, and extend it when real
  uses arise. Exhaustive monotonic coverage is not a prerequisite.
- Add useful, inexpensive early checks, but accept that some mistakes surface
  in the generated application's Rust Analyzer or Rust/RTIC checks. A mock
  check is not a promise that the final app compiles. Target-aware
  `cargo check` and the final build remain the verification steps.
- Keep host simulation separate from compile-only mock support.
- Typed task-local spawn mocks, composition-derived init spawn mocks, and
  authored-source checking tests already exist. Reuse them; do not reopen the
  mock design or claim it is unimplemented.

## Archived History

The per-system composition discussion and the initial walkthrough and backend
proofs were archived on 2026-09-16 to
`archive/2026-09-16/docs/src/review-historical.md`. That file holds the
rationale, the six-item walkthrough record, phase evidence, and implementation
status as they stood. Binding decisions from it are promoted above; the archive
is not active design authority and requires explicit permission to read.
