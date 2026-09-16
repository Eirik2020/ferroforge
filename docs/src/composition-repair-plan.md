# Firmware Composition, Family Backends, and Unified Target Repair

Status: requirements updated 2026-09-16; repair implementation pending.
This is the active continuation of the [initial implementation plan](implementation-plan.md).
The earlier bounded build proofs remain useful, but they did not deliver the
intended per-system `composition!` authoring interface.

## Implementation Evidence for the Governing Requirements

The [governing requirements](governing-requirements.md) are maintained on one
short page. This chapter contains implementation details, evidence to provide,
progress, and proposals. It must not redefine those requirements.

| Requirement | Evidence to provide |
| --- | --- |
| G1 | Firmware applications consume the same authored task definitions without editing or copying them into firmware-owned source. |
| G2 | Firmware uses required family platform support and independently selects reusable software and hardware task crates; init and tasks share a family HAL helper crate with consistent type identity, and the backend works without optional hardware task crates. |
| G3 | One coherent task/composition interface demonstrates both kinds; the interrupt distinction matches the user's clarified rule. |
| G4 | Supported-chip interrupt coverage and selected-chip validity are demonstrated; invalid bindings are rejected. |
| G5 | Migrated firmware workspaces use the agreed layout and shared host tooling. |
| G6 | Complete authored software-task, hardware-task, composition, and init examples are reviewed for RTIC familiarity before parser/API implementation; any extra declarations serve reuse and composition. |
| G7 | A helper command creates a recognizable project; CLI operations tolerate layout changes that preserve recognition and report missing/unrecognizable required inputs. Selecting a FerroForge library without an explicit marker fails with an error; broader library checks are deferred. |

Keep ordinary Rust task/init bodies, native HAL setup, familiar task/context and
resource access, and a shared composition model central to the API. Internal
renderer needs must not dictate extra author-facing abstractions. Existing
prototype syntax is evidence of current behavior, not a requirement to retain
every spelling in the new interface.

### Interrupt Task Kind - Resolved

On 2026-09-16 the user confirmed that "software" was a spelling error.
Hardware tasks require a direct interrupt binding; software tasks do not.
G4's typed enum requirement remains agreed. Software dispatcher selection is
a composition concern, not a required binding on each software task. Exact
authoring signatures remain subject to the RTIC-like authoring review.

### Implementation Checkpoint

- Current work: keep the governing requirements on one page and link the plan
  and decision record to it. Rust implementation remains pending.
- Existing evidence: reusable software source, standalone task/init checking,
  transplantation, and both bounded Nucleo pipelines already work. They do not
  establish completion of G1-G7 for the new firmware layout and family model.
- Pending: common/family crate organization, the shared task/interrupt enum API,
  firmware workspace migration, unified target consumption, the general CLI and
  project creation/recognition and required library marker handling, and
  checking-interface preparation/refresh in that layout.
- Next action: settle library marker location/syntax, minimum project recognition
  inputs, and new-project helper output; then interrupt enum ownership,
  RTIC-like authoring examples, and checking/target details.
- On continuation: reread `governing-requirements.md`, the current decision in
  `review.md`, and this checkpoint; verify implementation status against source
  and checks before editing. Update progress and evidence without weakening G1-G7.

## Confirmed Requirements

The following earlier requirements remain applicable where compatible with G1-G7:

- Use **firmware** for what was previously called a system. Each firmware owns
  an app composition using `composition!` and handwritten hardware init.
- Each firmware has its own Cargo workspace so its authored code can receive
  Rust Analyzer type checking. The declaration-driven authoring layout below
  is now agreed; the precise IDE preparation/refresh mechanism remains open.
- Provide the minimum backend information required to support each chip family,
  such as STM32F4, independently of project task selection. Put reusable hardware tasks and
  task-specific helpers in separate optional crates. The backend must not depend
  on those task crates. Shared HAL helpers and types live in a separate family
  crate, such as `ferroforge-stm32f4`, used by init and hardware tasks. This is a
  FerroForge-oriented HAL helper library.
- The family backend also owns its concrete chip target definitions. Firmware
  selects one; it does not maintain a separate copy of those chip facts.
- All firmware lives under one `firmware/` **grouping directory**, not an
  enclosing compiled crate. Each firmware has its own workspace containing
  composition and handwritten init. The user clarified that the earlier word
  "crate" referred to this directory grouping, inspired by Betaflight's target
  organization. Composition and handwritten init share one authored package;
  detailed Cargo configuration and checking-interface refresh remain open.
