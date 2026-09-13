# FerroForge Source-Transplant Architecture

This document records the agreed direction: complete reusable task bodies and
system-owned init bodies are checked through mock RTIC interfaces and
transplanted into a real RTIC application. The unresolved implementation
decisions are tracked in the [plan review](ferroforge_task_composition_plan.md#review-and-open-decisions-2026-09-05).
Exact contract syntax and the initial supported RTIC API remain open.

## Purpose

FerroForge enables developers to write complete RTIC-style tasks as reusable
Rust source, type-check that source before generation, and then transplant the
task into a generated, real RTIC application.

Reusable tasks are not merely libraries containing functions that generated
RTIC wrappers call. The reusable unit is the complete task declaration and
body, written with the same programming model used inside an RTIC app:

The following illustrates the intended task shape. Its resource, spawn,
configuration, and clock contracts are omitted; it is not yet a standalone
compilable task-crate example.

```rust
#[ferroforge::task(
    config = [period_ms],
    local = [led],
    shared = [enable_blink],
    spawn = [report],
)]
async fn blink(mut cx: blink::Context) -> ! {
    loop {
        let enabled = cx.shared.enable_blink.lock(|enabled| *enabled);

        if enabled {
            cx.local.led.toggle();
            cx.spawn.report().unwrap();
        }

        Mono::delay(blink::Config::PERIOD_MS.millis()).await;
    }
}
```

The mock FerroForge API and the task's declared contract must make this source
compile before generation. The renderer later combines it with system-owned
composition data and emits a `#[task(...)]` handler inside the final
`#[rtic::app(...)]` module.

## Why FerroForge Uses Multiple Workspaces

The workspace boundaries represent separate compilation environments and
validation stages. They are not only an organizational convention.

```text
Host workspace
  composer + renderer + shared schema
                  |
                  | reads source and metadata
                  v
Task and system check workspaces
  mock RTIC APIs + real embedded types
                  |
                  | source composition and generation
                  v
Generated firmware workspace
  complete real RTIC application
```

The host workspace runs the renderer using `std`, filesystem access, `syn`,
and `quote`. It must not link target-only ARM task or initialization code.

Task workspaces independently check reusable task definitions for their
appropriate environment:

- Portable software tasks use interfaces such as `embedded-hal` traits.
- HAL-specific tasks use the real target HAL and its concrete types.
- Different target families or incompatible HAL versions can use separate
  workspaces, target configurations, lockfiles, and feature resolution.

Each system also has its own source and generated-output boundary. A typical
layout is:

```text
systems/
`-- <system>/
    |-- init/
    |-- app_composition/
    `-- gen_app/
```

The generated firmware is a separate workspace because it has a different
purpose from the mock check workspaces. It contains the complete
`#[rtic::app]`, the final dependency manifest, target configuration, linker
layout, and probe configuration. It is checked and built in a second Cargo
invocation after generation.

The renderer can generate a new `Cargo.toml`, after which a subsequent Cargo
invocation resolves and builds the final firmware graph. A procedural macro or
build script cannot add dependencies to the graph already being compiled.

Separate workspaces are an architectural choice for independent checking,
configuration, dependency resolution, and generated output ownership. Cargo
can compile host build scripts and procedural macros during an embedded
cross-build; the requirement here is that the host renderer does not depend
on target-only task or init implementations. Cargo configuration is discovered
relative to the invocation directory and its ancestors, so commands must run
from the appropriate workspace or supply explicit target/configuration options.
See the [Cargo configuration reference](https://doc.rust-lang.org/cargo/reference/config.html).

## Validation Stages

FerroForge intentionally performs validation at more than one stage:

1. **Reusable task check**
   Rust checks each task body against its mock RTIC context and its portable or
   HAL-specific types.
2. **System initialization check**
   Rust checks the system's clock, peripheral, pin, resource, monotonic, and
   initial-spawn code against the real target HAL.
3. **Composition validation**
   The host composer resolves selected tasks, instances, resources,
   configuration, spawn aliases, priorities, interrupts, and dispatchers.
4. **Generated firmware check**
   The real RTIC macro checks the complete application and all global RTIC
   constraints.
5. **Final target build**
   Cargo compiles and links the generated firmware for the selected target.

Independent source checks validate each body against its own declared
contract. They do not prove that every concrete resource mapping or spawn
binding in a system is compatible. Rust must check those bindings in the
generated application; whether to add a target check harness for concrete
composition before final firmware generation remains an open decision.

The host composer can check declared names, completeness, and structural
constraints. Comparing the spelling of Rust types is not a substitute for
Rust checking their compatibility. Checked and generated source must also use
compatible dependency versions, target settings, and features.

The mock layer does not replace RTIC's final validation. Its purpose is to
provide useful Rust and Rust Analyzer feedback while developers edit reusable
task and system initialization source. Compile-only mocks do not supply a host
simulator; executable simulation is a separate optional objective.

## Reusable Task Workspaces

A reusable task workspace is both a source provider and an independent
compile-check environment. It is not a normal dependency of the host renderer.
The renderer discovers its package and source metadata without compiling its
target code for the host.

The mock task layer must reproduce the compile-time-facing parts of the RTIC
task API used by application code:

- `task_name::Context`;
- `cx.local.<resource>` access;
- `cx.shared.<resource>.lock(...)` access;
- `cx.spawn.<alias>(...)` calls;
- task input types and spawn result behavior;
- typed task configuration access;
- synchronous hardware-task and asynchronous software-task shapes;
- monotonic APIs used by tasks.

`cx.spawn.<alias>(...)` is a FerroForge composition API. After binding an alias
to a selected task instance, the renderer must translate it to that instance's
real RTIC call, for example `telemetry::spawn(value)`. Task context lifetimes,
input types, and spawn result types must follow the supported RTIC version.
See the [RTIC task and spawn documentation](https://rtic.rs/2/book/en/by-example/software_tasks.html).

Mocks may be zero-sized, panic if executed, or otherwise have no firmware
runtime behavior. Their signatures and types must nevertheless match the real
RTIC-facing behavior closely enough that successfully checked source can be
transplanted without changing its meaning.

Task implementations should not know the final board, concrete pin mapping,
task instance name, interrupt allocation, priority, or final RTIC application.
Those decisions belong to the system and its app composition.

### Independent Task Contract (Open)

The contract must provide enough information to check the task before a system
selects it. A resource name such as `led` or a spawn alias such as `report`
does not specify a Rust type or callable signature by itself.

The contract needs to describe:

- local and shared resource types or required trait bounds;
- typed configuration keys;
- task inputs and the expected argument/result types of spawn aliases;
- the clock or monotonic interface used by the task.

For a HAL-specific task, concrete HAL types may be appropriate. For a portable
task, Rust must check the body against its declared bounds; checking with one
convenient concrete HAL resource alone does not establish portability. If the
check uses generic context or function syntax, its conversion into a concrete
RTIC handler must be specified too.

The syntax and mock expansion are pending walkthrough item 1 in the plan.
The current prototype instead obtains resource type bindings, configuration
constants, and spawn methods from a surrounding `app!` in the same crate.
That mechanism alone does not make a task independently compilable.

## System-Owned Initialization

Initialization follows the same source-transplant lifecycle as tasks, but it
belongs to one system rather than being generally reusable across systems.

The system init module owns operations such as:

- clock and power configuration;
- peripheral acquisition and configuration;
- pin mapping;
- monotonic startup;
- construction of shared and local resources;
- enabling interrupts;
- initial task spawning.

The developer writes an RTIC-shaped init function using a mock init attribute
and context. `#[ferroforge::init]` below is proposed syntax, not an implemented
standalone attribute. Today the prototype interprets `#[init]` inside `app!`.
The body is illustrative pseudocode:

```rust
#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    let mut rcc = cx.device.RCC.freeze(/* system clock configuration */);
    Mono::start(/* monotonic input clock */);

    let led = /* configure a real HAL pin */;
    blink::spawn().unwrap();

    (
        Shared { /* shared resources */ },
        Local { led, /* local resources */ },
    )
}
```

The mock init environment must provide the compile-time-facing APIs used by
the source, including:

- an `init::Context` with correctly typed `device` and `core` fields;
- the actual PAC and Cortex-M peripheral types for the selected target;
- task spawn functions with accurate arguments and result types;
- the selected mock monotonic startup API;
- task configuration referenced during initialization;
- the real system-owned `Shared` and `Local` return types.

The context does not need to be constructed or executed. It exists so Rust can
validate the initialization body against real embedded types before the
renderer copies that body into the generated application.

The independent init check also needs the interfaces for its outgoing task
spawns, configuration references, and monotonic calls. These must come from
the same contracts used to validate composition so that manually duplicated
mock signatures cannot drift from selected tasks. The mechanism for providing
those interfaces is pending walkthrough item 2 in the plan.

The first supported init API must be explicit. The current context provides
`device` and `core`; facilities such as `cx.cs` and init-local storage need to
be implemented if included in that API. See the [RTIC init documentation](https://rtic.rs/2/book/en/by-example/app_init.html).

## App Composition

Each system owns an app composition. It supplies the global knowledge that a
reusable task or init module cannot know independently.

The composition is responsible for:

- selecting task definitions;
- assigning stable task instance names;
- allowing multiple instances of one reusable definition;
- mapping abstract task resources to system resources;
- providing typed configuration values;
- mapping task-local spawn aliases to task instances;
- assigning priorities;
- assigning hardware interrupt bindings;
- selecting dispatchers and a monotonic;
- selecting the target and final dependency versions or features.

Final dependency selections must be compatible with the transplanted source.
Task and init crates still need Cargo dependencies for their independent
checks. A central source of dependency policy or consistency checks should
prevent the check manifests and generated manifest from silently diverging.
Source validated against one HAL version is not automatically validated
against a different version chosen during composition.

Task-definition identity and task-instance identity must remain separate. For
example, one reusable `blink` definition may produce `status_blink` and
`error_blink` instances with different LEDs, periods, priorities, and spawn
bindings.

## Renderer Responsibilities

The renderer runs only on the host. It reads target task and init source; it
does not link those target workspaces into the host executable.

For each selected task instance, the renderer must:

1. Locate the reusable task source using stable package and task identity.
2. Parse the annotated task and its declared contract.
3. Copy the complete task implementation into the generated RTIC module.
4. Replace the mock task attribute with `#[task(...)]` inside the real
   `#[rtic::app(...)]` module.
5. Rename the function and context paths for the selected instance.
6. Map reusable resource names to system resource names.
7. Replace mock configuration paths with generated typed constants.
8. Rewrite task-local spawn aliases to their composed RTIC task targets.
9. Preserve required task inputs and the supporting imports/items allowed by
   the declared source boundary; handle supported macros and conditional
   compilation explicitly.
10. Collect the external Cargo dependencies and features required by selected
    source.

For system initialization, the renderer must similarly:

1. Locate and parse the system init source.
2. Copy the init implementation and its required supporting items.
3. Replace the mock init attribute with the real `#[init]` attribute.
4. Rewrite task instance, configuration, spawn, and monotonic references using
   the app composition.
5. Emit it together with the system's `Shared` and `Local` resource structs.

The result is one complete RTIC module containing the resource declarations,
init function, and every selected task. The renderer generates the global
wiring; it does not replace Rust's or RTIC's type checking.

## Source-Transplant Boundary

Source transplantation must have an explicit boundary. Copying only one
function is insufficient when a task or init implementation depends on
module-level imports, helper functions, local types, constants, macros, or
submodules.

FerroForge therefore needs a defined source-unit contract. Possible forms
include explicitly marked supporting items, a constrained task/init module, or
a manifest that names all source units belonging to the implementation. The
renderer should not attempt to infer an arbitrary Rust module dependency graph
because doing so would duplicate compiler name resolution.

The boundary must define how `crate::`, `super::`, imports, helper visibility,
feature conditions, and references inside macros behave after transplantation.
Copying the text of a helper does not by itself preserve its original scope.

The initial recommendation is an explicitly declared source module with a
documented set of supported reference patterns and clear diagnostics for
unsupported patterns. This is a proposal for walkthrough item 4, not a settled
syntax. Rewrites must preserve literals and unrelated identifiers; arbitrary
string replacement in macro token text cannot provide that guarantee.

The same parser and data model should be shared by the mock procedural macros
and the renderer so that both sides interpret task and init declarations
identically.

## Target Separation

Target-specific task and init crates must never become ordinary dependencies
of the host renderer. There are two distinct dependency graphs:

```text
x86_64 host graph:
  app composer -> renderer -> FerroForge schema and host utilities

embedded target graph:
  generated application -> final HAL, RTIC, runtime, and task dependencies
```

The renderer may use Cargo metadata and source manifests to discover embedded
packages. Reading package information or source on the host does not require
compiling that package for the host.

## Current Prototype

The current `embedded` workspace demonstrates the basic lifecycle in a single
package:

- `src/tasks.rs` contains RTIC-shaped task bodies checked through FerroForge
  mock contexts.
- `src/lib.rs` contains real HAL imports, resources, init code, target data,
  and check-time task configuration.
- `composer` applies host-owned scheduling and configuration.
- `ferroforge-renderer` extracts the source and writes the standalone project
  under `generated/nucleo-f401re`.

The prototype successfully checks and builds the generated STM32F401RE RTIC
firmware. It still needs to generalize source discovery to separate task and
system workspaces, define the supporting-source boundary, distinguish task
definitions from instances, support resource renaming, and apply spawn
bindings during rendering.

On 2026-09-05, the review reran 23 macro tests and 7 renderer/loader tests, the
ARM source library check, and the existing generated firmware release build;
all passed. These checks cover the existing prototype, not the proposed
independent task/init workspace architecture. The composer currently renders
the project; checking and building it are separate commands.

## Core Design Principle

Rust remains the source of truth for task behavior, initialization behavior,
types, and target API usage. Generation is limited to the pieces requiring
whole-application knowledge:

- selecting and instantiating reusable tasks;
- renaming and binding resources;
- resolving configuration and spawn aliases;
- assigning RTIC scheduling and interrupt properties;
- assembling the final `#[rtic::app]` module;
- producing the final target manifest and build configuration.

Both reusable tasks and system initialization follow the same lifecycle:

```text
mock RTIC source
    -> target-appropriate Rust check
    -> source transplantation
    -> composition-driven rewrites
    -> real RTIC validation
    -> final embedded build
```
