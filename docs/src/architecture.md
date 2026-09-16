# Source-Transplant Architecture

The [governing requirements](governing-requirements.md) take precedence over
earlier design wording in this chapter. This chapter explains the architecture
and its implementation status under those requirements. Per G3 and G4, the
typed interrupt enum is required and hardware tasks carry the interrupt
binding; software tasks do not.

**Size note:** this is the largest chapter in the book and is a candidate for
splitting once documentation consolidation is complete. Until then, add new
material to the section it belongs to rather than starting a parallel chapter,
so the eventual split follows real seams.

This document records the agreed direction: complete reusable task bodies and
system-owned init bodies are checked through mock RTIC interfaces and
transplanted into a real RTIC application. The unresolved implementation
decisions are tracked in the [review and open decisions](review.md).
The initial resource-keyed bounds, task input/spawn syntax, minimal SysTick
profile, generic checking expansion, complete check/generate/build pipeline,
and same-board cross-system reuse are implemented. General frontend ergonomics,
remaining contract details, additional target profiles, and broader target
portability remain work.
Shared declaration parsing, Cargo-aware source discovery, and structural
standalone composition validation now have an
[implemented foundation](prototype.md#standalone-discovery-foundation). That
foundation now includes independent task/init checks, a bounded combined
real-RTIC ARM transplant, and two explicit-Rust system frontends under
`systems/`; they remain separate from the legacy composer pipeline.

## Purpose

FerroForge enables developers to write complete RTIC-style tasks as reusable
Rust source, type-check that source before generation, and then transplant the
task into a generated, real RTIC application.

Reusable tasks are not merely libraries containing functions that generated
RTIC wrappers call. The reusable unit is the complete task declaration and
body, written with the same programming model used inside an RTIC app:

The following illustrates a complete blinking task in its source module.
Resource-keyed bounds, inline concrete resources, the mock-clock import, and
`monotonic` option are implemented for standalone checking and the bounded
transplant pipeline. `tasks/blinky` exercises this contract. This example assumes
the agreed 1 kHz, 32-bit SysTick profile for checking and generated firmware.

```rust,ignore
#![no_std]

pub mod indicators {
    use embedded_hal::digital::StatefulOutputPin;
    use ferroforge::{mock::systick::Mono, task};
    use fugit::ExtU32 as _;

    #[task(
        bounds = [led: StatefulOutputPin],
        local = [led, toggle_count: u32],
        shared = [enabled: bool],
        config = [period_ms: u32],
        monotonic = Mono,
    )]
    pub async fn blink(mut cx: blink::Context) -> ! {
        loop {
            let enabled = cx.shared.enabled.lock(|value| *value);

            if enabled {
                StatefulOutputPin::toggle(&mut *cx.local.led).unwrap();
                *cx.local.toggle_count =
                    (*cx.local.toggle_count).wrapping_add(1);
            }

            Mono::delay(CONFIG.PERIOD_MS.millis()).await;
        }
    }
}
```

System init constructs the real output pin and resource values, for example
`toggle_count = 0` and `enabled = true`; composition maps those resources and
supplies an immutable `period_ms` value such as `500`. Init also starts the
real monotonic and spawns the selected blink instance. The example's delay is
between toggles, so it is a half-cycle delay while enabled. Setting `enabled`
to false pauses toggling and leaves the pin at its current level; the next
iteration samples the flag again. The lock is released before the delay.
`unwrap()` makes pin errors panic in this example, while the counter explicitly
wraps at its maximum value. These are example behavior choices, not framework
policies.

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

Each firmware has its own source and generated-output boundary. The current
checkout uses this prototype layout:

```text
systems/
`-- <system>/
    |-- init/
    |-- app_composition/
    `-- gen_app/
```

The required target layout is fixed by G5 in the
[governing requirements](governing-requirements.md) and is not restated here.
Composition and init share one target-checked authored package; shared host
tooling performs generation, and reusable common and chip-family task crates
live outside individual firmware applications. This layout remains to be
implemented; detailed Cargo plumbing and checking-interface refresh remain open.

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

The intended pipeline performs validation at more than one stage. Composition
must supply the checking interfaces before system init can be checked:

1. **Reusable task check**
   Rust checks each task body against its mock RTIC context and its portable or
   HAL-specific types.
2. **Composition validation and init-interface generation**
   The host composer resolves selected tasks, instances, resources,
   configuration, spawn aliases, priorities, interrupts, and dispatchers.
   Validate the supported declarations, dependency requirements, and clock
   profile, then generate init's checking interfaces from those contracts and
   the composition. This is structural validation, not a proof of concrete
   Rust type compatibility.
3. **System initialization check**
   With those interfaces available, Rust checks the system's clock, peripheral,
   pin, resource, monotonic, and initial-spawn code against the real target HAL.
4. **Firmware generation**
   Transplant the checked task/init source and supporting code, apply the
   composition bindings, and emit the real RTIC project and manifest.
5. **Generated firmware check**
   The real RTIC macro checks the complete application and all global RTIC
   constraints.
6. **Final target build**
   Cargo compiles and links the generated firmware for the selected target.

Independent source checks validate each body against its own declared
contract. They do not prove that every concrete resource mapping or spawn
binding in a system is compatible. Rust must check those bindings in the
generated application; whether to add a target check harness for concrete
composition before final firmware generation can be considered later and is
not an initial prerequisite.

The host composer can check declared names, completeness, and structural
constraints. Comparing the spelling of Rust types is not a substitute for
Rust checking their compatibility. Checked and generated source must also use
compatible dependency versions, target settings, and features.

The mock layer does not replace RTIC's final validation. Its purpose is to
provide useful Rust and Rust Analyzer feedback while developers edit reusable
task and system initialization source. Compile-only mocks do not supply a host
simulator; executable simulation is a separate optional objective.

### Incremental Coverage and Early Checks

The initial implementation is deliberately use-driven, not exhaustive. Support
the task, resource, and monotonic operations needed by the first real examples,
then extend coverage as additional uses appear. In particular, begin with the
SysTick delay operation already used by the blinking task; a complete monotonic
API is not required before the first pipeline works.

Add low-cost early checks where they give useful feedback: declared resource
operations, configuration types, known spawn signatures, missing bindings,
and duplicate names. Preserve correct types for supported operations without
trying to reproduce every Rust or RTIC constraint in the mock layer.

Some mistakes may first be caught by Rust Analyzer in the generated project
or by its Rust/RTIC compiler checks. That is acceptable for this stage; document
the limits rather than claim the source check proves the final application.
Rust Analyzer is development feedback, while target-aware `cargo check` and
the final build remain required verification. Expand early validation when a
concrete error pattern has a straightforward solution.

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
- the task function shapes supported by RTIC (the current code distinguishes
  synchronous hardware tasks and asynchronous software tasks);
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

### Independent Task Contract

The initial resource, configuration, input/spawn, and SysTick directions are
agreed. The remaining grammar details and implementation proofs are tracked
in [software task clarifications](architecture.md#software-task-clarifications).

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
convenient concrete HAL resource alone does not establish portability. The
agreed checking expansion introduces generics internally; the renderer uses
the original task source to emit a concrete RTIC handler. Both paths must be
verified together.

Implement and verify that agreed contract; do not treat the completed initial
walkthrough as a prerequisite discussion still waiting to happen.
The current prototype instead obtains resource type bindings, configuration
constants, and spawn methods from a surrounding `app!` in the same crate.
That mechanism alone does not make a task independently compilable.

### Software Task Clarifications

The initial standalone implementation focuses on software (SW) tasks. Common
reusable crates hold SW definitions using portable interfaces such as
`embedded-hal`; chip-family crates hold hardware (HW) task definitions and
helpers using that family's APIs, such as `stm32f4xx-hal`. This ownership and a
common RTIC-like task model are required. Family implementation and the precise
interrupt assignment remain work under the
[governing requirements](governing-requirements.md).
The SW task's final resource can still be a chip-HAL type constructed by firmware
init; the reusable SW source does not name that concrete type.

For a blinking software task, the requirements can use existing Rust types
and traits:

| Requirement | Independent task interface | System/composition responsibility |
| --- | --- | --- |
| LED | `embedded_hal::digital::StatefulOutputPin` bound | Construct a real HAL pin and bind it to `led` |
| Period | Ordinary `u32`, with milliseconds as the declared unit | Supply a value such as `500` |
| Shared enable flag, if used | Ordinary `bool` behind an RTIC-like lock proxy | Construct and bind the shared resource |
| Clock | Mock matching the supported SysTick monotonic API | Select a compatible real monotonic configuration and initialize it |

`embedded-hal` provides the real interface, not a concrete pin instance. In
embedded-hal 1.0, `StatefulOutputPin` supplies `toggle()` and its
`Result<(), Self::Error>` return type. No custom FerroForge LED trait is
needed. The check must enforce the trait's operations and associated error
type, rather than checking only one convenient dummy or chip-specific pin.
See the [StatefulOutputPin documentation](https://docs.rs/embedded-hal/latest/embedded_hal/digital/trait.StatefulOutputPin.html).

Likewise, `period_ms: u32` needs no mock integer or custom configuration value
type. The macro provides typed access to the configuration; composition owns
the actual value. Conversion to a duration must preserve milliseconds and
match the selected monotonic types. The existing prototype's `u64` period is
an example choice, not an architectural restriction.

The selected form keys `bounds` by resource name: `bounds = [led:
StatefulOutputPin]` declares the required interface, and `local = [led]`
declares its access category. There is no separate `Led` type name and no
`impl` marker in the task declaration. Concrete resources carry their type
inline, for example `local = [led, toggle_count: u32]` and
`shared = [enabled: bool]`.

This is an explicit declaration convention, not type inference by the macro.
Bound entries identify trait requirements; typed local/shared entries identify
concrete types. The macro generates the generic checking types internally,
retaining an ordinary `task_name::Context` in the authored task signature.
A bound does not imply local or shared access: its resource must still be
claimed in the appropriate list. The renderer emits a concrete RTIC handler.
This syntax is implemented for the initial standalone checking cases, and the
early renderer transplants direct resource, configuration, spawn, clock, and
bounded logging uses. The generated init checker supplies composition-derived
spawn/startup interfaces, and the first Phase 4 path transplants that checked
init into the same real-RTIC app. Dependency-manifest emission and the initial
explicit-Rust system frontend are implemented; generalized frontend syntax and
legacy migration remain open.

Resource requirements should remain inline with each reusable task. Do not
introduce a separate `Local`/`Shared` requirement-struct section in task modules
as the default authoring model; that suggestion was not selected. Familiar
declaration syntax is the goal, with resource-keyed bounds providing the
distinction between trait requirements and inline concrete types.
This does not change the real system-owned resource structs used by init.

### Local and Shared Resources in the Initial SW Scope

Both resource categories are required from the first SW implementation. Keep
RTIC-familiar `local = [...]` and `shared = [...]` claims: local resources are
accessed through `cx.local.<name>`, while mutable shared access uses
`cx.shared.<name>.lock(...)`. These are actual application resources, not
function-local variables recreated on every invocation. A resource may hold
ordinary state such as a counter or flag, or an embedded peripheral/driver.

For each selected task, the renderer must retain the resource name, local or
shared category, and composition binding. Emit the corresponding real RTIC
claims and retain the intended context-access paths when resources are renamed.
System-owned resource construction/initial values remain part of init.
Task-owned inline resource initialization is a separate syntax detail, not
selected by this decision.

Complete resource validation in FerroForge is not required initially. Prefer
inexpensive checks such as missing bindings, duplicate claims, and category
mismatches that can be established from declarations. Leave remaining concrete
type, lifetime, ownership, and locking errors to the generated Rust/RTIC checks.
Still provide useful typed access for the supported mock operations; partial
validation does not mean the renderer can discard the local/shared distinction.

The categories, access style, and initial resource-keyed bounds convention are
agreed and implemented for the bounded standalone path. Its generated ARM checks
exercise concrete resource bindings; independent mock checks alone do not prove
all composition type compatibility. More elaborate resource forms remain work.

### Task-Local Configuration Access

The agreed task-body spelling is `CONFIG.PERIOD_MS` for a declaration such
as `config = [period_ms: u32]`. It avoids embedding the task's definition name
in ordinary configuration reads. Configuration is immutable per composed task
instance; it is not an RTIC local/shared resource requiring a runtime lock.

The agreed checking mechanism is a macro-generated, task-local immutable
`CONFIG` view with correctly typed fields. The developer does not declare or
import that binding. Separate tasks in one module can each use `CONFIG` while
retaining distinct configuration types and values. Generated boilerplate must
handle the uppercase field naming convention without imposing lint allowances
on the developer's code. Treat the binding as reserved in its supported scope
and diagnose conflicting declarations rather than silently rewrite a shadowed
user variable. The standalone macro implements the immutable typed view inside
the generic checking function and diagnoses conflicting `CONFIG` declarations.

Generate the checking interface from the task's `config = [...]` declarations,
without requiring a system composition or its actual values. For example,
Rust knows that `CONFIG.PERIOD_MS` is a `u32` and can check its supported uses
before a system supplies the period. This validates field names, types, and
usage, not value-dependent conditions such as requiring a nonzero period.
Such validation needs the actual composition values and any applicable rule;
it is not automatically proved by either the mock view or ordinary type checks.

The renderer resolves direct `CONFIG.<FIELD>` reads against the current task
instance and preserves the declared type. The selected output is a reference
to a named, typed constant, not a hardcoded literal in the task body. For
example, `Mono::delay(CONFIG.PERIOD_MS.millis()).await` becomes
`Mono::delay(__ferroforge_config::status_blink::PERIOD_MS.millis()).await` for
the `status_blink` instance. Preserve source scope; arbitrary text replacement
is insufficient.
A runtime `CONFIG` object need not survive generation.

Collect all composed task-configuration constants in one clearly marked
namespace at the start of the generated RTIC app, before `Shared`, `Local`, and
`init`. Group the constants by task instance so their qualified paths provide
an overview and distinguish separate uses of one task definition:

```rust,ignore
mod __ferroforge_config {
    pub(super) mod status_blink {
        pub(crate) const PERIOD_MS: u32 = 500;
    }
    pub(super) mod alarm_blink {
        pub(crate) const PERIOD_MS: u32 = 100;
    }
}
```

Composition remains the source of truth; this namespace is a generated view of
resolved values, not a second hand-maintained configuration source. Keep it
inline initially, with no separate prelude/import mechanism required. A
generated `config.rs` can be considered later if the section grows. Named Rust
constants retain compile-time values without requiring a runtime configuration
object. Mutable local/shared resource contents remain resources, not
configuration constants. This grouped namespace and its per-instance rewrite
are implemented by the standalone transplant.

Initial support is limited to direct field reads in task bodies. Using
configuration in array lengths, const-generic arguments, or other positions
requiring constant expressions is separate work, even though final generated
references resolve to constants. Passing the whole view to helpers, changing
it, and cross-task/init access are not implied by this decision and can be
designed when needed.
The legacy [current renderer](prototype.md#renderer) recognizes qualified
`task::Config::FIELD` paths. The standalone transplant now recognizes direct
`CONFIG.FIELD` expressions and emits collision-safe, typed constants per task
instance before the resource structs and init. Two instances of one definition
with different values compile in the renderer-owned ARM fixture. The generated
section uses the grouped namespace shown above.
The initial supported native logging argument forms now use the same direct
configuration rewrite.

### Task Inputs and Spawning

The agreed starting point keeps incoming inputs as ordinary Rust parameters
after the task context. There is no separate `inputs = [...]` declaration.
Inputs are values supplied for each invocation, unlike persistent local/shared
resources or the fixed configuration supplied by composition.

Declare outgoing spawn aliases inline, including their expected arguments:
`spawn = [report(value: u32)]`. The caller declares an interface, not a dependency
on a destination implementation. The macro generates a typed mock method for
`cx.spawn.report(value)` from that declaration, allowing the caller to check
independently. The receiving task's function defines its own incoming signature;
composition must connect compatible signatures.

Selected starting syntax, implemented for standalone task checking and translated
into generated RTIC by the standalone transplant:

```rust,ignore
pub mod telemetry {
    use ferroforge::task;

    #[task(shared = [last_value: u32])]
    pub async fn record(mut cx: record::Context, value: u32) {
        cx.shared.last_value.lock(|last| *last = value);
    }

    #[task(
        local = [dropped: u32],
        spawn = [report(value: u32)],
    )]
    pub async fn forward(cx: forward::Context, value: u32) {
        if cx.spawn.report(value).is_err() {
            *cx.local.dropped =
                (*cx.local.dropped).wrapping_add(1);
        }
    }
}
```

System init constructs `last_value` and `dropped`; composition binds them.
This example deliberately drops a value and counts the failure if the receiver
cannot be spawned. That error policy belongs to the task, not the framework.

If composition connects `forward`'s `report` alias to an instance of `record`
named `main_telemetry`, the renderer rewrites `cx.spawn.report(value)` to
`main_telemetry::spawn(value)`. Preserve the arguments and surrounding error
handling. The outgoing declaration and mock handle do not enter firmware.
`cx.spawn` is FerroForge authoring syntax, not a real RTIC 2 context field.

Match RTIC 2's spawn result automatically from the declared inputs:

- No inputs: `Result<(), ()>`.
- One input of type `T`: `Result<(), T>`.
- Multiple inputs: `Result<(), (T1, T2, ...)>`.

An unsuccessful spawn returns the unaccepted input values, not a custom
queue-full error. Authors do not repeat this result type in the alias
declaration. See RTIC's [spawn generation](https://rtic.rs/2/api/src/rtic_macros/codegen/module.rs.html)
and [input grouping](https://rtic.rs/2/api/src/rtic_macros/codegen/util.rs.html).

Spawning is not message delivery to an already-running task. RTIC 2 rejects
spawning an instance whose previous invocation is pending or still running,
including when suspended at an await. A forever-running blink instance is
spawned once; its shared flag can control subsequent behavior. No scheduler
simulation is required in the compile-only mock. See the
[RTIC task and spawn examples](https://rtic.rs/2/book/en/by-example/software_tasks.html).

Initial validation should check calls against declared mock signatures and
reject missing/unknown aliases, missing targets, and obvious argument-count
mismatches. Full Rust type compatibility is not a string comparison in the
renderer; remaining binding, ownership, and lifetime errors may surface in
the generated Rust/RTIC checks. Add inexpensive earlier checks as useful cases
arise, and retain the final target check/build.

The macro now has two paths. Legacy names-only declarations derive outgoing
signatures from the surrounding app's target tasks and return
`Result<(), ferroforge::SpawnError>` with `QueueFull`. Typed standalone
declarations independently generate accurate zero/one/multiple-input result
types. The legacy app-backed renderer stores spawn bindings without rewriting
calls. The standalone transplant now rewrites direct `cx.spawn.<alias>(...)`
calls to the bound instance's RTIC `spawn` function while retaining arguments
and result handling; zero/one/multiple-input shapes pass its ARM fixture.
Independent init now receives selected-instance spawn interfaces from the
validated composition. Their initial argument and RTIC-compatible failure types
come from the task contracts; the bounded real-RTIC init transplant now retains
the same direct spawn calls.

### SysTick Mock Direction

The agreed initial profile is Cortex-M SysTick using the core clock, with a
1_000 Hz tick rate and `u32`-backed time values. One tick represents one
millisecond. This is the monotonic's software time representation, not a claim
about the hardware counter width. The real backend generates an interrupt
each tick; at this profile that means 1,000 interrupts per second.

Support only the operations required by the first task and system init:

- Task delay: `Mono::delay(duration).await`, with a real
  `fugit::Duration<u32, 1, 1_000>` argument and unit output.
- System startup: `Mono::start(systick, core_clock_hz)`, accepting the actual
  `cortex_m::peripheral::SYST` by value and a `u32` clock frequency.

Keep `period_ms` as `u32` and use real `fugit::ExtU32` conversions, so the
task retains `Mono::delay(CONFIG.PERIOD_MS.millis()).await`. Mock the clock
interface, not the duration type or conversion. The standalone task fixture now
imports `ferroforge::mock::systick::Mono`, and its compile-only `delay` signature
checks the agreed duration. The standalone transplant now emits the real
1 kHz/`u32` SysTick backend and rewrites direct declared delay calls. Its ARM
fixture starts that backend with `cx.core.SYST` and a `u32` core frequency from
the initial parsed-shell proof. The independent init checker exposes the
matching `start(SYST, u32)` signature against real HAL clock setup, and the
combined transplant now rewrites that checked init call to the generated real
monotonic.

In generated firmware, resolve the task's clock reference to the real SysTick
monotonic using the same 1 kHz/32-bit profile. System init calls
`Mono::start(cx.core.SYST, core_clock_hz)` with the frequency obtained from its
HAL clock setup. This input is the core clock frequency, not the 1,000 Hz tick
rate; systems with different core clock frequencies can use the same profile.
The init-side mock checks the real peripheral argument in the target workspace.
Portable SW task checks do not need to construct or access hardware. See the
[SysTick implementation and API examples](https://docs.rs/rtic-monotonics/latest/src/rtic_monotonics/systick.rs.html).

The mock is compile-only and may panic if executed. Do not sleep a host thread,
simulate interrupts, or implement a scheduler. Initially omit `now()`,
`delay_until()`, timeouts, alternative clock sources, and 64-bit profiles;
extend coverage when tasks need them. The real SysTick prelude uses `ExtU32`
for this profile; enabling `systick-64bit` would change its time representation
and is outside the initial supported combination.

Rust checks delay and startup argument types. Composition must reject a
selected clock profile that does not match the mock's rate and representation,
including unsupported combinations. This does not prove the runtime clock
frequency is correct or that init calls startup exactly once; the mock is not
a hardware execution test. Retain the generated target check/build and add
inexpensive earlier checks as useful cases arise.

The legacy app-backed mock is not this faithful SysTick interface. Its fixed
`MillisDurationU64` delay and one-argument `start(clock_hz)` remain for the
existing STM32-timer example. The new standalone task mock covers only the
agreed SysTick delay; startup is intentionally deferred to init checking.

### Modules Group Related Tasks

A source module groups related reusable tasks. Developers place common
`use` declarations at the top of that module, including external traits,
duration extensions, task attributes, and any shared mock-clock import.
Task functions in the module use those names without repeating the imports
inside every function. A task crate can contain multiple such modules;
one task does not require its own crate. Cargo dependencies belong in the
containing crate's manifest.

The group is an ordinary logical Rust module, not necessarily an inline
`mod { ... }` block. For example, `src/lib.rs` can declare
`pub mod indicators;`, with imports, helpers, and annotated task functions
directly at the top level of `src/indicators.rs`. An inline module also works
as an authoring form, and `src/lib.rs` itself can be the group when no further
split is needed. No additional FerroForge module wrapper or grouping annotation
is required. Discovery supports these forms; the standalone transplant's ARM
fixtures prove the initial generated namespace and private-helper access layout.

Each task still has its own resource/configuration requirements and generated
context. Grouping tasks does not merge their RTIC resource ownership or make
a local resource shareable between handlers. Composition continues to select
and instantiate individual task definitions.

The renderer must retain the declaring module's import environment alongside
each selected task, preserve aliases and scope, and prevent collisions when
tasks from different modules are composed. Copying only the function loses
this environment; merging all imports into one global scope is insufficient.
Ordinary supporting items move together at the logical-module boundary while
tasks remain individually selectable. See the agreed
[source-transplant boundary](#source-transplant-boundary) for the proven initial
namespace/access layout and the remaining reference-scope limitations.

For a clock used by several tasks, the implemented checking spelling is a
module-level `ferroforge::mock::systick::Mono` import independent of any one
task's generated context module, with `monotonic = Mono` on tasks that use it.
The standalone transplant resolves the supported references to its real
monotonic and removes the direct `ferroforge` mock import.

### Generated Checking Contexts

The task developer writes the resource/configuration requirements and the
complete RTIC-shaped function body, including resource access, control flow,
and asynchronous waits. The developer should not also have to handwrite a
`task_name::Context` struct, its local/shared resource views, or repeated mock
configuration and clock bindings for each task.

FerroForge owns reusable mock support. Its procedural macro generates each
task's particular checking context and connects it to the declared real types,
trait bounds, and shared mock APIs. The agreed starting mechanism is to replace
the annotated function with a generic checking function during macro expansion.
The author retains the plain `task_name::Context` signature in source; the
macro introduces resource type parameters, lifetimes, and the corresponding
context arguments in its output. It keeps the complete task body and its
declaring import environment.

For a returning task declaring `bounds = [led: StatefulOutputPin]` and
`local = [led, toggle_count: u32]`, the expansion can look like this inside
the original module (with its module-level trait import still in scope):

```rust,ignore
pub mod toggle_led {
    pub struct Local<'a, Led> {
        pub led: &'a mut Led,
        pub toggle_count: &'a mut u32,
    }

    pub struct Context<'a, Led> {
        pub local: Local<'a, Led>,
    }
}

pub async fn toggle_led<'a, Led>(cx: toggle_led::Context<'a, Led>)
where
    Led: StatefulOutputPin,
{
    StatefulOutputPin::toggle(&mut *cx.local.led).unwrap();
    *cx.local.toggle_count = (*cx.local.toggle_count).wrapping_add(1);
}
```

`Led` and `'a` are generated implementation details, not handwritten per-task
boilerplate. Rust checks the generic body against the real trait and its
supertraits without selecting a concrete pin implementation. Concrete resource
entries keep their actual types. Use ordinary mutable references for local
access so Rust performs borrowing checks. Attribute macros can replace the
annotated item with this checking representation; see the
[Rust macro reference](https://doc.rust-lang.org/reference/procedural-macros.html#the-proc_macro_attribute-attribute)
and [trait bounds](https://doc.rust-lang.org/reference/trait-bounds.html#bound-satisfaction).

Shared resources use a typed borrowing proxy from reusable mock support. For
`shared = [enabled: bool]`, its interface has the shape
`fn lock<R>(&mut self, f: impl FnOnce(&mut bool) -> R) -> R`. The macro supplies
the resource type; the developer keeps `cx.shared.enabled.lock(...)`. This
compile-only proxy may panic instead of executing the closure. It must not
allow a borrowed resource reference to escape the supported lock operation.

The example above covers a returning task. Lifetimes must follow the supported
RTIC task shape; the forever-running blink requires the corresponding lifetime
treatment rather than applying this example indiscriminately. Full RTIC
ownership and scheduling validation remains the generated application's job.

The renderer transplants the task source, not those generated checking
structs or mock implementations. Real RTIC creates the final contexts, using
the system's actual resources and clock. This preserves the complete-task
authoring model without requiring manually maintained per-task mock boilerplate.

The 2026-09-06 isolated compiler experiment using real `embedded-hal` 1.0.0
validated the generic representation without any concrete pin. On 2026-09-08,
the standalone macro and committed package/compile-fail harness carried that
representation into the implementation:

- The valid local/shared resource body compiled.
- An undeclared pin method failed to compile.
- Assigning `bool` to the `u32` local counter failed to compile.
- Returning a shared-resource reference out of the lock closure failed to compile.

These cases now run through the implemented attribute macro and committed
regression suite, which asserts the authored source line for every focused
compiler failure. On 2026-09-10, an opt-in Rust Analyzer 1.98.0 scan reported
the invalid method and concrete value errors at their authored task-body lines.
It includes the generated generic name in the method message and does not emit
the shared-lock higher-ranked lifetime error, which remains covered by rustc.
The early concrete RTIC task transplant also has ARM coverage. Configuration,
spawn, delay, and bounded logging translations are integrated, and the first
Nucleo pipeline now orchestrates the separated Cargo checks and final build.
Real negative cases cover each required Phase 5 failure class. Cross-system
reuse is proved for two same-board profiles; generalized orchestration and
additional targets remain open.

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

Hardware initialization uses the selected HAL's native types, traits, and
methods, together with actual PAC/Cortex-M peripheral types. FerroForge does
not introduce replacement hardware traits or a HAL wrapper API for clock,
GPIO, or peripheral construction. The mock app context exposes real peripheral
types; it does not substitute fake HAL objects. Init is checked for its embedded
target, without executing hardware initialization. The current prototype uses
an independent init package; the required firmware layout checks init in the
same authored package as composition.

The developer writes an RTIC-shaped init function using a check-only init
attribute and context. `#[ferroforge::init]` is now implemented as the bounded
standalone signature/source marker; the legacy prototype still interprets
`#[init]` inside `app!`. The body below illustrates the implemented shape:

```rust,ignore
#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    let mut rcc = cx.device.RCC.freeze(/* system clock configuration */);
    Mono::start(cx.core.SYST, /* core clock frequency from HAL */);

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

The agreed mechanism is to generate init's check-only app interfaces from
system composition and the selected task contracts. These provide the mock
`init::Context`, spawn entry points named for the selected task instances with
matching arguments/results, and the selected monotonic startup interface.
Configuration interfaces are included if init needs them; their initial
author-facing syntax remains to be specified. Developers do not maintain
duplicate task signatures in the init module.

The host generator reads declarations as source and metadata, emits the
checking interfaces, and then the target init workspace checks its native HAL
body against them. It does not link ARM task or init implementations into the
host. The initial implementation writes the generated context, spawn, and
SysTick interface plus a Cargo configuration to an explicitly selected output
directory, then checks the original no-std init package and authored manifest.
The check-only `ferroforge` dependency remains available for that source check
and is excluded only from later firmware output. Composition is the source of
truth for app-specific bindings, while the init source owns native hardware
setup and the actual `Shared`/`Local` resource construction.

The renderer transplants the complete init body and its declared supporting
source. Native HAL calls stay intact, with only the supported composition-driven
app-interface rewrites. Check-only interfaces do not enter the firmware; real
RTIC contexts, task spawn functions, and monotonics supply the final equivalents.
The generated checker mechanism is implemented and ARM-checked for the initial
STM32F401 fixture. Regenerating after deselecting an instance removes its spawn
module, so an unchanged stale init call fails instead of using a cached
interface. The init attribute parses the generated interface into its expansion
instead of emitting a nested `include!`; an opt-in Rust Analyzer 1.98.0 check
therefore reports focused errors on the authored init lines. Source-adjacent
interface/config output gives Cargo and the editor the same regenerated view.
The bounded Phase 4 renderer now consumes the same discovered init package,
retains its root support and resource structs/body, removes the checking-only
marker, and ARM-checks the combined real-RTIC output.

The first supported init API is deliberately narrow: a qualified crate-root
marker, self-contained root support, real STM32F401 PAC and Cortex-M peripheral
contexts, selected-instance spawn functions, `(Shared, Local)`, and the initial
1 kHz/u32 SysTick startup. Configuration access, `cx.cs`, init-local storage,
child support modules, and other targets remain outside this slice. See the
[RTIC init documentation](https://rtic.rs/2/book/en/by-example/app_init.html).

## App Composition

The agreed term is now **firmware**, replacing "system" in the new design.
One `firmware/` grouping directory contains all firmware targets; it is not a
compiled Cargo crate. Each target has its own workspace containing app composition
and handwritten native hardware init, with Rust Analyzer checking of authored code
as a requirement. Per G5 the authored package holds one `composition!`
declaration carrying target selection, task wiring, and the handwritten init
with its `Shared` and `Local` resources; file layout within that package is the
author's choice. Generation uses shared host tooling, and `gen_app` is an
excluded, generated standalone build package. Checking-interface
refresh and detailed Cargo plumbing remain open; current `systems/` paths still
describe the checkout. See the
[agreed logical hierarchy](composition-repair-plan.md#family-backends-and-firmware-workspaces).

A family backend crate, such as STM32F4, owns its concrete chip target definitions
and contains that family's reusable hardware tasks and helpers (functions,
structs, and supporting code). Common reusable crates contain software tasks.
Firmware selects the chip definition from the backend rather than maintaining
another copy. Both task kinds use the same RTIC-like definition and composition
model. Per G4, interrupt selection uses a typed enum per chip owned by that
chip's platform backend, so an interrupt the selected chip does not have is a
type error in the authored package. Per G3, hardware tasks carry the interrupt
binding and software tasks do not.
Enum representation and helper sharing details remain open. Concrete target
availability and helper type identity must be preserved through checking and
transplantation.

The required authoring interface is a per-firmware `composition!`, accompanied
by that firmware's native hardware init. Each firmware selects one unified target
definition; checking, init-interface generation, rendering, and building consume
the same resolved target. Application scheduling/bindings and executable hardware
setup have distinct ownership from target facts. See the
[repair contract and ownership table](composition-repair-plan.md#unified-target-contract).
This is an agreed direction, not current standalone behavior. The firmware
hierarchy and single authored package are required; target representation and
detailed Cargo inheritance remain open.

Each firmware owns an app composition. It supplies the global knowledge that a
reusable task or init module cannot know independently.

The composition is responsible for:

- selecting task definitions;
- assigning stable task instance names;
- allowing multiple instances of one reusable definition;
- mapping abstract task resources to system resources;
- providing typed configuration values;
- mapping task-local spawn aliases to task instances;
- assigning priorities;
- assigning interrupt bindings according to the clarified task-kind requirement;
- selecting dispatchers and a monotonic;
- selecting the target and final dependency versions or features.

Final dependency selections must be compatible with the transplanted source.
[Dependency management](dependencies.md) owns that policy: Cargo manifests as
the source of requirements, conservative inclusion, the
[check-only setting](dependencies.md#agreed-check-only-dependency-setting), and
the [merge and conflict rules](dependencies.md#agreed-initial-merging-and-conflict-policy).

The architectural constraint is narrower: source validated against one HAL
version is not automatically validated against a different version chosen
during composition, and matching requirements do not prove identical resolved
graphs or valid feature combinations.

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
7. Rewrite task-local configuration reads to named, typed constants in one
   generated configuration section grouped by composed task instance.
8. Rewrite task-local spawn aliases to their composed RTIC task targets.
9. Preserve required task inputs and the supporting imports/items allowed by
   the declared source boundary; handle supported macros and conditional
   compilation explicitly.
10. Read dependency requirements from participating source crates' Cargo
    manifests, retaining applicable normal dependencies conservatively rather
    than inferring only what selected task bodies use. Apply the explicit
    agreed check-only exclusion setting and conservative dependency merge policy.

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

### Native RTT and defmt Logging

Ordinary RTT/defmt logging syntax is required in the initial supported source
surface, not deferred merely because it uses macro arguments. Developers use
the real logging crates and macros, with imports at module scope; no FerroForge
logging wrapper or replacement formatting traits are required. Typical calls
must remain natural in reusable task bodies:

```rust,ignore
defmt::info!("period={=u32} ms", CONFIG.PERIOD_MS);
rtt_target::rprintln!("toggles={}", *cx.local.toggle_count);
```

The standalone transplant now supports the ordinary logging macro families,
including
defmt's level macros and `println!`, and rtt-target's `rprint!`/`rprintln!`.
Preserve the native format strings, format hints, macro paths, delimiters, and
argument structure. Apply the supported configuration/resource/spawn/clock
rewrites to argument expressions in place, including references such as
`CONFIG.PERIOD_MS`. Preserve ordinary qualified calls and module-imported macro
names/aliases using their declaring scope, not a guess based only on a final
identifier such as `info`. The initial parser handles ordinary comma-separated
Rust expressions and resolves direct provider paths plus module-level direct,
renamed, glob, and provider-alias imports. The ARM regression covers qualified
and renamed calls from both logging families. RTT terminal-selector syntax and
more elaborate re-export chains remain explicit extensions.

Do not hoist argument evaluation outside the logging macro, which could change
filtering, evaluation order, or borrowing behavior. Do not rewrite string-literal
contents as if they were Rust paths. Real logging macros retain responsibility
for their native formatting rules; for example, defmt's normal `{}` formatting
uses its `Format` trait. See the [defmt logging reference](https://defmt.ferrous-systems.com/macros)
and [rtt-target usage](https://docs.rs/crate/rtt-target/latest).

Using native logging syntax does not choose one transport or require competing
RTT backends to run together. System-owned logging setup and generated Cargo
dependencies must match the chosen backend. Library checks use actual logging
APIs; the final target build validates the complete backend integration.

This requirement does not promise arbitrary custom-macro expansion. The
standalone implementation is syntax-aware handling of known logging inputs;
additional forms can be added as needed. Custom macros and general source
extraction remain review work. The legacy prototype still uses token-text
string replacement for macro-argument configuration rewriting and is not the
safe standalone implementation described here.

## Source-Transplant Boundary

Source transplantation must have an explicit boundary. Copying only one
function is insufficient when a task or init implementation depends on
module-level imports, helper functions, local types, constants, macros, or
submodules.

The agreed boundary for reusable tasks is the logical source module described
in [module grouping](#modules-group-related-tasks), whether file-backed, inline,
or the crate root. Selecting a task carries that module's ordinary supporting
imports, helper functions, types, implementations, and constants as a unit;
authors do not maintain a separate helper-selection list. It does not select
or instantiate sibling tasks. Unused supporting items and dependencies may
therefore remain and must compile. Precise pruning is deferred.

Different source modules keep separate supporting namespaces so aliases,
helper names, and local type names do not collide. Selected tasks and instances
from the same source module share its supporting type identity rather than
receiving independently duplicated type definitions. Reusing a task does not
automatically create another copy of module-level state. This does not merge
RTIC resource ownership or promise arbitrary global-state patterns are safe.

Complete task bodies still become real RTIC handlers. Generated supporting
modules and access wiring are a possible implementation mechanism, not runtime
wrappers around library tasks. The exact layout must be proven to preserve
ordinary private-helper access without asking authors to mark helpers public
solely for FerroForge. Copying a helper's text alone does not preserve scope.

The initial fixed ARM layout nests one real `#[rtic::app]` beneath one logical
source module. The renderer-owned layout generalizes that proof: each selected
module is emitted once under a deterministic child of one private generated
parent, and every handler imports only its own source namespace. Repeated
instances share one emitted support type, unselected sibling tasks are omitted,
and sibling modules with colliding `Sample`, `STEP`, and `update` names compile
in one RTIC app. Direct local/shared context fields are rewritten to their
composition resource names; context-parameter shadowing is rejected as
ambiguous. Support visibility is widened only inside the generated private
namespace for cross-module access and RTIC public resource interfaces, without
requiring authored visibility changes.

Selecting both an ancestor logical module and its descendant is still rejected
because their recursive support boundaries overlap. Supported meanings for
authored paths now start with a contained-only rule: task-body `self::` and
in-boundary `crate::` paths become absolute generated-namespace paths, while a
descendant support module may retain `super::` only when it does not climb above
the selected root. Escaping paths and relative `use` declarations are rejected.
Relative paths inside macro token streams and general cross-boundary references
remain outside the proof.

Start with self-contained task modules and ordinary external-crate imports.
The implementation must preserve the meaning of supported `self::`, `super::`,
and `crate::` references. More elaborate cross-module references, nested source
discovery, feature conditions, and custom macros need explicit support and
diagnostics, not an attempt to infer an arbitrary Rust module dependency graph.
The initial init transplant preserves self-contained crate-root support beside
the generated RTIC resources and handler. Child modules and broader init
reference/access rules remain to be proven.

The boundary and discovery are implemented, and the renderer-owned sibling-module
ARM layout has focused collision and contained-relative-path proofs.
Overlapping-module, relative-import/macro, and cross-boundary reference handling
remain work in
the [source-transplant boundary](#source-transplant-boundary). Rewrites must
preserve literals and unrelated identifiers; arbitrary string replacement in
macro token text cannot provide that guarantee.

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

[Current prototype](prototype.md) owns the description of what exists today,
separated into the active standalone path and the legacy app-backed path. This
chapter describes the agreed model, not its implementation status.

## Core Design Principle

FerroForge is a framework extending RTIC, not a new language for task behavior
or hardware setup. Keep ordinary Rust task and init bodies, familiar RTIC
contexts and resource access, and native HAL calls. Additional declaration
syntax must serve reusable contracts or firmware composition without introducing
a separate programming model.

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
