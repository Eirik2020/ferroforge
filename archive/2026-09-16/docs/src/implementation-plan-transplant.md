# implementation-plan - Archived 2026-09-16 (transplant design)

Archived from `docs/src/.md` when FerroForge moved to the call-through
model, in which nothing is transplanted. This chapter describes the source-
transplant design and the tooling built for it, all of which was removed from
the active tree. Binding decisions were promoted into the rewritten chapters
and the decision record before archival.

The code it describes is recoverable from branch `main` (full transplantation)
and `test/new_task_method` (transplanted init).

---

# Implementation Plan

Current continuation (2026-09-14): the required
[per-system composition and unified-target repair](composition-repair-plan.md)
is pending. The bounded proofs below do not mean the intended `composition!`
authoring interface is complete. The repair includes hardware interrupt tasks,
multiple task packages, one authoritative target, and legacy compatibility.

The implementation baseline was reviewed on 2026-09-05. This plan incorporates
the subsequent agreements in the [source-transplant architecture](architecture.md)
and [decision record](review.md). The initial design and implementation sequence
are agreed; explicitly proposed API spellings and remaining technical details
are still to be resolved during the scoped implementation proofs. The original
six-phase sequence was met for its bounded backend scope and is archived; the
active sequence is the [repair plan](composition-repair-plan.md).

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

Firmware layout is defined by G5 in the
[governing requirements](governing-requirements.md) and is not restated here.
Reusable software and hardware task crates and the host renderer live outside
any single firmware workspace; their placement remains open.

Each workspace has its own target configuration. The system app composition
and renderer run on the host; HAL-specific task and init checks and the final
firmware build run for the appropriate embedded target. Portable task checks
may also run on the host.

These boundaries provide independent checking, configuration, and dependency
resolution. They are an architectural choice: Cargo can itself compile host
build scripts and procedural macros during a cross-build. Generating the
final manifest is followed by a separate Cargo invocation for that project.


## Task Crates

Reusable task source lives in normal Rust crates. A crate may contain multiple
modules, each grouping related tasks with imports declared once at module
scope. Individual tasks remain selectable and keep their own requirements and
generated contexts. A separate crate per task is not required.