- Both task kinds use the same composition model. Interrupt bindings use the
  required enum; its owner and precise chip-availability API remain open.
  Hardware tasks require the binding; software tasks do not.
- Hardware interrupt tasks are included in this repair, not deferred behind a
  software-only completion claim.
- A composition can select definitions from multiple independent task crates.
- The legacy `composer` / `embedded` / `generated` example must keep working
  during migration.
- One unified target is a main architectural pillar. Target information must
  not be independently maintained throughout the workspace.

Existing agreements still apply: preserve complete task/init bodies and their
supported source scopes; use native HAL initialization; keep target
implementations out of the host dependency graph; derive source dependency
requirements from Cargo manifests; retain independent checks and final real
RTIC checks/builds. A mock check alone is not a firmware correctness proof.

## Evidence of the Current Gaps

| Current location | Observed limitation | Required change |
| --- | --- | --- |
| `composer/src/composition.rs` and `ferroforge-macros/src/lib.rs` | Legacy `composition!` selects names, priorities, configuration, and spawn bindings for the app-backed pipeline; it can restate but cannot change embedded interrupt bindings | Add the standalone declaration/resolution path while retaining legacy behavior |
| `systems/nucleo-f401re/app_composition/src/lib.rs` | A profile feeds a hardcoded blink/report graph, target/runtime settings, and pipeline implementation | Move generic machinery into the framework; author the actual graph in the system |
| `systems/nucleo-f401re-fast-blink/app_composition/Cargo.toml` | The second system depends on the first system's composer | Both systems depend directly on shared framework tooling |
| `ferroforge-renderer/src/composition.rs` | Validation consumes one task-source collection; selections have no hardware interrupt binding | Resolve multiple packages and represent software/hardware scheduling |
| `ferroforge-contracts/src/lib.rs` | Standalone task contracts require safe async functions | Support and independently check synchronous hardware handlers |
| `ferroforge-renderer/src/transplant.rs` | Generated module namespace names encode module paths but not package identity | Preserve package-qualified identity and isolate colliding source modules |
| `ferroforge-renderer/src/init_check.rs` | Initial context generation contains STM32F401-specific types and creates spawn interfaces for selected tasks | Derive context from the selected target; expose spawning only for spawnable software tasks |
| `ferroforge-renderer/src/standalone.rs` and the first system composer | Target files, PAC path, runtime requirements, and build target are assembled in separate places | Consume one resolved target throughout checking, generation, and building |
| Root `Cargo.toml`, `ferroforge/Cargo.toml`, and composer binaries | The facade is a library and the executable entry points are example/system composers | Add the general FerroForge CLI, project creation/recognition, and required library marker handling |

## CLI Projects and Reusable Libraries

Agreed responsibilities, 2026-09-16; implementation remains pending:

| Component | Purpose |
| --- | --- |
| Reusable software task library | Task definitions intended for reuse across all hardware and projects, expressed against their declared interfaces rather than a particular HAL |
| Reusable hardware task library | HAL-specific task definitions reusable across projects using that HAL |
| Project `firmware/` | All firmware apps belonging to that project; each app has the agreed independent workspace |
| Family backend | Minimum information FerroForge needs to support the family and its chips, independent of project task choices |
| `ferroforge-stm32f4` | A FerroForge-oriented STM32F4 HAL helper library; init and tasks share its helpers and concrete types |
| FerroForge CLI | Shared command-line entry point with project conventions, a new-project helper, and rejection of unmarked FerroForge libraries; orchestrates the generation/check/build workflow |

Reuse is an authoring requirement. A software task still declares resource and
runtime requirements that a composition must satisfy; a hardware task requires
a compatible HAL and supported peripherals. No firmware-specific wiring or
project path belongs in a reusable definition.

Proposed organization: keep consumer project structure distinct from the CLI's
source repository and reusable library repositories. A project should select
libraries and backend support without copying the tool implementation or every
library into its tree. Local library development can remain possible; required
local directories, external library resolution, and backend distribution need
explicit conventions. The compact governing tree shows component layouts, not
one mandatory checkout containing all components.

Project conventions define how FerroForge finds its inputs. A helper command,
analogous to `cargo generate`, sets up a new project in the expected structure.
Changes that preserve recognition are allowed. When a project has changed so
far that required inputs cannot be recognized, the operation reports an error
identifying the problem. Do not add a general layout linter or reject unrelated
files and directories merely because they differ from the generated template.

