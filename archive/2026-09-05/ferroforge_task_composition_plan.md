# FerroForge Task Composition Implementation Plan

Reviewed on 2026-09-05 against the implementation and the clarified
[source-transplant architecture](ferroforge_source_transplant_architecture.md).
The architecture direction is agreed. The review below records open decisions
for discussion one at a time; proposed mechanisms and example syntax are not
yet implementation commitments.

## Goal

Create a reusable embedded task system where Rust performs type checking
before generation.

The reusable unit is a complete RTIC-style task declaration and body. Each
system owns its app composition and an init module whose body is transplanted
through the same mock-check-to-real-RTIC process. The host renderer reads task
and init sources without linking their target implementations.

The system avoids a large intermediate representation (IR). Instead, it
uses:

-   Real Rust crates for reusable tasks.
-   Real HAL types for hardware-specific tasks.
-   `embedded-hal` traits for portable software tasks.
-   Mock RTIC syntax for reusable RTIC-style tasks.
-   A host-side renderer that composes the final firmware crate.

A small shared declaration model is appropriate for contracts and bindings.
Rust remains responsible for type checking; the renderer does not model the
semantics of arbitrary Rust task bodies.

## Workspace Structure

    ferroforge/

    ├── systems/
    │   ├── foxeer_f405v2/
    │   │   ├── init/
    │   │   ├── app_composition/
    │   │   └── gen_app/
    │   │
    │   └── speedybee_f405v5/
    │
    ├── tasks/
    │   ├── stm32f4/
    │   │   ├── uart/
    │   │   └── schedule/
    │   │
    │   └── sw/
    │       ├── blink/
    │       └── report/
    │
    └── code_renderer/

Each workspace has its own target configuration. The system app composition
and renderer run on the host; HAL-specific task and init checks and the final
firmware build run for the appropriate embedded target. Portable task checks
may also run on the host.

These boundaries provide independent checking, configuration, and dependency
resolution. They are an architectural choice: Cargo can itself compile host
build scripts and procedural macros during a cross-build. Generating the
final manifest is followed by a separate Cargo invocation for that project.


## Task Crates

Tasks are normal Rust crates.

They must:

-   Compile independently.
-   Provide Rust analyzer support.
-   Use real types where possible.
-   Avoid system-specific dependencies.

Illustrative task shape (the independent typed contract is still to be
defined in review item 1):

``` rust
#[ferroforge::task(local = [led])]
async fn blink(mut cx: blink::Context) {
    let _ = cx.local.led.set_high();
}
```

Resource names alone do not make this a standalone compilable task. Its
contract must provide the type or trait bounds used to generate the mock
context. The developer continues to write the task body through that context.

The task crate does not know:

-   Board.
-   Pin mapping.
-   Interrupt allocation.
-   Final task instance name or priority.
-   Final RTIC application.

## HAL-Specific Tasks

Hardware tasks live in HAL workspaces.

Example:

    tasks/stm32f4/uart

Uses the real selected STM32F4 HAL types. The current prototype uses
`stm32f4xx-hal`; any separate FerroForge HAL package is a future choice.

Purpose:

-   Provide reusable STM32F4 functionality.
-   Validate against the actual HAL.
-   Keep target dependencies isolated.

## Software Tasks

Portable tasks use `embedded-hal`.

For example, a task can require its `led` resource to implement
`embedded_hal::digital::OutputPin`, then use `cx.local.led.set_high()` in its
body. The independent check must enforce that declared interface. Checking
against one concrete HAL pin alone does not establish portability.

The syntax for declaring resource bounds and the mock expansion are open.
If checking introduces generics, the renderer must define how those become
concrete RTIC handlers while preserving the complete task body.

Benefits:

-   MCU independent.
-   Host testing where executable resource implementations are provided.
-   Reusable across platforms.

Compile-only RTIC mocks are not a simulator. A simulation runtime is a separate
optional objective.

## Mock RTIC Layer

Extend the existing `ferroforge` and `ferroforge-macros` facilities to support
independent task and init declarations. Whether to expose a separate
`mock_rtic` package or keep the existing names is a packaging decision.

Responsibilities:

-   Provide RTIC-like attributes.
-   Preserve IDE support.
-   Allow reusable tasks to compile.
-   Check bodies against declared resource, configuration, spawn, and clock
    interfaces.
-   Match the supported RTIC signatures, result types, and resource lifetimes.

Example:

Input:

``` rust
#[ferroforge::task]
async fn report(_cx: report::Context)
{
}
```

Generated inside `#[rtic::app(...)]`, using composition-owned priority:

``` rust
#[task(priority = 1)]
async fn report(_cx: report::Context)
{
}
```

The mock layer supports local Rust checks without executing a scheduler or
proving whole-application RTIC constraints. Composition validates declared
bindings and structural constraints. Rust and the real RTIC macro check the
complete generated application.

For example, a FerroForge call `cx.spawn.report(value)` bound to the
`telemetry` instance must become `telemetry::spawn(value)`, with compatible
input and result types. See the [RTIC task documentation](https://rtic.rs/2/book/en/by-example/software_tasks.html).

## System Workspace

The system defines:

-   Board.
-   Init.
-   Task composition.
-   Hardware configuration.

Proposed composition shape, not currently supported syntax:

``` rust
app!(
    board = FoxeerF405V2,

    init = init,

    tasks = [
        blink,
        uart,
        report
    ]
);
```

Init remains system-owned because it contains:

-   Clock setup.
-   Peripheral creation.
-   Pin mapping.
-   Resource construction.

Init must independently check using a mock `init::Context` with real target
peripheral types, together with the task spawn, configuration, and monotonic
interfaces it references. The renderer transplants the init body and its
declared supporting source into the generated app alongside `Shared`, `Local`,
and the selected tasks.

The architecture document illustrates a future `#[ferroforge::init]`
attribute. Today init is supported only as `#[init]` inside the existing
`app!` declaration. The independent init contract is review item 2.

## Code Renderer

Runs on the host target.

Responsibilities:

1.  Parse `app_composition`.
2.  Locate selected task sources and the system init source.
3.  Parse their contracts and declared supporting source.
4.  Transplant task and init bodies, applying instance, resource,
    configuration, spawn, and monotonic bindings.
5.  Emit real RTIC syntax and generate `gen_app`, including its manifest and
    target build configuration.
6.  Orchestrate the required source checks and final target check/build. The
    current composer only renders; these checks are currently separate commands.

The renderer does not:

-   Replace Rust type checking.
-   Maintain a complete IR.
-   Understand HAL internals.

## Review and Open Decisions (2026-09-05)

Walk through these in order. All six items remain open; recording a
recommendation does not mean its API or implementation has been selected.
The current discussion starts at item 1.

| Item | Topic | Status |
| --- | --- | --- |
| 1 | Typed contract for independent task checking | Open - next discussion |
| 2 | Independent system init contract | Open |
| 3 | Supported mock API and real RTIC translation | Open |
| 4 | Supporting source and reference scope | Open |
| 5 | Validation guarantees and dependency consistency | Open |
| 6 | Implementation order and acceptance criteria | Proposed - confirm after earlier decisions |

### 1. Typed Contract for Independent Task Checking

Finding: `local = [led]`, `config = [period_ms]`, and `spawn = [report]`
declare names without specifying the resource interface, configuration type,
or spawn signature. The current macro obtains resource bindings, configuration
constants, and spawn methods from a surrounding `app!` in the same crate.
See [resource binding expansion](ferroforge-macros/src/lib.rs),
`expand_task` and `expand_rtic_app`.

Required decision: how a reusable task declares local/shared resource types or
trait bounds, configuration types, task inputs, spawn signatures, and clock
requirements before any system selects it. The mock context must derive from
that contract. Portable checks must enforce the declared bounds. Any generic
check representation needs a defined conversion into a concrete RTIC handler.

Acceptance: a task compiles in its own workspace with no system dependency;
invalid resource operations and spawn arguments fail there. Its whole body can
then be transplanted using compatible concrete bindings.

### 2. Independent System Init Contract

Finding: real HAL peripheral types are available to the system, but an
independent init module also needs the interfaces behind references such as
`blink::spawn(...)`, task configuration, and monotonic startup. Today these
are supplied by the containing app declaration in
[embedded/src/lib.rs](embedded/src/lib.rs).

Required decision: how init obtains those check interfaces from the same
contracts used by composition, without manually duplicating signatures or
making ARM implementations dependencies of the host renderer. Specify the
initial init API, including whether `cx.cs` and init-local storage are in scope.
The standalone `#[ferroforge::init]` attribute remains a proposed API.

Acceptance: the system's init body and resource construction compile for its
target independently of `gen_app`; its task/configuration references stay
consistent with the selected composition.

### 3. Supported Mock API and Real RTIC Translation

Finding: the earlier examples did not express the intended context-based task
shape and incorrectly showed `#[rtic::task]` as the generated attribute. The
actual renderer already emits `#[task(...)]` inside `#[rtic::app(...)]`.
It validates/stores spawn bindings but does not apply them in `render_task`
in [ferroforge-renderer/src/lib.rs](ferroforge-renderer/src/lib.rs).

Required decision: define the supported RTIC version/API, including input and
spawn result types, resource lifetimes, and hardware/software task shapes.
Composition owns final priorities and interrupt bindings. Explicitly specify
how mock spawn aliases, configuration paths, resource names, and instance
context paths are translated in both task and init bodies.

Acceptance: a typed spawn alias bound to a differently named task compiles in
the final RTIC app, including code handling the failure result. The mock must
not accept incorrect types or lifetimes just to make an example pass.

### 4. Supporting Source and Reference Scope

Finding: the [source loader](ferroforge-renderer/src/loader.rs),
`load_task_implementations`, currently copies annotated functions and omits
their surrounding helpers and imports. Relocation can also change the meaning
of `crate::`, `super::`, feature conditions, and references inside macros.

Proposed starting point: an explicitly declared source module and a documented
set of allowed reference patterns. Specify which supporting items move with a
task or init, how names and visibility are preserved, and how unsupported
patterns are diagnosed. This source boundary has not yet been selected.
Rewrites must preserve literals and unrelated identifiers; the current macro
configuration rewrite uses string replacement and needs a safer defined scope.

Acceptance: a task and an init function using declared helpers/imports retain
their meaning after transplantation; unrelated names and strings stay intact.

### 5. Validation Guarantees and Dependency Consistency

Finding: each independent check proves a body against its own contract. It does
not prove that every resource mapping or spawn binding in a concrete
composition is valid. Comparing Rust type spellings in the host renderer is
insufficient. Likewise, a check against one HAL version does not validate source
against a different final version or feature set.

Required decision: whether concrete binding checks happen only in the mandatory
generated firmware check or also in an earlier target check harness. Define
how Cargo dependencies used for task/init checks stay compatible with the
generated manifest. Independent source crates still need their own Cargo
dependencies; central Rust metadata cannot supply missing Cargo dependencies.

Acceptance: incompatible concrete bindings fail in a Rust check, and generated
dependency choices cannot silently be treated as covered by unrelated source
checks. The real RTIC check/build remains mandatory.

### 6. Implementation Order and Acceptance Criteria

Finding: the original phases started with components already present in the
prototype and delayed renderer work until after generation. The next proof
must exercise separate task and init workspaces through final transplantation.

Proposal: use the revised phases below. Establish independent contracts first,
then validate them through one complete generated application. Add a second
system using unchanged reusable task source before broadening target/HAL
support. Keep host simulation separate from compile-only mock support.

Review baseline: on 2026-09-05, all 23 macro tests and 7 renderer/loader tests
passed, the embedded library checked for `thumbv7em-none-eabihf`, and the
existing generated firmware completed a release build. These results validate
the current example, not the proposed separated task/init design.

## Development Order

These phases are a proposed implementation sequence pending the decisions
above. Each phase must preserve complete task/init source transplantation.

### Phase 1 - Define Contracts and the Supported Mock API

-   Resolve task and init contract syntax and ownership.
-   Define the supported RTIC APIs and concrete handler translation.
-   Set the initial supporting-source boundary and validation guarantees.

### Phase 2 - Prove Independent Source Checking

-   Create one reusable task crate with no system dependency.
-   Create one system init crate with real HAL resource construction.
-   Verify independent compilation, meaningful type failures, and Rust Analyzer
    feedback under the correct targets.

### Phase 3 - Share Declaration Parsing

-   Share a small declaration model/parser between macros and renderer.
-   Resolve source packages/modules and distinguish definitions from instances.
-   Validate composition completeness and declared bindings consistently.

### Phase 4 - Implement Composition-Driven Transplantation

-   Transplant complete task and init bodies with their declared support items.
-   Apply task-instance, resource, configuration, spawn, and clock mappings.
-   Generate the firmware manifest and target configuration.

### Phase 5 - Prove the Complete Pipeline

-   Use a mapped resource and a typed task-to-task spawn alias.
-   Spawn a selected task from the independently checked system init.
-   Check the generated app with real RTIC and build the target firmware.
-   Add failure cases that exercise contract/binding incompatibility.

### Phase 6 - Prove Reuse Across Systems

-   Add a second system using unchanged reusable task source.
-   Verify instance/resource mapping and system-specific init ownership.
-   Consolidate target metadata and add further boards/HAL support as needed.
-   Consider executable host simulation as a separate extension.

## Design Principle

Keep Rust as the source of truth.

Use generation only for the parts that require global knowledge:

-   Application composition.
-   RTIC wiring.
-   Hardware resource assignment.

Everything else should remain normal Rust code.