Use ordinary file-backed modules, inline modules, or the crate root as the
logical group; no extra FerroForge grouping wrapper is required. Carry each
selected module's ordinary support items together without a handwritten helper
list, while selecting task definitions individually. Discovery and generated
access layout must implement the agreed
[source boundary](architecture.md#source-transplant-boundary).

They must:

-   Compile independently.
-   Provide Rust analyzer support.
-   Use real types where possible.
-   Avoid system-specific dependencies.

Illustrative names-only task shape, not the selected standalone contract. The
[architecture chapter](architecture.md#software-task-clarifications) holds the agreed typed syntax:

```rust,ignore
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
Their detailed design is deferred while the current walkthrough focuses on
software tasks. System init still uses real HAL types to construct resources.

Example:

    tasks/stm32f4/uart

Uses the real selected STM32F4 HAL types. The current prototype uses
`stm32f4xx-hal`; any separate FerroForge HAL package is a future choice.

Purpose:

-   Provide reusable STM32F4 functionality.
-   Validate against the actual HAL.
-   Keep target dependencies isolated.

## Software Tasks

Portable tasks use `embedded-hal`. The agreed authoring design lives in
[reusable task workspaces](architecture.md#reusable-task-workspaces) and is not
restated here:

- [Independent task contract](architecture.md#independent-task-contract)
- [Software task clarifications](architecture.md#software-task-clarifications)
- [Local and shared resources](architecture.md#local-and-shared-resources-in-the-initial-sw-scope)
- [Task-local configuration access](architecture.md#task-local-configuration-access)
- [Task inputs and spawning](architecture.md#task-inputs-and-spawning)
- [SysTick mock direction](architecture.md#systick-mock-direction)
- [Modules group related tasks](architecture.md#modules-group-related-tasks)
- [Generated checking contexts](architecture.md#generated-checking-contexts)

The binding decisions behind those sections are recorded in the
[decision record](review.md#binding-decisions-carried-forward).

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

```rust,ignore
#[ferroforge::task]
async fn report(_cx: report::Context)
{
}
```

Generated inside `#[rtic::app(...)]`, using composition-owned priority:

```rust,ignore
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

Each firmware authors `composition!` alongside its handwritten init, per G5 and
the [repair plan](composition-repair-plan.md). The earlier
`app!(board = ..., init = ..., tasks = [...])` shape is superseded; the exact
declaration grammar is still unselected.

Init remains system-owned because it contains:

-   Clock setup.
-   Peripheral creation.
-   Pin mapping.
-   Resource construction.

Init must independently check using a mock `init::Context` with real target
peripheral types, together with the task spawn, configuration, and monotonic
interfaces it references. Generate these check-only interfaces from composition
and selected task contracts before the target init check. The host reads source
and metadata, not target implementations as dependencies; developers do not
hand-maintain duplicate task signatures. The initial implementation writes the
generated interface and Cargo configuration to an explicit output directory,
regenerated from the validated composition on each render, and checks the
original no-std init package against them. Init configuration spelling remains
open.

Clock, GPIO, and peripheral setup uses native HAL/PAC types, traits, and methods,
not FerroForge hardware-trait replacements. The mock context exposes real
peripheral types. Compilation checks init without executing hardware setup.
The renderer transplants the init body and its
declared supporting source into the generated app alongside `Shared`, `Local`,
and the selected tasks. Native HAL calls remain intact; real RTIC/monotonic
interfaces replace checking interfaces in the generated firmware.

`#[ferroforge::init]` is implemented as the standalone signature/source marker.
The legacy app-backed path continues to use `#[init]` inside `app!`. The initial
independent contract is deliberately limited to a qualified crate-root marker,
one synchronous `fn init(init::Context) -> (Shared, Local)`, and self-contained
root support. The independent init contract is in
[system-owned initialization](architecture.md#system-owned-initialization).

## Code Renderer

Runs on the host target.

Planned host pipeline responsibilities:

1.  Parse `app_composition`.
2.  Locate selected task sources and the system init source.
3.  Parse their contracts and declared supporting source, and validate the
    supported composition/dependency/profile requirements.
4.  Generate composition-derived init checking interfaces before the target
    init check.
5.  Transplant task and init bodies, applying instance, resource,
    configuration, spawn, and monotonic bindings.
6.  Emit real RTIC syntax and generate `gen_app`, including its manifest and
    target build configuration.
7.  Orchestrate source checks, generation, and final target check/build in the
    [validation-stage order](architecture.md#validation-stages), propagating
    failures. The first Nucleo system composer now runs this sequence; the
    legacy composer still only renders.

The renderer does not:

-   Replace Rust type checking.
-   Maintain a complete IR.
-   Understand HAL internals.

## Development Order

The agreed sequence proves independently checked SW tasks, then native HAL init,
then one complete generated firmware application, and finally reuse of unchanged
task source in a second system. The phases below break down that work. Each
phase must preserve complete task/init source transplantation.

Start with a file-backed module containing an LED task and a small reporting
task. Across that example, exercise local/shared resources, configuration,
SysTick delay, spawning, and native logging. Add focused negative tests as each
part is implemented. Prove private-helper access and supporting type identity
early, alongside independent checking, rather than waiting for final integration
to discover that the generated namespace layout cannot preserve them.

The acceptance bar is independently checked source producing buildable real
RTIC firmware, followed by demonstrated reuse. Exhaustive mock coverage is not
required. See [incremental coverage](architecture.md#incremental-coverage-and-early-checks).

Apply the agreed [incremental coverage policy](architecture.md#incremental-coverage-and-early-checks):
implement only the APIs needed by concrete uses, add inexpensive early checks,
and accept that the generated project's Rust Analyzer/Rust/RTIC checks will
find some remaining mistakes. Do not make exhaustive mock or monotonic
coverage, or a comprehensive earlier check harness, a prerequisite. Keep
target-aware compiler checks and the final build in the pipeline.

### Verification and Completion Rules

Each phase lists its deliverables, verification, and exit gate. Do not mark a
phase complete solely because its examples render or a mock body compiles.
Record exact fixture/source paths, targets, commands, and results when
implementing it. Both source/system pipelines and the representative negative
end-to-end proofs now exist.

- Host parser/renderer tests: run `cargo test --workspace --all-targets --locked`
  from the host workspace, including the new regression fixtures.
- Source checks: from each new source crate's workspace, run
  `cargo check --lib --target <declared-target> --locked`. For a portable SW
  check, the declared target may be the host; init uses its actual ARM target.
  Init is the exception: it needs the generated `.cargo/config.toml` that sets
  `FERROFORGE_INIT_INTERFACES`, so run its check from `.ferroforge/init-check`
  with `--manifest-path`. See [workflow](workflow.md#check-standalone-system-init).
- Firmware checks: from each generated workspace, run
  `cargo check --all-targets --target <firmware-target> --locked` and
  `cargo build --release --target <firmware-target> --locked`.
- Replace target placeholders with concrete values in the completion record.
  Create/update new lockfiles intentionally before using `--locked`; run from
  the appropriate workspace so Cargo loads its target/linker configuration.
- Compile-fail fixtures must fail for the intended error, not merely because a
  dependency, target, or generated interface is missing. The test harness must
  distinguish expected negative results from an unsuccessful verification run.
- Record a focused Rust Analyzer check of authored-source diagnostics where
  required. IDE feedback does not replace compiler checks. A build does not
  claim flashing, hardware validation, or executable mock simulation.

These commands specify the verification shape, not a new CLI already available
in the prototype. [Workflow](workflow.md) remains the source for today's commands.

## Archived History

The "Review Status" narration, the Phase 1 and Phase 2 progress records, and the
Phase 1-6 definitions were archived on 2026-09-16 to
`archive/2026-09-16/docs/src/implementation-plan-history.md`. All six phases were
met for their bounded backend scope; the active sequence is the
[composition and unified-target repair](composition-repair-plan.md). The archive
is not active design authority and requires explicit permission to read.

## Design Principle

Keep Rust as the source of truth.

Use generation only for the parts that require global knowledge:

-   Application composition.
-   RTIC wiring.
-   Hardware resource assignment.

Everything else should remain normal Rust code.
