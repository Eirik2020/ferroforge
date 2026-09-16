# Implementation Plan History - Archived 2026-09-16

Archived from `docs/src/implementation-plan.md` on 2026-09-16. Contains the
"Review Status" narration, both Phase 1 and Phase 2 Progress Records, and the
Phase 1-6 definitions with their deliverables, verification, and exit gates.

All six phases were met for their bounded backend scope. The active sequence is
now `docs/src/composition-repair-plan.md`. Binding material - the acceptance
bar, the incremental coverage policy, and the verification and completion rules
- was kept in the active chapter. What remains here is status, evidence, and
the superseded phase sequence, not active design authority.

---

## Review Status and Progress Records

## Review Status

The [six review topics](review.md) track agreements and remaining work. Initial
SW checking, system-init interface generation, native logging, and the logical
module support boundary are recorded, along with the initial dependency policy.
Item 6's initial implementation sequence and acceptance bar are now agreed.
Phase 1 foundations are implemented, and the bounded Phase 2 and Phase 3 exit
gates are met, as recorded below. Phase 4 now includes the first standalone
init-to-firmware transplant plus manifest-derived project emission, complete
initial STM32F401RE target files, and a real-RTIC ARM check/release link.
The first explicit-Rust frontend, grouped configuration namespace, and
centralized Nucleo system are also in place. The bounded Phase 4 exit gate is
met. The bounded full pipeline and second-system reuse proof are now recorded
below. Required per-system declaration/unified-target work is tracked in the
repair plan; general reference coverage and additional targets remain limited.

### Progress Record - Phase 1 Foundation

Implemented:

- `ferroforge-contracts`: shared parsing/structural validation of task arguments,
  including the new typed resource/configuration and inline spawn declarations.
  The source loader uses its legacy adapter; the task macro retains the working
  app-backed branch for names-only declarations and selects standalone expansion
  for typed declarations.
- `ferroforge-renderer/src/source.rs`: explicit-root discovery of ordinary file,
  inline, and `mod.rs` modules; source/function/support retention; definition and
  instance identity; individual selection and duplicate/missing-name checks.
- `discover_task_package`: Cargo-backed resolution from an exact manifest to its
  package identity and library target/root, followed by source discovery without
  compiling or linking the task package. Virtual manifests, packages without a
  library, and library roots outside the package are rejected; declared custom
  library paths are honored.
- A real package-layout fixture at
  `ferroforge-renderer/tests/fixtures/sw/Cargo.toml`, with its file-backed task
  module under `src/`, plus parser and discovery regression tests. The manifest
  records the initial normal dependencies and explicit FerroForge check-only
  dependency. Its typed source is now independently compiled by the Phase 2
  regression harness.
- `ferroforge-renderer/src/composition.rs`: an owned host-side composition model
  and validator for definition-to-instance selection, complete local/shared
  resource mappings, configuration names/types/value syntax, complete spawn
  alias bindings, obvious spawn argument-count mismatches, and the agreed
  SysTick 1 kHz/`u32` profile. It permits shared-resource reuse, rejects local
  resource reuse/category conflicts, and leaves concrete Rust compatibility to
  the generated compiler check. This is not a settled author-facing macro
  grammar and is not connected to the legacy composer yet.
- `tasks/blinky`: the first non-fixture reusable task package, independently
  checked without a system dependency and selected unchanged by the Nucleo
  system.
- `systems/nucleo-f401re`: the agreed centralized `init/`, `app_composition/`,
  and `gen_app/` ownership layout. Its host-only composition crate is the first
  frontend for the owned standalone model and keeps init/target/runtime choices
  with the system rather than the reusable task package.
- Cargo metadata plus the package's authored TOML now supplies conservative
  normal dependency requirements for each discovered task package.
  `ferroforge-renderer/src/dependencies.rs` implements exact source/version/
  effective-default-feature matching, feature union, and contributor-aware
  conflict diagnostics. The collector parses and excludes the agreed explicit
  `check-only-dependencies`, rejects unknown metadata, and preserves authored
  version strings rather than Cargo's normalized display. Initial runtime scope
  is unconditional, non-optional, unrenamed registry dependencies; target,
  optional, renamed, git, path, and workspace-inherited runtime entries receive
  diagnostics. A check-only path dependency is allowed because it is excluded
  before runtime-source validation. The standalone project writer and first
  system frontend now consume this model; the legacy renderer remains separate.