Library checks beyond an explicit marker are deferred and will be added as
needed. A reusable library must identify itself as a FerroForge library; an
attempt to use it as such without the marker must fail with an error identifying
the crate and missing marker. Apply this requirement to libraries selected for
FerroForge use, not indiscriminately to ordinary Rust dependencies. The marker
declares intent; existing source, composition, and final RTIC checks still apply.

The project helper and library marker are requirements. Marker location/syntax,
command names, template details, and any new manifest format are unselected.
A dedicated library compatibility command and comprehensive check criteria are
not current implementation gates. Next choose the marker form, then settle
project-root discovery, required inputs, and library references.
Library scaffolding remains an optional proposal, distinct from the agreed
new-project helper.

## Unified Target Contract

There must be one authoritative definition for each supported target, selected
once by a firmware. Both F401 applications select the same target definition;
they do not copy or re-enter its fields. Supporting more than one target later
does not mean introducing several competing descriptions of a selected target.

The framework resolves that definition once and passes the same validated
target through every pipeline stage. Helpers must not independently choose
an F401 fallback, target triple, PAC, backend, memory layout, or probe chip.

| Owner | Information |
| --- | --- |
| Family platform backend | Minimum required family/chip support information, including concrete target definitions, independent of reusable task libraries |
| Reusable hardware task crates | Optional family-specific hardware task definitions, task helpers, and supporting code |
| Shared HAL helper crate (for example `ferroforge-stm32f4`) | FerroForge-oriented family HAL helpers and concrete types consumed by init and hardware tasks |
| Unified target | MCU/board identity, Rust target triple, PAC/device path, HAL/chip selection, RTIC backend, memory/linker settings, flashing/probe identity, supported monotonic/interrupt constraints, and required platform dependency selections |
| Firmware workspace | One target-checked authored package containing composition and handwritten init, with a usable Cargo/Rust Analyzer setup; detailed checking/refresh configuration remains to be specified |
| Firmware composition | Selected target reference, selected task packages/definitions, instance names, resource/configuration/spawn bindings, priorities, hardware interrupt assignments, software dispatchers, and selected monotonic configuration |
| Firmware init | Native peripheral acquisition and setup, clock initialization, pin modes, concrete resource construction, startup spawning, and interrupt-source setup/acknowledgment as appropriate |
| Shared framework | Parsing, discovery, validation, target resolution, checking-interface generation, transplantation, dependency resolution, file generation, and command orchestration |
| Generated output | Derived Cargo configuration, interfaces, RTIC attributes, linker scripts, probe configuration, and firmware files; these are not additional authoring authorities |

An interrupt assignment such as `TIM2` is an application choice against a
target, not a second MCU definition. Similarly, an init body's clock setup is
executable hardware initialization, not permission for the build pipeline to
maintain an unrelated clock assumption. Any clock value needed outside init
must have an explicit source and a defined way to reach its consumers.

Required consistency rules:

- Independent task checks, init checks, and generated firmware checks/builds
  use the selected target's Rust triple.
- Init context device types and generated RTIC device paths derive from the
  same target. HAL and backend feature choices must be compatible with it.
- Memory, linker offsets, probe configuration, and target Cargo configuration
  are generated from that target; no system-specific copies are hand-maintained.
- Target capabilities constrain the composition's dispatchers, interrupt
  bindings, and monotonic selection. Unsupported combinations get explicit
  diagnostics; checks requiring PAC/RTIC knowledge remain compiler checks
  unless the target definition provides enough information for earlier checks.
- Generated artifacts may repeat resolved values. Repetition is acceptable
  only when it is generated, not a second manually maintained definition.
- A change to a target is tested through all its consumers, not just firmware
  rendering. No API permits independently overriding only one consumer's target.

### Family Backends and Firmware Workspaces

The agreed logical hierarchy is below. Names illustrate the existing two
applications; this is not yet a selected Cargo manifest or source-file layout.

```text
firmware/ (grouping directory, not a Cargo package)
  nucleo-f401re (own workspace)
    composition
    handwritten init
  nucleo-f401re-fast-blink (own workspace)
    composition
    handwritten init
```

The per-firmware entries are application variants, not duplicate chip
definitions. Both select the STM32F401RE definition owned by the STM32F4 backend.

### Betaflight Structural Reference

Inspected upstream documentation and `master` on 2026-09-14. This comparison
informs the layout discussion; it does not import Betaflight's build system or
override the agreed Rust workspace/checking requirements.

