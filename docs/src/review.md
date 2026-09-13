# Review and Open Decisions

Implementation baseline reviewed on 2026-09-05; the decision record incorporates
the subsequent walkthrough agreements. The findings below distinguish agreed
design, current behavior, and remaining implementation details. The
[implementation plan](implementation-plan.md#development-order) records the
agreed initial sequence and phase-completion gates.

These six topics record the completed initial walkthrough, while keeping
remaining implementation and verification visible. Recording a recommendation
does not select it.
After recording an agreement, introduce the next open point in the same response.
The initial walkthrough reached agreement on item 6's implementation order and
acceptance criteria. The agreed centralized system layout and its first
explicit-Rust composition frontend are now implemented. The bounded Phase 5
pipeline and its real source/composition/init/render/check/link failure proofs
are complete. The next open point is the concrete second system for Phase 6:
it must reuse `tasks/blinky` unchanged while exercising different system-owned
instance, resource, and configuration bindings. Remaining details below are not
automatically resolved or implemented.
Manifest-based requirements, conservative source-crate dependency inclusion,
explicit check-only metadata, and conservative dependency merging are agreed.
The [Phase 1 foundation](implementation-plan.md#progress-record---phase-1-foundation)
now implements shared task parsing, Cargo-aware package/library-root resolution,
module discovery/selection, and structural validation of the initial standalone
resource/configuration/spawn/profile bindings. Initial manifest collection,
check-only classification, and conservative dependency merging are also
implemented and now drive bounded standalone project emission. The complete
initial STM32F401RE target package is emitted and release-linked; generalized
frontend ergonomics, additional target profiles, and cross-system reuse remain
work. The
bounded Phase 2 exit gate is now complete: the SW fixture checks independently,
focused invalid uses report authored-source compiler locations, an opt-in Rust
Analyzer scan records authored-source diagnostics, and renderer-emitted
sibling-module RTIC source checks on ARM. The initial explicit-Rust frontend and
first-system package now live together under `systems/nucleo-f401re`; this does
not complete generalized frontend ergonomics or cross-system reuse. The
bounded Phase 3 gate is also complete: the native-HAL init package checks on ARM
against regenerated interfaces, and an opt-in Rust Analyzer scan reports the
focused spawn/startup errors on their authored init lines. Phase 4 now has a
bounded combined ARM proof that transplants that checked init package into the
real-RTIC task output, emits its merged Cargo manifest and complete initial
STM32F401RE target package, and checks and release-links the generated project.
The bounded Phase 4 exit gate is met. The bounded Phase 5 exit gate is also met:
the runnable first-system command executes each Cargo stage in order, and both
injected-command and real compiler/renderer/linker regressions prove that a
failure is reported at its boundary and short-circuits dependent stages.

Item 1's initial SW resource declaration form is agreed:
resource-keyed bounds, inline concrete types, and an RTIC-shaped task signature.
Ordinary incoming parameters, inline typed outgoing spawn aliases, and the
minimal 1 kHz/32-bit SysTick profile are also agreed as a starting point.
Generic checking expansion and an immutable typed `CONFIG` view are the
selected checking mechanisms. Item 2's native HAL initialization and
composition-generated checking interfaces are also agreed. Remaining API
details, renderer integration, and verification are still work; these design
agreements do not claim complete implementation of either item.

Item 3's initial translation direction includes ordinary native RTT/defmt
logging. Item 4's logical-module supporting-source boundary is now agreed,
including ordinary file-backed modules. Remaining API boundaries, source
namespace/visibility handling, and integration proofs stay visible
below; advancing the discussion does not mark them implemented.

Implementation scope agreed during the SW walkthrough: build only the API
coverage needed by current examples and extend it when real uses arise. In
particular, exhaustive monotonic coverage is not a prerequisite. Add useful,
inexpensive early checks, but it is acceptable for some mistakes to surface
in the generated application's Rust Analyzer or Rust/RTIC checks. Keep those
limits explicit; a mock check is not a promise that the final app compiles.
Target-aware `cargo check` and the final build remain the verification steps.

| Item | Topic | Status |
| --- | --- | --- |
| 1 | Typed contract for independent task checking | Initial generic resource/config/spawn/delay checking, authored-source IDE evidence, and sibling-module real-RTIC emission implemented; full integration remains |
| 2 | Independent system init contract | Initial native-HAL ARM checker, composition-generated context/spawn/SysTick interfaces, authored-file IDE evidence, and bounded real-RTIC transplant implemented; broader integration remains |
| 3 | Supported mock API and real RTIC translation | Initial direction and native logging agreed; API details/integration remain |
| 4 | Supporting source and reference scope | Discovery, sibling-module layout, and contained relative paths implemented; cross-boundary/import/macro cases remain |
| 5 | Validation guarantees and dependency consistency | Initial collection, check-only filtering, structural validation, conservative merge, standalone manifest emission, and first-system integration implemented; legacy migration/general frontend work remains |
| 6 | Implementation order and acceptance criteria | Bounded Phase 2 through Phase 5 gates met; Phase 6 second-system reuse proof is next |

### 1. Typed Contract for Independent Task Checking

Finding: `local = [led]`, `config = [period_ms]`, and `spawn = [report]`
declare names without specifying the resource interface, configuration type,
or spawn signature. The current macro obtains resource bindings, configuration
constants, and spawn methods from a surrounding `app!` in the same crate.
See [mock macro expansion](prototype.md#mock-macro-expansion),
`expand_task` and `expand_rtic_app`.

Discussion record:

- Focus on SW tasks using real `embedded-hal` interfaces. HW tasks using
  chip HALs such as `stm32f4xx-hal` remain a separate, deferred discussion.
- Use `embedded_hal::digital::StatefulOutputPin` for an LED that needs
  toggling. System init constructs the concrete pin; composition binds it.
  The check must preserve the real trait bounds and error types.
- Use a plain `u32` for `period_ms`. Composition supplies the value; duration
  conversion must preserve its units and match the selected clock API.
- Use task-local `CONFIG.PERIOD_MS` instead of `blink::Config::PERIOD_MS` in task
  bodies. A generated task-local immutable typed view is the agreed checking mechanism;
  its fields derive from `config = [...]`. Resolve reads per composed instance.
  Rewrite reads to named typed constants, preserving declared types such as
  `u32`. Keep the initial scope to direct task-body field reads.
- Check configuration field names, types, and supported usage independently of
  a system's actual values. Value-dependent rules, such as a nonzero period,
  require the composition values and are not proved by this source check.
  Configuration in array lengths or other constant-expression positions is
  outside the initial checking scope. The view is implemented for direct fields
  of the initial unqualified `u32` type; broader types and renderer integration
  remain work, not a request for handwritten task boilerplate.
- Collect composed configuration constants in one marked namespace before the
  generated app's resource structs and init. Group by composed task instance
  and use instance-qualified paths. Composition remains the source of truth;
  do not inline configuration values as task-body literals or maintain the
  generated constants by hand. No separate prelude or `config.rs` is needed
  initially; file extraction can be considered if the section grows.
- Provide a compile-only mock of the supported SysTick monotonic API.
  The agreed initial profile is 1 kHz, `u32`-backed time values, and the core
  clock source. Use real `fugit::Duration<u32, 1, 1_000>` and `ExtU32` for
  `Mono::delay(duration).await`. Init checks `Mono::start(SYST, core_clock_hz)`
  with the actual peripheral and a `u32` frequency from HAL clock setup.
  The core frequency is distinct from the monotonic tick rate.
- Require matching checking/generated clock profiles and diagnose unsupported
  combinations. The mock may panic if executed; no host sleep or scheduler
  simulation is needed. Defer `now()`, `delay_until()`, timeouts, alternative
  clock sources, and 64-bit profiles until a concrete use requires them.
  Final firmware uses the real monotonic, started by system init. The profile
  is agreed. The task-side `ferroforge::mock::systick::Mono::delay` spelling and
  expansion are implemented. The generated init checker supplies
  `start(SYST, u32)`; the bounded standalone init transplant rewrites that call
  to the generated real backend.
- The authoring direction is to generate each task's mock context and typed
  accessors from its requirements. Developers write the complete task body,
  not extra handwritten context structs for every task. FerroForge owns
  reusable mock support; those mocks do not enter the generated firmware.
- Use macro-generated generic functions and contexts as the initial checking
  mechanism. The author keeps the plain task-context signature; expansion
  introduces resource type parameters/lifetimes and enforces declared real
  trait bounds on the checking function. No concrete dummy pin or system
  binding is needed to check the body.
- Generate local fields as ordinary mutable references to the generic resource
  or declared concrete type, and shared fields as typed borrowing proxies with
  a checked `lock` closure. The renderer transplants the original complete task
  source, not the generic checking structs/function or a call to that function.
- The standalone task macro and committed compiler regression suite validate
  this resource representation, including rejection of an undeclared pin
  method, wrong concrete resource assignment, and a reference escaping a shared
  lock. The suite asserts that all focused failures name their authored source
  lines. An opt-in Rust Analyzer 1.98.0 scan also reports the invalid method and
  value at their authored task-body lines; it does not currently report the
  shared-lock higher-ranked lifetime error, so rustc remains authoritative for
  that case. Broader task-shape lifetimes and end-to-end firmware integration
  remain work.
- Related tasks share a source module, with imports declared once at module
  scope. Tasks retain separate typed requirements and contexts. The renderer
  must preserve that import environment; see item 4 for extraction rules.
- Use resource-keyed `bounds = [led: StatefulOutputPin]` together with
  `local = [led]` or `shared = [led]`. The bound names the resource, not a
  separate generic type placeholder. Concrete resources carry inline types,
  such as `toggle_count: u32` and `enabled: bool`. The macro generates its
  internal generic checking types; the author keeps a plain task context
  signature. This selected form supersedes the earlier direct `led: Trait`
  resource shorthand and type-placeholder proposals. It requires neither
  `impl` nor `led: Led`, and is implemented on the standalone checking path.
- Both RTIC local and shared resources are required in the initial SW scope,
  including resource state that persists across task invocations. Ordinary
  function variables are not a replacement for these resources.
- Keep the familiar `local = [...]` and `shared = [...]` claims, direct
  `cx.local` access, and `cx.shared.<name>.lock(...)` access. The renderer
  records the category and binding of each resource and preserves them in
  generated RTIC declarations. Full ownership/locking validation is not an
  initial requirement; add straightforward checks and use the generated
  Rust/RTIC checks for the remaining errors.
- Keep resource requirements inline with each reusable task. The suggestion
  to add explicit `Local`/`Shared` requirement structs to reusable task modules
  was not selected. The user's RTIC-familiarity request concerns declaration
  style, not an extra resource section. Concrete system resource structs and
  system init ownership are unchanged. Resource-keyed bounds distinguish
  trait requirements from inline concrete types.
- Keep incoming task inputs as ordinary Rust parameters after the context,
  without a separate input declaration. Inputs belong to each invocation;
  local/shared resources persist and configuration is fixed by composition.
- Use inline outgoing signatures such as `spawn = [report(value: u32)]` as
  the initial authoring form. Generate typed `cx.spawn.report(value)` mock
  methods from the caller's contract, without requiring a destination task
  implementation. Composition binds each alias to a compatible task instance;
  the renderer translates the call to that instance's real `name::spawn(...)`.
- Derive RTIC-compatible spawn results from the input signature: failure
  returns `()` for no inputs, the value for one input, or a tuple for multiple
  inputs. Do not keep the prototype's custom `SpawnError::QueueFull` API.
  Preserve task-owned failure handling during transplantation.
- Preserve RTIC 2's pending/running-instance restriction. Spawning does not
  deliver messages to a running task; the forever-running blink instance is
  spawned once and can be controlled through its shared flag. Compile-only
  mocks do not need to simulate scheduling.
- Start with calls checked against declared signatures, complete alias/target
  bindings, and obvious argument-count checks. The generated Rust/RTIC check
  may catch remaining compatibility errors; do not require a general Rust
  type resolver in the renderer. The independent checking API implements this
  starting point; the bounded task and init transplants now perform direct
  real-instance translation, while broader integration remains open.

These points narrow the contract work; they do not finalize the entire attribute
grammar or complete independent checking and transplantation. See the
[SW task clarifications](architecture.md#software-task-clarifications) and
[task inputs and spawning](architecture.md#task-inputs-and-spawning), plus
[generated checking contexts](architecture.md#generated-checking-contexts).

Current result: the typed SW fixture compiles without a system dependency, and
focused invalid resource, borrow, spawn, configuration, and duration uses fail
for the intended compiler reasons at asserted authored-source locations. The
focused Rust Analyzer proof records the two resource/type diagnostics it emits
at authored task-body lines. A renderer-owned path also compiles repeated
instances and colliding support from sibling source modules as complete RTIC
handlers with resource renaming. Remaining work includes broader task-shape
lifetimes and cross-boundary/import/macro handling. Direct
per-instance task configuration, spawn alias, initial task-side SysTick
translation, and mapped expressions in the bounded native logging forms are
implemented. General macro handling remains outside scope.

Acceptance: a task compiles in its own workspace with no system dependency;
invalid resource operations and spawn arguments fail there. Its whole body can
then be transplanted using compatible concrete bindings.

#### Remaining SW Implementation and Contract Details

The simple LED/period shape does not yet settle all SW requirements:

- **Resource implementation:** the generic checking expansion proves the initial
  resource-keyed bounds and inline concrete types. Repeated-instance and
  sibling-module original-source emission now have ARM proofs; relative paths,
  IDE behavior, and full firmware integration remain unproven.
- **Input/spawn implementation:** independent typed aliases and RTIC-compatible
  results are proved. Composition-driven direct alias-to-instance calls now
  pass an ARM check for zero, one, and multiple inputs; macro-contained calls
  remain unsupported. The generated init checker derives selected-instance
  spawn signatures from composition, and the bounded init transplant retains
  those direct calls in the real RTIC app.
- **SysTick implementation:** the agreed 1 kHz/32-bit task delay, real backend,
  and profile binding have an ARM proof. Independent init checking validates
  `start(SYST, u32)` against real HAL setup, and the combined transplant rewrites
  that authored startup call; additional methods/profiles are deferred.
- **Module extraction:** imports, aliases, helpers, constants, submodules, and
  selecting tasks independently without changing their source meaning.
- **Checking guarantees:** enforce declared bounds and RTIC-compatible borrowing
  and locking rules; preserve method resolution after concrete substitution;
  keep dependency versions/features compatible and run the final Rust/RTIC
  check. The initial expansion has compiler proofs, while concrete substitution
  and whole-application integration still need them.

These are remaining review and implementation topics, not a demand for
exhaustive coverage. Resource/input/spawn authoring decisions and the minimal
SysTick profile are recorded above. Item 1 retains implementation and remaining
contract-detail work; its generic checking mechanism and profile choice are
resolved. The initial walkthrough is complete; resolve the remaining details
as needed by the bounded implementation proofs.
HW task design and host simulation remain deferred. Resolve only what the
first concrete uses need.

### 2. Independent System Init Contract

Finding: real HAL peripheral types are available to the system, but an
independent init module also needs the interfaces behind references such as
`blink::spawn(...)`, task configuration, and monotonic startup. Today these
are supplied by the containing app declaration in
[current system source](prototype.md#system-source).

Agreed direction:

- Hardware setup uses native HAL types, traits, and methods, plus real PAC and
  Cortex-M peripherals. No FerroForge hardware-trait replacements or HAL
  wrapper API are introduced for initialization.
- Generate check-only app interfaces from composition and the selected task
  contracts: a mock `init::Context` carrying real peripheral types, typed spawn
  entry points for selected instances, and the selected monotonic startup API.
  Include configuration interfaces if init needs them; their spelling is not
  selected by this decision.
- The host reads source/metadata without linking target implementations.
  Generate the interfaces before checking init for the actual embedded target.
  Developers do not duplicate selected task signatures by hand, and checking
  does not execute the hardware setup.
- Transplant the complete init body with native HAL calls intact. Generated
  firmware uses real RTIC/monotonic equivalents, not the check-only interfaces.

Current implementation: `#[ferroforge::init]` is a signature-validating source
marker. Cargo-aware discovery requires exactly one initial `fn init` returning
`(Shared, Local)`. The renderer writes composition-generated `init::Context`,
selected-instance spawn functions, `Mono::start(SYST, u32)`, and a Cargo
configuration for checking the original no-std package. The check-only
`ferroforge` dependency remains in that authored check manifest and is reserved
for exclusion from the later firmware manifest. The macro parses the generated
interface during expansion, avoiding a nested `include!` that Rust Analyzer
cannot load. The STM32F401 fixture checks on ARM. Regeneration removes
deselected instance interfaces, and compiler negative cases reject the resulting
stale call, a wrong spawn argument, and a wrong startup clock type. An opt-in
Rust Analyzer 1.98.0 regression reports the focused spawn/startup errors on the
authored init lines.

Current result: the bounded standalone renderer now transplants the init's
self-contained root support, `Shared`/`Local`, and complete native body into the
real-RTIC path. It replaces the checking-only marker, rewrites direct
`Mono::start`, retains selected-instance spawn calls, and rejects child modules
or surviving `ferroforge` references. Additional API remains deferred until a
concrete use requires configuration references, `cx.cs`, init-local storage, or
child support modules. The initial checker/transplant requires a qualified
crate-root marker, STM32F401 PAC/Core types, and the 1 kHz/u32 SysTick profile.

Acceptance: the system's init body and resource construction compile for its
target independently of `gen_app`; its task/configuration references stay
consistent with the selected composition.

### 3. Supported Mock API and Real RTIC Translation

Finding: the earlier examples did not express the intended context-based task
shape and incorrectly showed `#[rtic::task]` as the generated attribute. The
actual renderer already emits `#[task(...)]` inside `#[rtic::app(...)]`.
It validates/stores spawn bindings but does not apply them in `render_task`
in [renderer implementation](prototype.md#renderer).

For the legacy app-backed scope, the existing clock mock still needs to match SysTick:
its `start(clock_hz)` omits the `SYST` argument, and its delay accepts only
`MillisDurationU64`. Matching duration/instant types and the selected counter
width/tick rate is required. The separate standalone task mock now checks the
agreed `u32`/1 kHz delay, but startup remains a recorded gap.
See [current monotonic support](prototype.md#target-and-monotonic-support).

The standalone input/spawn contract now implements RTIC 2's failure result
shape. The legacy app-backed mock still uses `SpawnError::QueueFull`, and the
legacy renderer still does not translate standalone aliases. The standalone
transplant rewrites direct alias calls to real instance spawns with matching
zero/one/multiple-input `Result` shapes.

Remaining decision/work: define the supported RTIC version/API boundary,
including resource lifetimes and hardware/software task shapes, and extend the
initial direct-call implementation where concrete uses require it.
Composition owns final priorities and interrupt bindings. Explicitly specify
how mock spawn aliases, configuration paths, resource names, and instance
context paths are translated in both task and init bodies.

The standalone transplant now supports the preferred task-local
`CONFIG.FIELD` form for direct expressions, using the current instance's
declared values and types. Its syntax-aware field visitor emits references such
as `__ferroforge_config::status_blink::PERIOD_MS` into one generated namespace
grouped by task instance, and diagnoses bare, unknown, or shadowed `CONFIG`
uses. Supported native logging arguments receive this rewrite; configuration
inside other macro token streams is still rejected. The legacy app-backed
renderer continues to handle qualified `task::Config::FIELD` paths separately.

Logging requirement agreed during this discussion: ordinary RTT/defmt syntax
is initial supported scope, including composition-dependent expressions inside
logging arguments. Do not require FerroForge logging wrappers or moving such
arguments into handwritten temporary variables. Preserve format strings/hints,
native macro calls, argument evaluation, and module-level imports/aliases.
Use actual logging crates and their formatting rules, with system-selected
backend setup. See [native logging](architecture.md#native-rtt-and-defmt-logging).

The standalone transplant now handles ordinary comma-separated expression
arguments for the agreed logging families. Its ARM regression covers
configuration/resource renaming, qualified calls, module-imported aliases, and
unchanged lookalike string literals for both defmt and rtt-target. The legacy
renderer still performs string replacement inside macro token text. General
custom-macro support, RTT terminal selectors, and more elaborate re-export
chains remain separate boundaries.

Acceptance: a typed spawn alias bound to a differently named task compiles in
the final RTIC app, including code handling the failure result. Ordinary
RTT/defmt calls with mapped configuration and resource arguments must also
compile using the selected real backend, with format strings preserved. The mock must
not intentionally use incorrect types or lifetimes just to make an example
pass. Full RTIC equivalence is not required in the first version; document
remaining early-check gaps and let the final Rust/RTIC check reject them.

### 4. Supporting Source and Reference Scope

Finding: the [source loader](prototype.md#source-loader),
`load_task_implementations`, currently copies annotated functions and omits
their surrounding helpers and imports. Relocation can also change the meaning
of `crate::`, `super::`, feature conditions, and references inside macros.

Agreed supporting-source boundary:

- A logical Rust module groups related tasks and their imports/supporting code.
  Ordinary file-backed modules (`mod indicators;` plus `indicators.rs`), inline
  modules, and a crate root used as one group are valid authoring forms. No
  extra FerroForge module wrapper or grouping annotation is required.
- Select tasks individually; selecting one task does not instantiate its
  siblings or merge their RTIC resources.
- Carry the module's ordinary supporting imports, helper functions, types,
  implementations, and constants together, without a handwritten helper list.
  Unused supporting items and their dependencies may be retained and must
  still compile; precise dependency/helper pruning is deferred.
- Keep different source modules' support namespaces separate. Tasks and
  instances from the same source module share supporting type identity;
  instantiating a task twice must not automatically duplicate module-level
  state. This is not a blanket guarantee for arbitrary global-state patterns.
- Keep complete task bodies in real RTIC handlers. Generated supporting
  modules/access wiring may preserve their source environment, but must not
  turn reusable tasks into runtime library callbacks or require authors to
  expose ordinary private helpers just for transplantation.

Initial scope is self-contained task modules with ordinary external-crate
imports. Module discovery is implemented. The renderer now emits deterministic
per-source namespaces and handler-local imports, preserving one support identity
for repeated instances and isolating colliding names across sibling modules.
Generated support visibility is widened only inside a private parent namespace;
authors do not change source visibility. Direct resource field renaming is also
implemented. Task `self::`, contained task `crate::`, and descendant-support
`super::` paths now have syntax-aware handling. Escaping paths are diagnosed.
Remaining work is overlapping ancestor/descendant selection, relative imports,
macro token streams, and general cross-boundary references.
Define explicit support/diagnostics for more elaborate
cross-module references, nested source discovery, feature conditions, and
custom macros rather than inferring an arbitrary compiler dependency graph.
Init support must likewise preserve its source environment; its exact
discovery/access integration still needs proof.
Rewrites must preserve literals and unrelated identifiers; the legacy
app-backed macro configuration rewrite uses string replacement and needs a
safer defined scope.

Acceptance: a task and an init function using declared helpers/imports retain
their meaning after transplantation; unrelated names and strings stay intact.
Related tasks can share module-level imports, a task can be selected without
its siblings, and imports from different source modules do not collide or
silently change which methods/types a task uses.
File-backed discovery, private-helper access, shared supporting identity,
sibling-module name isolation, and direct per-instance configuration now have
renderer-owned ARM proofs. Contained relative-path semantics are also
implemented with escape diagnostics. The initial task-side SysTick binding and
bounded native logging forms now have ARM proofs as well. Focused authored-source
rustc and Rust Analyzer evidence closes the bounded Phase 2 gate.
Composition-generated independent init checking now has a native-HAL ARM proof.
Source-preserving Rust Analyzer wiring for that checker and the bounded
standalone init transplant are now implemented. Relative imports, mapped calls
and paths inside other macro tokens, overlapping module selection, manifest
emission, and pipeline integration remain explicit gaps.

### 5. Validation Guarantees and Dependency Consistency

Finding: each independent check proves a body against its own contract. It does
not prove that every resource mapping or spawn binding in a concrete
composition is valid. Comparing Rust type spellings in the host renderer is
insufficient. Likewise, a check against one HAL version does not validate source
against a different final version or feature set.

Agreed scope: the generated firmware check may catch integration mistakes that
the mock check misses. Add straightforward early checks as useful cases arise;
a comprehensive earlier target-check harness is not an initial prerequisite.
Agreed dependency requirements: use task/init crates' normal Cargo manifests
as the source of requirements, without a second per-task dependency list or
duplicated Rust version registry. Include dependencies needed by retained
supporting code, not only selected task bodies. Initially carry each
participating source crate's applicable normal dependencies conservatively,
excluding explicitly identified check-only dependencies; unused helpers and
even unselected modules may therefore retain dependencies. Precise pruning is
deferred. The host reads metadata/source, not target implementation libraries.
See [the dependency decision](dependencies.md#agreed-manifest-based-requirements).

Agreed check-only identification: list dependency names in
`[package.metadata.ferroforge]` using
`check-only-dependencies = ["ferroforge"]`. They remain available to the source
check but are excluded from generated manifest entries. Do not infer this role
from crate names or procedural-macro status. Remove or translate corresponding
checking-only imports, attributes, and supported references as part of source
transplantation; manifest exclusion alone is insufficient. See
[the check-only setting](dependencies.md#agreed-check-only-dependency-setting).

Agreed initial merging: the system supplies HAL, RTIC, monotonic, logging-backend,
and panic dependency choices without silently overriding task/init requirements.
For the same package, matching sources, version requirement strings, and
effective default-feature settings permit combining requested features.
Differences in those sources, requirements, or settings produce diagnostics
identifying the contributing manifests/system selections and require alignment.
Even potentially compatible but differently written version requirements are
initially rejected. Smarter compatibility handling is deferred. See
[the merging policy](dependencies.md#agreed-initial-merging-and-conflict-policy).

Implement collection, diagnostics, and supported source/feature handling, then
verify the generated graph. Matching requirements do not prove identical
resolved versions across workspaces or valid feature combinations.
Independent source crates still need their own Cargo dependencies; central Rust
metadata cannot supply missing Cargo dependencies. A broader early-check harness
may be considered later. The current registry implementation remains unchanged.

Acceptance: incompatible concrete bindings fail in a Rust check, and generated
dependency choices cannot silently be treated as covered by unrelated source
checks. The real RTIC check/build remains mandatory.

### 6. Implementation Order and Acceptance Criteria

Finding: the original phases started with components already present in the
prototype and delayed renderer work until after generation. The next proof
must exercise separate task and init workspaces through final transplantation.

Agreed sequence: use the [development phases](implementation-plan.md#development-order)
to prove one complete path before expanding coverage:

1. Independently check a file-backed SW module with an LED task and a small
   reporting task, exercising local/shared resources, configuration, SysTick
   delay, spawning, and native logging.
2. Independently check native HAL system init against composition-generated
   interfaces.
3. Transplant complete bodies and supporting code, apply bindings, generate the
   manifest, and pass real RTIC target checks and firmware builds.
4. Compose unchanged reusable task source into a second system to prove reuse.

Add focused failure tests along the way for resource operations, spawn arguments,
dependency conflicts, and source-preservation mistakes. Prove private-helper
access and shared supporting type identity early because they are unresolved
implementation risks, not just final polish.

Acceptance is independently checked source producing buildable real RTIC
firmware followed by demonstrated reuse, not exhaustive mock coverage. Keep
host simulation separate from compile-only mock support. This agreement does
not claim that the separated-source pipeline is already implemented.

The documentation cleanup makes the prerequisites and exit gates explicit:
shared parsing and minimal discovery precede init-interface generation, and
the private-helper/type-identity proof is part of the early SW phase. Each
phase records deliverables, verification, and pass/fail conditions, including
configuration/profile rejection and orchestration failure propagation. These
clarifications do not expand the agreed initial feature scope or mark any
implementation phase complete.

Review baseline: on 2026-09-05, all 23 macro tests and 7 renderer/loader tests
passed, the embedded library checked for `thumbv7em-none-eabihf`, and the
existing generated firmware completed a release build. These results validate
the current example, not the proposed separated task/init design.