Current fixture scope: safe async tasks with a named `task_name::Context`
parameter, named incoming arguments, and `()`/`!` returns; no authored generics,
custom ABI, or destructured parameters. The parser accepts `monotonic = Mono`;
`ferroforge::mock::systick::Mono::delay` is implemented for the initial
1 kHz/`u32` duration type. The generated init checker now supplies the matching
`start(SYST, u32)` interface. The initial
target proof remains STM32F401 / `thumbv7em-none-eabihf`, RTIC
2.3.1 and rtic-monotonics 2.2.1, matching the existing backend versions, with
the agreed 1 kHz/32-bit SysTick profile now implemented in the early task
transplant. Existing legacy firmware still uses TIM5. The first SW package
fixture path is now concrete. The first standalone init crate lives at
`ferroforge-renderer/tests/fixtures/init`; its composition-generated interface
and Cargo configuration are written to a caller-selected output directory, then
the original package is checked against them. The same discovered package can
now supply root support, `Shared`, `Local`, and its complete native body to the
standalone RTIC renderer.

Remaining after the completed Phase 1 foundation and bounded Phase 2/3 gates:
add API spellings and additional target profiles only as concrete systems
require them, and complete orchestration. Initial standalone
dependency/manifest/STM32F401RE
target emission and the checking-only init marker/root-import cleanup are
implemented; broader
checking-only source translation remains later Phase 4 work. Source selection
and parser validation alone are not a concrete type check; the separate fixture
`cargo check` and generated real-RTIC target check provide that evidence.
See [implemented discovery limits](prototype.md#standalone-discovery-foundation).

Verification for this foundation:

| Working directory | Command | Result |
| --- | --- | --- |
| Repository root | `cargo test --workspace --all-targets --locked --offline` | 83 top-level tests pass and two Rust Analyzer tests are opt-in/ignored: 10 contract, 23 macro, 7 existing renderer/loader, 10 discovery, 6 standalone composition, 6 dependency collection/merging, 4 standalone checking tests (one ignored), 8 standalone transplant tests, 3 standalone project tests, 4 independent-init tests (one ignored), 3 first-system frontend/orchestration tests, and 1 second-system reuse test; nested checks add one positive SW fixture package, eight expected compiler failures with authored-source locations, the fixed ARM layout, renderer-emitted ARM task source including the combined native-init case, one manifest-driven generated ARM project that checks and release-links with its emitted target files, one native-HAL init fixture check, three expected init-check failures, and the real source/composition/init/render/check/link pipeline failure matrix |
| Repository root | `cargo test -p ferroforge-renderer --test standalone_project --locked --offline` | Pass; emits, ARM-checks, and release-links a manifest-derived standalone project with `memory.x`, Cargo runner/linker settings, and probe configuration; retains conservative task/init dependencies, excludes `ferroforge`, and diagnoses invalid target data or a system/source version conflict before writing output |
| Repository root | `cargo run -p ferroforge-nucleo-f401re-composer --locked --offline` | Pass; checks `tasks/blinky`, validates composition, generates and checks the centralized native init interface, emits/resolves/checks the real-RTIC firmware, and completes its release build |
| Repository root | `cargo run -p ferroforge-nucleo-f401re-fast-blink-composer --locked --offline` | Pass; reuses `tasks/blinky` unchanged with a separate init and alternate instance/resource/configuration bindings, then checks and release-links the second generated system |
| `tasks/blinky` | `cargo check --lib --target thumbv7em-none-eabihf --locked --offline` | Pass; independently checks the reusable portable task package |
| `systems/nucleo-f401re/.ferroforge/init-check` | `cargo check --lib --manifest-path ../../init/Cargo.toml --locked --offline` | Pass; checks the centralized native HAL init against the generated composition interface |
| `systems/nucleo-f401re/gen_app` | `cargo build --release --locked --offline` | Pass; release-links the first standalone system using its emitted target package |
| Repository root | `cargo test -p ferroforge-renderer --test standalone_check rust_analyzer_reports_errors_on_authored_task_body_lines --locked --offline -- --ignored --exact` | Pass with Rust Analyzer 1.98.0; E0599 and E0308 point to the intentionally invalid authored task-body lines |
| Repository root | `cargo test -p ferroforge-renderer --test init_check --locked --offline` | Pass; composition-generated init interfaces check the complete native STM32F401 init on ARM and reject stale/mistyped calls |
| Repository root | `cargo test -p ferroforge-renderer --test init_check rust_analyzer_reports_init_errors_on_authored_lines --locked --offline -- --ignored --exact` | Pass with Rust Analyzer 1.98.0; E0107 and E0308 point to the intentionally invalid authored init lines |
| Repository root | `cargo clippy --workspace --all-targets --locked --offline -- -D warnings` | Pass |
| `embedded` | `cargo check --lib --target thumbv7em-none-eabihf --locked --offline` | Pass; existing app-backed source |
| `generated/nucleo-f401re` | `cargo build --release --target thumbv7em-none-eabihf --locked --offline` | Pass; existing generated firmware |

The standalone fixture now has an intentional lockfile for its independent
`--locked` check. New local package dependencies required intentional Cargo
lockfile updates in the host and embedded check workspaces; generated firmware
dependencies are unchanged. These checks preserve the prototype and prove the
bounded independent SW compile surface, focused authored-source diagnostics,
and the early concrete RTIC transplant. They do not prove editor behavior for
every compiler error or the end-to-end generated pipeline.

### Progress Record - Phase 2 Independent SW Checking

Implemented:

- Typed `#[ferroforge::task]` declarations branch to a generic checking
  expansion without a surrounding system app. Resource-keyed bounds become
  real Rust generic bounds, concrete local resources become mutable references,
  and shared resources use a typed lock proxy that prevents borrow escape.
- Inline spawn signatures generate methods with checked argument types and
  RTIC-compatible `Result` failure values for zero, one, and multiple inputs.
- The task-local immutable `CONFIG` view checks direct field reads for the
  initial unqualified `u32` field type and diagnoses a task-body `CONFIG`
  binding conflict. Broader configuration types and constant-expression use
  remain outside this slice.
- `ferroforge::mock::systick::Mono::delay` accepts the agreed
  `fugit::Duration<u32, 1, 1_000>` and remains compile-only.
- `ferroforge-renderer/tests/standalone_check.rs` checks the real file-backed
  SW package and asserts intended compiler failures for undeclared resource
  operations, wrong concrete resource types, escaping shared borrows, missing
  spawn inputs, wrong/unknown configuration uses, `CONFIG` conflicts, and a
  mismatched duration representation. Every focused failure must identify the
  authored task-body line. Its opt-in Rust Analyzer check uses the intentionally
  invalid `fixtures/ra-diagnostics` package and confirms the invalid method and
  value diagnostics resolve to the authored lines. Rust Analyzer 1.98.0 does
  not emit the shared-lock lifetime failure; the rustc regression remains the
  required proof for that case.
- `ferroforge-renderer/tests/fixtures/rtic-layout` checks a minimal real RTIC
  application on `thumbv7em-none-eabihf`. Nesting the app beneath one logical
  source module keeps ordinary helper functions private and emits each support
  type only once for both handlers. RTIC's generated public resource interface
  requires a support type used as a resource to be public in the generated
  copy; this is generator-owned visibility wiring, not an author requirement.
- `ferroforge_renderer::transplant::render_rtic_app` now owns the early layout.
  It emits each selected logical module's support once in a deterministic
  namespace, recursively retains child support modules, omits sibling task
  handlers, renames repeated selections to their instance names, rewrites
  direct local/shared context fields, and emits complete handlers in one RTIC
  app. Per-handler imports isolate colliding `Sample`, `STEP`, and `update`
  names from two source modules in the ARM regression. It rejects context
  shadowing rather than risk rewriting a different binding. The original
  single-module entry point remains as a stricter compatibility gate.
- Task-body `self::` references and `crate::` references rooted inside the
  selected logical module are rewritten to deterministic generated paths.
  Descendant support keeps `super::` paths that remain inside the selected
  subtree. Escaping `super::`/`crate::` paths and relative `use` declarations
  receive explicit diagnostics. The ARM fixture exercises task `self::`,
  contained task `crate::`, and descendant-support `super::` references.
- Direct task-body `CONFIG.FIELD` expressions resolve to named typed constants
  generated per composed instance before the RTIC resource structs. Repeated
  instances with different values pass the ARM check. The rewrite is
  syntax-aware and diagnoses bare or shadowed `CONFIG`; supported native logging
  arguments use the same rewrite and other macro token streams are rejected.
- Direct `cx.spawn.<alias>(...)` calls resolve to the selected RTIC instance's
  `spawn` function without changing arguments or surrounding result handling.
  Differently named targets and RTIC's zero-, one-, and multiple-input failure
  shapes pass the ARM fixture. Bare spawn handles and macro-contained calls are
  diagnosed as outside the initial supported syntax.
- The initial SysTick profile emits `systick_monotonic!` at 1 kHz with the
  default `u32` representation. Direct declared `Mono::delay(...)` calls resolve
  to that generated backend. The separate init checker validates the matching
  startup call against real HAL clock data, and the bounded Phase 4 transplant
  rewrites that authored call to start the backend with real `SYST` and a `u32`
  core frequency. Bare/unsupported/macro-contained monotonic uses receive
  focused diagnostics.
- Qualified and module-imported/aliased defmt level/`println!` and rtt-target
  `rprint!`/`rprintln!` calls parse ordinary comma-separated Rust expression
  arguments and apply the existing mapped-expression visitors in place. The ARM
  fixture proves configuration/shared-resource rewrites while preserving macro
  paths, delimiters, format hints, and lookalike literal text. RTT terminal
  selectors, elaborate re-export chains, and general custom macros remain out
  of scope.

The bounded Phase 2 exit gate is met. The early transplant path deliberately
rejects selecting overlapping ancestor/descendant source modules. Relative
imports, relative paths and mapped expressions inside macro token streams, and
references outside the selected support subtree remain unsupported outside the
bounded native logging forms; those broader cases are not Phase 2 prerequisites.


---

## Phase Definitions (Phases 1-6, all met)

### Phase 1 - Shared Contracts and Minimal Source Discovery

-   Record the first fixture's concrete workspace paths, target, supported
    RTIC version, and remaining task/init/clock API spellings, retaining the
    agreed resource and input/spawn authoring forms. Use the existing supported
    STM32F401 target as the starting candidate; no new board support is required
    to begin this proof.
-   Start with SW resource bounds from `embedded-hal`, typed `u32` period
    configuration, and the agreed 1 kHz/32-bit SysTick delay/start profile.
    Defer HW task design.
-   Define the minimum RTIC APIs needed by the first example and concrete
    handler translation; start monotonic support with its used delay API.
-   Define the remaining reference/visibility rules within the agreed logical
    module support boundary, and specify supported manifest cases within the
    agreed conservative dependency policy.
-   Implement the small shared declaration model/parser used by macros and the
    renderer. Retain resource categories/types/bounds, configuration types,
    incoming inputs, outgoing alias signatures, and the clock profile.
-   Resolve source packages/modules and distinguish definitions from instances.
    Support ordinary file-backed and inline modules, including a crate-root
    group, retaining imports and ordinary support items without a helper list.
-   Validate declared names, duplicate/missing bindings, resource categories,
    alias/target completeness, and obvious argument-count mismatches. Limit
    reference/manifest handling to the selected initial scope with explicit
    diagnostics for unsupported cases; do not infer arbitrary Rust semantics.

Deliverables: the shared contract model/parser, initial discovery fixtures, and
a recorded first-example scope. This foundation precedes init-interface
generation rather than being deferred until after independent init checking.

Verification: host tests exercise the supported module forms, definition versus
instance identity, retained support/imports, and positive/negative declaration
cases. Both macro and host consumers use the same contract parser.

Exit gate: the host can discover and validate the first example's declarations
without linking target task/init implementations; unsupported or incomplete
declarations fail with useful diagnostics. Concrete Rust type compatibility
remains a compiler responsibility.

### Phase 2 - Independent SW Checking and Early Transplant Proof

-   Create one reusable SW task crate with no system dependency, checked
    against its declared real trait bounds and generated mock context.
-   Implement the selected generic function/context expansion, preserving
    body source spans and import scope. Exercise returning and divergent task
    lifetimes and add regression cases for invalid resource operations/types
    and shared-lock reference escape. Verify Rust Analyzer diagnostics.
-   Generate outgoing mock methods from inline alias signatures, derive
    RTIC-compatible failure results, and check invalid calls independently of
    a destination task or system composition.
-   Implement the immutable typed task-local `CONFIG` checking view and the
    compile-only 1 kHz/32-bit SysTick delay interface. Check configuration without
    needing system values; keep constant-expression uses outside the initial
    scope. Do not introduce host sleeping or scheduler simulation.
-   Build a small real-RTIC target fixture for the proposed namespace/access
    layout alongside the SW checker. Prove private-helper access and supporting
    type identity shared by tasks/instances from one module, without author-side
    visibility changes, duplicated support state, or runtime task wrappers.
    This is an early layout proof, not the full composition pipeline.

Deliverables: the reusable file-backed SW task module, generated checking
contexts, compile-pass/fail fixtures, and a minimal real-RTIC layout proof.

Verification: independently check the SW crate; run regression cases for an
undeclared resource method, wrong local/shared value type, escaping lock borrow,
invalid spawn arguments, unknown/wrongly typed configuration reads, and a
conflicting `CONFIG` binding. Check delay argument types and compile the layout
fixture for the first ARM target. Record Rust Analyzer diagnostics against the
authored source.

Exit gate: valid SW bodies check without a system dependency, invalid supported
uses fail for the intended reasons, and the early layout proof preserves private
access/type identity while retaining complete real RTIC task bodies. If the
layout cannot preserve those properties, resolve it before broad renderer work.

Status: met for the bounded initial scope. The always-run rustc cases assert
authored-source locations, and the opt-in Rust Analyzer fixture records the
authored locations for the invalid method and concrete value diagnostics that
Rust Analyzer 1.98.0 emits. Rust Analyzer does not replace the compiler checks.

### Phase 3 - Composition-Generated Init Checking

-   Use Phase 1's contracts, instance resolution, and composition validation
    to generate the initial check-only app interfaces.
-   Create one system init crate with real HAL resource construction.
-   Generate its check-only context, selected-instance spawn entry points, and
    monotonic startup interface from composition/contracts before the target
    init check. Preserve native HAL calls and verify interface regeneration
    when the selected composition changes.
-   Implement the real `SYST`/`u32` startup checking signature and reject
    unsupported or mismatched composition clock profiles before init checking.
-   Provide the chosen packaging and regeneration/invalidation mechanism for
    checking interfaces. Include configuration access only if the first init
    uses it, after specifying that supported syntax.

Deliverables: one native-HAL init crate and a host-generated checking interface
derived from the selected composition, with no handwritten duplicate task
signatures and no dependency on the final generated firmware.

Verification: generate interfaces, then check init for the selected ARM target.
Test invalid spawn/startup arguments, unsupported clock profiles, and a changed
composition that updates instance names/signatures. Check that stale interfaces
cannot make that changed composition appear valid. Record focused IDE feedback.

Exit gate: the valid init body checks independently of `gen_app`, uses real HAL
and peripheral types, and stays consistent with regenerated composition
interfaces. Negative fixtures fail without executing hardware initialization.

#### Progress Record - Phase 3 Init Checking

Implemented:

- `InitContract` and the re-exported `#[ferroforge::init]` marker validate one
  safe synchronous `fn init(init::Context) -> (Shared, Local)` without generics
  or an ABI. Cargo-aware source discovery retains exactly one init declaration.
- `ferroforge_renderer::init_check` writes the composition-generated
  `init::Context`, selected-instance spawn functions, `Mono::start(SYST, u32)`,
  and a Cargo configuration that makes them available while checking the
  original no-std init package. Its authored manifest therefore retains both
  normal dependencies and the explicitly check-only `ferroforge` dependency.
- The `#[ferroforge::init]` macro parses the generated interface during macro
  expansion and emits ordinary interface tokens beside the original function.
  This avoids a nested `include!` that Rust Analyzer cannot load while retaining
  authored spans for the complete init body. Generated Windows paths omit the
  verbatim `\\?\` prefix so Cargo and Rust Analyzer see the same file identity.
- The initial target/API scope is STM32F401, a qualified crate-root marker,
  self-contained root support, real `cortex_m::Peripherals` and HAL PAC
  peripherals, `(Shared, Local)`, and the validated 1 kHz/u32 SysTick profile.
- The native fixture freezes real HAL clocks, configures GPIO PA5, constructs
  resources, starts the check-only SysTick interface, and spawns selected task
  instances. Its original package checks on `thumbv7em-none-eabihf` without
  executing or linking the init/task bodies into the host.
- Regeneration tests prove deselecting an instance removes its spawn module and
  makes the unchanged stale init call fail. Wrong spawn arguments and a `u64`
  startup frequency also fail for their intended compiler reasons; a missing
  profile is rejected before rendering.
- An opt-in Rust Analyzer 1.98.0 regression deploys the generated interface and
  Cargo configuration beside the init package, then verifies that wrong spawn
  arity and startup-frequency diagnostics point to their authored init lines.

Status: met for the bounded initial scope. Init configuration access, `cx.cs`,
init-local storage, child support modules, and other targets remain deferred
until required by a concrete system. The initial standalone init transplant is
now implemented in Phase 4 together with manifest and complete initial
STM32F401RE target emission. The explicit-Rust frontend, first-system
integration, grouped configuration namespace, bounded Phase 5 orchestration,
and Phase 6 same-board reuse proof are implemented.

### Phase 4 - Implement Composition-Driven Transplantation

Status: met for the bounded initial scope. `render_rtic_app_with_init` takes the
discovered,
independently checked `InitPackage` plus target-owned RTIC shell data. It retains
crate-root support and authored `Shared`/`Local` definitions, replaces the
checking-only init marker, rewrites direct `Mono::start(SYST, u32)` to the real
generated SysTick monotonic, and emits the complete native HAL body beside the
selected task instances. A generated STM32F401 program checks with real RTIC on
`thumbv7em-none-eabihf` and contains no `ferroforge::` reference. The initial
scope deliberately diagnoses child init modules and surviving checking-only
references. `render_standalone_project` now merges the task package's, init
package's, and explicit system requirements using the conservative dependency
policy, emits `Cargo.toml` plus the initial STM32F401RE `.cargo/config.toml`,
`memory.x`, and `Embed.toml`, and checks and release-links the resulting project
on ARM. The manifest retains conservative unused source dependencies, excludes
`ferroforge`, and reports contributor-aware conflicts before writing partial
output. Board memory/configuration data is validated before output begins.
The first explicit-Rust frontend now generates the centralized system under
`systems/nucleo-f401re` from the independent `tasks/blinky` and system init
packages, and that actual project release-links. Configuration values are
emitted once under `__ferroforge_config`, grouped by task instance, and direct
task reads use instance-qualified constant paths. Broader source translation
and additional target profiles remain explicitly deferred until a concrete
system requires them; they do not keep the bounded first-system gate open.

-   Transplant complete task and init bodies with their supporting source;
    carry ordinary task-module support as a unit without selecting sibling tasks.
-   Preserve module import aliases and scope when selecting tasks, including
    tasks from different modules with conflicting imported names.
-   Extend the Phase 2 layout proof into generated supporting namespaces/access
    wiring that preserves private-helper access and supported relative paths.
    Share supporting type identity across
    tasks/instances from one source module; do not duplicate module-level state
    merely because a task is instantiated twice.
-   Apply task-instance, resource, configuration, spawn, and clock mappings.
-   Rewrite typed `cx.spawn.<alias>(...)` calls to composed instance spawn
    functions, preserving arguments and surrounding result/error handling.
-   Rewrite supported `CONFIG.FIELD` expressions per instance, preserving
    declared types and diagnosing conflicting bindings in the supported scope.
-   Support ordinary native RTT/defmt logging arguments from the initial
    implementation. Rewrite mapped expressions in place while preserving
    format strings/hints, native macro syntax, and imported macro aliases.
    Replace token-text substitution with syntax-aware handling of supported
    logging forms; do not hoist evaluation outside logging macros.
-   Emit a single named-constant configuration overview grouped by task instance,
    keeping configuration values out of literal substitutions in task bodies.
-   Generate the firmware manifest from normal task/init Cargo dependency
    requirements plus the system's runtime requirements, and generate target
    configuration. Replace duplicate per-task registry requirements with the
    agreed manifest-based input. Initially retain participating source crates'
    applicable normal dependencies conservatively, including unused support
    dependencies. Parse the agreed `package.metadata.ferroforge` setting
    `check-only-dependencies`, exclude listed entries from the firmware manifest,
    and remove/translate supported checking-only imports and attributes. Define
    and implement the agreed conservative merging policy: matching sources,
    version requirement strings, and effective default-feature settings permit
    feature union; differences require diagnostics and explicit alignment.
    Identify contributing manifests/system choices rather than silently
    overriding source requirements.

Deliverables: composition-driven real RTIC source, grouped named configuration
constants, and the generated manifest/target files for the first system.

Verification: host regression tests cover mappings, sibling exclusion, support
identity/import collisions, native logging arguments and unchanged literals,
check-only cleanup, and dependency merging. The first centralized system is
generated by its host frontend; its task and init packages check independently,
and its generated target release-links. Phase 5 subsequently incorporated
these steps into the end-to-end command.

Exit gate: the generated project checks with real RTIC and no FerroForge mock
dependency or task-library runtime wrapper. Selected bindings and constants are
visible in the output; unsupported rewrites and dependency conflicts produce
diagnostics rather than silently altering source meaning or requirements.

### Phase 5 - Orchestrate and Prove the Complete Pipeline

Status: met for the bounded first-system scope. `run_nucleo_f401re_pipeline` is
the recorded host entry point. It invokes child Cargo processes in dependency
order for the reusable task check, generated native-init check, firmware
lockfile resolution, real-RTIC target check, and release build, with
composition/interface and
firmware generation between those stages. Errors identify the failed stage and
return immediately. A fake command executor regression injects failure at every
Cargo boundary and proves later command stages are skipped. The documented real
Nucleo invocation completes successfully. An always-run real-process regression
now covers an invalid reusable task, a composition missing the selected task
definitions, an init call inconsistent with the generated interface, a
checking-only init reference rejected during firmware generation, a concrete
resource type rejected by the generated Rust/RTIC check, and a malformed
test-owned linker script rejected by the release build. It asserts the reported
stage and exact Cargo-stage prefix in every case, so dependent later stages are
not merely assumed to be skipped. These positive and negative proofs meet the
Phase 5 exit gate without claiming hardware execution or general multi-system
orchestration.

-   Add the host orchestration entry point for the required checks and generation.
    The initial command is
    `cargo run -p ferroforge-nucleo-f401re-composer --locked --offline`; later
    generalization beyond this first-system executable remains an implementation
    choice.
-   Run task checking and structural composition/dependency/profile validation,
    generate init checking interfaces, check init for its target, generate the
    firmware, then run its target check and release build. Never link target
    task/init implementations into the host executable.
-   Propagate an unsuccessful stage as an unsuccessful overall command, retaining
    useful diagnostics and skipping dependent later stages. Do not report a
    successful pipeline merely because rendering succeeded or old output exists.
-   Select a task from a file-backed module with imports and private helpers,
    leaving sibling tasks unselected. Check conflicting names across modules
    and shared supporting type identity across multiple tasks/instances from
    one module. Retained unused support must still compile.
-   Verify a selected task retains a dependency used only by an ordinary
    supporting helper, even when the sibling task that calls it is unselected.
    Cover init-only dependencies and ensure identified checking-only support
    does not leak into the generated firmware manifest.
-   Verify explicit check-only dependencies remain available to independent
    source checks while their supported source references are removed/translated
    in firmware. Test unknown metadata entries and unsupported surviving uses;
    do not classify all procedural-macro dependencies as checking-only.
-   Exercise both local and shared resource claims and access in generated RTIC.
-   Use a mapped resource and a typed task-to-task spawn alias bound to a
    differently named instance, including handling the returned inputs on
    spawn failure. Cover zero, one, and multiple input result shapes.
-   Spawn a selected task from the independently checked system init.
-   Exercise ordinary RTT/defmt logging with configuration/resource arguments,
    qualified and imported macro calls, and literal text resembling mapped
    references. Verify native formatting checks and selected backend integration;
    no FerroForge logging wrapper or replacement formatting trait is required.
-   Check the generated app with real RTIC and build the target firmware.
-   Verify feature union for matching dependency entries and diagnostics for
    differing sources, version requirements, and default-feature settings,
    including conflicts with system/backend selections. Confirm that differently
    written but potentially compatible requirements are conservatively rejected.
-   Add failure cases that exercise contract/binding incompatibility.

Deliverables: the recorded runnable orchestration command, the first complete
generated firmware, and focused positive/negative end-to-end regression cases.

Verification: run the command from a documented starting state and confirm all
required checks and the release build pass. Inject representative failures in
source checking, composition/interface generation, init checking, firmware
checking, and linking. Assert unsuccessful overall status and that dependent
later stages are not executed. Earlier phases' expected-failure tests must
continue to pass as tests.

Exit gate: one command carries the separated source workspaces through a real
target firmware build, with correct failure propagation and no claims of
hardware execution. Manual target check/build results corroborate the first
successful pipeline run; document the supported scope and remaining early-check
limits in the workflow and prototype chapters.

### Phase 6 - Prove Reuse Across Systems

Status: met for the bounded same-board scope. The second system is
`systems/nucleo-f401re-fast-blink`. It consumes `tasks/blinky` unchanged,
owns a separate native init, explicit-Rust composition, and generated project,
maps the task definitions to `heartbeat` and `diagnostics`, renames the LED,
counter, and shared-enable resources, and binds a 125 ms period instead of 500
ms. Its host regression verifies those generated differences and byte-for-byte
task-source preservation. Its recorded one-command pipeline independently checks
the task and init, checks the generated real-RTIC target, and completes the
optimized ARM release link. The first system pipeline remains successful.

-   Add a second system using unchanged reusable task source.
-   Verify instance/resource mapping and system-specific init ownership.

Deliverables: a second system composition/init and generated project reusing
the original SW task module without system-specific edits to it.

Verification: run the complete pipeline for both systems with their recorded
targets, check/build both generated projects, and verify that the reusable task
source is unchanged. Exercise different instance/resource/configuration bindings.

Exit gate: both systems pass while sharing unchanged reusable source and owning
their own init/composition. A different MCU family is not required for this
initial reuse proof; it is not evidence of untested HAL/target portability.

Further board/HAL coverage, broader manifest/macro support, and executable host
simulation remain follow-on work, not extra gates for this initial pipeline.
The bounded backend sequence has its recorded proofs, but the intended
per-system `composition!` frontend was not delivered. The active next work is
the [composition and unified-target repair](composition-repair-plan.md), with
its own explicit pending gates.