Current board definitions live in the separate `betaflight/config` repository
at `configs/<MANUFACTURER_ID>/<BOARD_NAME>/config.h`, with optional `config.c`
and `config.mk` beside it. The config identifies its MCU using `FC_TARGET_MCU`
and specifies board hardware/resources. The older unified-target configuration
format was deprecated starting with 4.5. See the
[official configuration guide](https://betaflight.com/docs/development/manufacturer/creating-configuration).

The main repository keeps STM32 support under `src/platform/STM32/`, including
driver implementations, startup, linker, build, and MCU target directories.
For example, `target/STM32F405/target.mk` identifies `STM32F405xx` and the
`STM32F4` family. See the
[STM32 source tree](https://github.com/betaflight/betaflight/tree/master/src/platform/STM32)
and [F405 target selection](https://github.com/betaflight/betaflight/blob/master/src/platform/STM32/target/STM32F405/target.mk).

The main build uses `src/config` for the configuration submodule and derives
the MCU target from the selected board config. See
[config build integration](https://github.com/betaflight/betaflight/blob/master/mk/config.mk).

Our architectural analogy is shared family backends/chip definitions plus
grouped firmware-specific declarations. Our firmware entries also select task
graphs and contain handwritten init, so they are broader than Betaflight board
configuration files. Per-firmware Cargo workspaces are our requirement, not a
Betaflight feature. Manufacturer grouping and a separate configuration repository
have not been selected for this project.

### Backend and Checking Boundaries

Family ownership does not imply every chip in STM32F4 has identical peripherals,
interrupts, memory, or HAL features. Hardware-task selection and interrupt
bindings must be checked against the firmware's concrete target. The required
enum must cover supported-chip interrupts; its exact type organization remains
open. Accepting a family-wide variant alone is not proof that the selected chip
supports that interrupt.

Shared HAL helpers and types belong in their own family crate, such as
`ferroforge-stm32f4`. Handwritten init and hardware tasks consume that same crate;
generated firmware must preserve the dependency and concrete type identity,
rather than copying types independently into init and task scopes. Cargo
version/feature compatibility and dependency retention still need implementation
and verification. Native HAL dependencies must stay out of the host tooling graph.

A Cargo workspace alone is not evidence of working Rust Analyzer type checking.
The firmware needs an explicit host-composition/target-init checking arrangement,
generated interface availability, and target/feature selection. Verification
must use authored files in that firmware workspace, not only final generated
RTIC code. Moving the current composers out of the root tooling workspace, if
required by the selected layout, is an explicit migration task.

## Decisions Still Open

These are explicit questions or implementation proposals, not inferred user
agreements. Resolve them before treating the plan as implementation-ready.

The [interrupt task-kind rule](#interrupt-task-kind---resolved), enum requirement,
and RTIC-like authoring goal are agreed.

1. **Target access and board-specific settings:** family-backend ownership of
   concrete chip definitions is now agreed. Still settle where board-specific
   settings belong and how the host reads that definition without linking
   native hardware code. These implementation details do not reopen ownership.
2. **Cargo centralization (asked):** must the target also eliminate repeated
   HAL versions/chip features from source manifests, or can independently
   authored compatibility requirements remain and be checked against it?
   Centralizing these too requires a concrete Cargo inheritance/checking
   arrangement. The current isolated workspaces and dependency collector do
   not provide that arrangement. Merely creating a Rust target struct would
   not solve this part.
3. **Target authoring format (proposal pending items 1 and 2):** keep one canonical
   target descriptor and one resolved host model. Prefer Cargo-owned dependency
   requirements and target metadata over a parallel Rust version registry.
   Decide the exact file/package location and schema after settling inheritance;
   do not introduce both a TOML target and separately maintained Rust presets.
4. **Declaration grammar (proposal constrained by G6):** retain `composition!` as the entry point;
   introduce an unambiguous standalone form containing a target reference,
   init reference, named source packages, and named task instances. Each
   instance selects `package_alias::module::definition` and declares its
   scheduling/resource/configuration/spawn bindings. Freeze exact spelling and
   a complete example before implementing the parser. Legacy invocations keep
   their current interpretation. Show ordinary task/init bodies and familiar
   RTIC declarations first; justify each addition needed for reusable definitions
   or firmware composition. Do not introduce an independent task/init DSL.
5. **Initial scope limits (proposal):** prove the new interface on the existing
   F401 target, with native synchronous interrupt handlers and async software
   tasks. Additional boards, arbitrary Rust macro expansion, host simulation,
   init-local storage, and a new init-configuration API are not proposed gates.
   Confirm these limits; do not use them to omit the confirmed hardware or
   multi-package requirements.
6. **Rust Analyzer preparation and refresh (pending discussion):** the
   declaration-driven workspace layout below is agreed. Specify how a fresh
   checkout gets checking interfaces and backend-derived Cargo/editor settings,
   how edits refresh them, and how target changes reload the checking context.
   The detailed IDE proposal below is not yet an agreed implementation mechanism.
7. **Shared helper integration and interrupt API:** implement the agreed family
   helper-crate dependency with shared type identity; decide the interrupt enum's owner,
   chip-specific availability, and conversion to real RTIC bindings. Do not
   introduce an unrelated hardware-only composition frontend.
8. **Project conventions and library marker:** choose the marker location/syntax,
   then specify minimum project recognition inputs, new-project helper output,
   library references, backend discovery, and command names. G7 requires useful
   recognition errors and rejection of unmarked FerroForge libraries. Broader
   library checks remain deferred and are added as needed.

Discuss item 8 next, then interrupt enum ownership and selected-chip validation,
then the complete RTIC-like authoring examples and item 6. Cargo and checking
mechanics must implement the agreed layout. Unknowns stay open rather than
becoming silent defaults.

## Agreed Declaration-Driven Workspace Layout

The user accepted declaration-driven authoring: one authored checking package
at each firmware workspace root, containing composition and handwritten init,
read by shared host tooling. There is no per-firmware host composer executable.
This is an agreement, not a description of current code.

```text
firmware/
  nucleo-f401re/
    Cargo.toml              # authored package and its workspace
    src/
      lib.rs                # checking crate root
      composition.rs        # composition! declaration
      init.rs               # handwritten init, resources, local helpers
    .ferroforge/            # generated checking interfaces/configuration
    gen_app/                # generated standalone firmware build package
  nucleo-f401re-fast-blink/
    ...                     # same authoring layout, own workspace
```

Agreed boundaries:

- The authored package checks for the selected embedded target. Composition is
  declarative input and its checking expansion, not a host program that links
  the native init. A shared framework host command reads the sources and runs
  discovery, checking, generation, and building; there is no firmware-specific
  `app_composition/src/main.rs` or copied pipeline implementation.
- `composition.rs` owns target selection and task wiring; `init.rs` owns native
  initialization and concrete resources. Splitting them into files is for
  readability, not a requirement for separate authoring packages.
- The `gen_app` directory remains a generated, standalone build package excluded
  from the authored workspace, rather than an authored workspace member that
  must exist before first generation. This keeps its final dependency graph
  and checking-only dependencies separate. It is build output beneath the
  firmware directory, not another hand-maintained firmware declaration.
- Generated checking interfaces must be refreshed before checking authored init.
  A fresh-checkout preparation path must work before the authored crate can
  compile. Rust Analyzer uses that package, selected target, and generated
  interfaces. Automatic refresh on composition changes needs implementation
  and evidence; it is not provided merely by adding a workspace manifest.
- Target-specific configuration is derived from the selected backend definition,
  not another manually authored `.cargo/config.toml` target selection.

Tradeoff: composition cannot depend on arbitrary host Rust execution to decide
its task graph. The generator and macro share a supported declaration parser;
they do not evaluate arbitrary Rust functions. Normal task/init bodies remain
Rust. Exact constant/path/enum resolution and Cargo dependency sharing still
need a subsequent explicit design.

Implementation consequences: the current host-composer path must be adapted,
and init discovery/checking must support the proposed file-backed init module
instead of assuming a separate crate-root init package. The alternative is
separate host-composition and target-init packages in each firmware workspace;
that alternative was not selected. The agreed layout still needs implementation.
The legacy example continues working during migration.

## Rust Analyzer Checking and Refresh - Detailed Proposal

The user requested this explanation after accepting the authoring layout.
The checking goals follow existing agreements. The subsequent review correction
is important: typed spawn mocks and init checking interfaces already exist.
They are not new design work. The build-hook discussion below is only an
unselected refresh option, not a prerequisite or an agreed replacement.

### Existing Implementation to Reuse

- `ferroforge-macros/src/lib.rs` generates typed `SpawnHandle` methods from a
  standalone task's declared outgoing signatures, such as
  `spawn = [report(value: u32)]`. Calls through `cx.spawn.report(...)` are
  already checked without knowing the eventual firmware instance mapping.
- `ferroforge-renderer/src/init_check.rs` already derives instance-named
  `task::spawn(...)` checking functions from selected task signatures.
- `#[ferroforge::init]` already loads those generated interfaces through
  `FERROFORGE_INIT_INTERFACES`, retaining the handwritten function and its spans.
- `standalone_check` and `init_check` integration tests already cover checking
  behavior; both also contain opt-in authored-source Rust Analyzer tests.
  Earlier bounded verification is recorded in the initial implementation plan.

Remaining work is to connect these mechanisms to the agreed per-firmware
declaration/layout and unified target, extend their coverage for family hardware
tasks and multiple source packages, and verify refresh behavior in that layout.
First exercise existing interface generation/loading and establish the actual
refresh gap. Do not implement a second spawn-mocking system or add a build hook
merely because this explanatory proposal described one.

### What the Editor Checks

The authored checking package is a real Rust library checked for the firmware's
selected embedded target, with the matching backend/HAL feature selection.
Handwritten init and shared HAL helper calls use real types and ordinary Rust
ownership, borrowing, trait, and function-signature checking. The checking layer
does not replace the HAL with dummy peripheral types or execute initialization.

The existing generator supplies composition-dependent RTIC interfaces: init's core/device
context, selected software task spawn signatures, and the chosen monotonic's
supported startup/checking API. A selected report task accepting `u32` produces
a checking spawn function accepting `u32`; passing a string or referring to a
removed instance must fail on the authored init usage after refresh. Hardware
tasks must not acquire software spawn interfaces.

Reusable software and hardware task crates also retain their own
contract-based source checks. Merely listing a task in a declaration does not
cause Rust Analyzer to analyze its body if that source is not in the loaded
Cargo graph. The shared checker must check participating source packages;
source navigation and external-package IDE diagnostics need explicit integration
and tests rather than a promise based on declaration strings.

Composition has two kinds of diagnostics. Ordinary Rust checking applies to
typed expressions actually emitted by the macro. Structural errors such as
unknown instance bindings, duplicate interrupts, or unavailable definitions
require the shared declaration resolver/validator. Macro tokens that are only
stringified do not automatically get Rust name resolution, completion, or
rename support. Preserve authored spans where possible and explicitly test
which diagnostics appear on declaration tokens versus generated code.

Final concrete resource compatibility, RTIC concurrency/scheduling constraints,
transplantation, dependency merging, and link correctness still require checking
and building `gen_app`. Existing bounded checks do not yet prove all these
errors can be reported early in authored files. A clean editor is not a
replacement for the final build or for hardware testing.

### Candidate Preparation and Edit Cycle - Subject to a Demonstrated Gap

Evaluate reuse of the current generation/loading path first. The following hook
is a possible way to automate refresh if the new layout requires it, not newly
required spawn functionality. It needs a separate decision and evidence before
implementation.

1. Shared tooling resolves the firmware declaration and backend target before
   the first authored-package compilation. It prepares source-resolution inputs
   and derived Cargo/editor configuration without requiring `gen_app` or a
   successfully compiled init. Installed toolchains/dependencies remain real
   prerequisites; this step does not silently install or download them.
2. A small framework-supplied build hook generates only checking interfaces,
   using the shared parser/validator and prepared source inputs. It writes to
   Cargo's `OUT_DIR` and exposes the interface location to the checking macros.
   This hook is not a per-firmware composer program. It must not recursively
   invoke Cargo, generate/build the full firmware, or execute hardware code.
3. The hook tracks composition, selected source contracts, backend descriptor,
   and relevant manifests so saved changes trigger regeneration. Cargo provides
   `rerun-if-changed` and build output/environment mechanisms; Rust Analyzer
   supports build scripts, procedural macros, and check-on-save. See
   [Cargo build scripts](https://doc.rust-lang.org/cargo/reference/build-scripts.html)
   and [Rust Analyzer configuration](https://rust-analyzer.github.io/book/configuration.html).
4. Normal init body edits use ordinary editor analysis and compiler checks.
   Composition/contract changes refresh checking interfaces on save; this is
   not a promise to run generation on each keystroke. Verify editor invalidation
   as well as Cargo reruns: regeneration alone does not prove completion or
   diagnostics updated in the editor.
5. Selecting a different chip, feature set, or source package may require
   re-preparation and project reload. A build script cannot choose a different
   Cargo compilation target after Cargo has started the build. Shared tooling
   must refresh the derived setup before subsequent checks; no manually
   synchronized second target selection is acceptable.

If build-hook input resolution or macro refresh cannot be made reliable, use a
shared prepare-and-check editor command and explicit project reload as the
documented fallback. It must preserve Cargo JSON diagnostics and support the
editor's project-loading/build-script path as well as its on-save check path.
Do not silently leave completion on stale interfaces while updating only error
messages. The exact hook/resolver API is not selected by this proposal.

### Isolation and Evidence

Start with one firmware workspace as the editor's active project. Checking
another firmware must use its own selected target, features, and generated
interfaces; do not merge every chip feature with `--all-features`. Simultaneous
editing across different targets requires an explicit multi-project setup or
separate editor contexts, not an assumed repository-wide target override.

Acceptance tests must cover a fresh checkout without generated artifacts,
authored HAL errors, spawn type errors, renaming/removing an instance, external
task-contract edits, changing the selected target, failed generation followed
by recovery, and two firmware workspaces without interface leakage. Verify
completion/hover and diagnostic locations in the real editor/LSP separately
from compiler tests. Missing or stale interfaces must be reported clearly;
never present an old successful check as validation of a new declaration.

## Proposed Implementation Sequence

All phases below are pending. Their order includes dependencies; a passing
earlier phase is not completion of this repair.

### R0 - Freeze the Authoring and Ownership Contract

Resolve the open decisions. Write complete declarations for both firmwares and
a family-backend/software-package example, with handwritten init and firmware
workspace manifests. Check those examples against G1-G7, including the clarified
interrupt rule and the requirement to extend familiar RTIC authoring. Specify
where package/output names, runtime/logging choices, and generated interfaces
are authored; which values are required; and every permitted default.
Include a complete consumer project and standalone reusable task library layout,
with the project helper, recognition behavior, and library marker required by
G7. Do not silently turn illustrative paths into mandatory
conventions or treat the project template as an exhaustive list of allowed files.

Define path resolution relative to the declaring manifest, never the caller's
working directory. Define configuration expression scope explicitly: expressions
are transplanted Rust, not automatically references to host composer constants.
Define missing/unknown/duplicate-key errors. Record supported Cargo dependency
forms and how target choices reach independent source checks. Freeze the
shared HAL helper dependency/type-identity boundary and an authored-file Rust Analyzer
checking workflow before claiming the workspace design is settled.

Exit gate: an agreed complete example and ownership table, without placeholder
target data or unresolved choices hidden inside implementation tasks.

### R1 - Implement the Single Target Source and Its Consumers

Introduce the agreed canonical target representation and resolver. Inventory
current target literals in active source, manifests, build scripts, and Cargo
configuration. Classify each as authoritative input, source compatibility
requirement, generated value, test expectation, or obsolete duplication.

Establish the minimal STM32F4 backend and its agreed relationship to concrete
target definitions. Settle its packaging in R0; keep family support and
chip-specific selection distinct.

Replace scattered build/render/init-interface settings with the resolved
target. Implement the Cargo arrangement selected in R0, including collector
support and independent-check invocation. Do not silently relax conservative
dependency-conflict checks. Keep task/init implementation packages out of host
linking. Route legacy target handling through the same authority or a derived
compatibility adapter; an adapter cannot become another maintained target table.

Exit gate: one target edit reaches all relevant checking and generated outputs;
conflicts fail clearly; both existing pipelines and the legacy example still
work. No independent F401 preset remains in a consumer.

### R2 - Add the Per-System Declaration Frontend

Extend macro parsing and static declaration types without introducing a host
renderer dependency into the no-std checking layer. Resolve declarations in
host tooling into the existing binding model, extended where required.

Support explicit source/definition/instance identity, resource mappings, typed
configuration, outgoing spawn bindings, priorities, dispatchers, monotonic
selection, and hardware interrupt bindings. Remove graph construction from
the system profile/template API only after equivalent declarations work.

Exit gate: parser/compile-fail and lowering tests cover the agreed syntax;
legacy macro tests still pass. System authors can change a task graph without
editing shared renderer code or adding fields to a blink-specific profile.

### R3 - Resolve and Transplant Multiple Task Packages

Discover/check each distinct participating package once using the selected
target. Resolve aliases to Cargo/package/module/definition identities. Define
duplicate aliases and repeated references to the same package explicitly.

Include package identity in stable generated namespaces without leaking
machine-specific absolute paths into generated names. Keep supporting code
once per logical module, distinguish identical module/function names across
packages, and preserve shared type identity within a module. Multiple instances
of one definition get independent bindings, not duplicated supporting types.

Merge retained-source dependencies from every participating package, init, and
target/runtime requirements. Report conflicts with their contributors. Do not
infer support for arbitrary inter-package source references: supported paths
need explicit rewriting/tests, and unsupported crossings need diagnostics.

Exit gate: two independent task crates with colliding module/helper/task names
produce checking and release-linked firmware. Repeated instances, retained
support, cross-package software spawning, and dependency conflicts have tests.
Include a struct from the shared HAL helper crate used by both handwritten init and a hardware
task, proving compatible type identity and visibility through final checking.

### R4 - Add Standalone Hardware Interrupt Tasks

This phase follows G3's confirmed hardware-binding rule. Review the exact
authoring signatures before freezing the API and scheduling rules.

Extend shared contracts and checking expansion for synchronous hardware
handlers with native HAL resources. Proposed initial signature: safe,
non-generic `fn(task_name::Context)` returning `()`, with no software-spawn
payload. Confirm this exact contract in R0. The composition assigns the
interrupt and priority; the reusable task does not fix a system's scheduling.
Place hardware definitions and task-specific helpers in optional STM32F4 task
crates, separate from the required STM32F4 platform backend. Verify that the
backend remains usable without those task crates.
Use the same resource/configuration/instance machinery as software tasks, adding
the required interrupt binding and hardware-specific validity checks. Implement
the agreed enum/binding API and selected-chip validation.

Validate task kind against bindings, duplicate interrupt ownership, dispatcher
collisions, and prohibited spawning of hardware handlers. Emit real RTIC
`binds` attributes. Generate init/task spawn interfaces only for software
targets. Allow a hardware handler to spawn software work using the existing
typed alias contract. Interrupt setup belongs to native init and acknowledgment
belongs to the appropriate handler; generation must not invent either body.

Exit gate: a HAL-backed interrupt fixture plus software tasks from another
package independently check and produce release-linked F401 firmware. Negative
tests cover invalid signatures/bindings/spawns and concrete HAL resource types.
This is compile/link evidence, not a claim of on-board interrupt behavior.

### R5 - Share Orchestration and Migrate Both Systems

Move generic pipeline stages/errors/execution into framework-owned tooling.
Use the resolved declaration, package set, init, and single target as inputs.
Preserve stage ordering, fail-fast behavior, and a render-only testing entry
point. Provide an explicit init-interface regeneration/check command usable
from the authored init workspace/IDE; do not claim IDE support from parsing alone.

Give each firmware the agreed Cargo workspace and migrate package membership
without silently changing the legacy workspace. Demonstrate Rust Analyzer
diagnostics on handwritten init, shared HAL helper/resource use, and composition
bindings to the extent promised by the agreed checking API. Document any checks
that still require generation or final RTIC compilation.

Replace both system profiles with actual `composition!` declarations. Retain
their current 500 ms and 125 ms behavior, resource names, and native init unless
the agreed authoring layout requires relocation. Remove the second composer's
dependency on the first. Preserve the legacy command through migration; do not
retire its files or API under this plan.

Implement the shared CLI entry point, new-project helper, project recognition,
and library marker requirement under the contract agreed in R0.
It must operate on consumer projects without requiring a custom composer binary
or the FerroForge development repository layout.

Exit gate: the shared CLI independently runs the whole pipeline for each app. A
fixture composing a different graph demonstrates that shared orchestration
contains no blink/report names, fixed task count, first-system path, or
independently selected target settings.

### R6 - Verify the Contract and Correct the Documentation

Run host tests, both complete F401 pipelines, the hardware/multi-package proof,
and legacy checks/builds. Preserve current failure-boundary regressions and
extend them for multiple source checks, target conflicts, interrupt binding,
package collisions, unrecognizable required project inputs, and attempts to use
unmarked FerroForge libraries. Verify that a generated
project is recognized and that harmless project additions do not cause layout
errors. Verify CLI use from a separate consumer project and reuse of the same
library across projects. Prove that a marked library is accepted for discovery
and that removing its marker produces the intended error when it is selected.
Real negative tests must assert the intended cause,
not merely any failure at the expected stage.

Verify source preservation, deterministic generation, repeated execution, and
target consistency across init interfaces, RTIC source, Cargo configuration,
memory/linker/probe files, and commands. Check that generated firmware has no
checking-only framework dependency. Update architecture, prototype, workflow,
dependency policy, and the decision record to match delivered behavior.

Exit gate: every confirmed requirement has passing evidence and every remaining
limitation is documented. Do not declare the repair complete from two renamed
instances of a hardcoded graph or from successful rendering alone.

## Verification Status

This document records inspection and a proposed repair sequence only. No new
Rust implementation or build evidence is claimed. Existing recorded successful
checks belong to the old bounded implementation; they do not validate this plan's
new macro, hardware, multi-package, or unified-target behavior.
