# Current Prototype

This chapter describes the 2026-09-05 prototype baseline plus the implemented
standalone foundation. Typed standalone SW tasks compile independently, and one
host command now carries the centralized Nucleo-F401RE system through init
checking, rendering, real-RTIC checking, and an optimized release link.
The [architecture](architecture.md) and [review](review.md) distinguish the
planned independent task/init design from this example.

## Workspace Map

| Location | Role |
| --- | --- |
| Root `Cargo.toml` | Host workspace containing the framework, legacy composer, and first-system composer |
| `ferroforge` | `no_std` API, mock resource/clock types, metadata, macro re-exports |
| `ferroforge-contracts` | Shared task declaration parser and structural validation; no HAL dependency |
| `ferroforge-macros` | Procedural macros for tasks, apps, composition, and dependencies |
| `ferroforge-renderer` | Host source loader and RTIC firmware renderer |
| `composer` | Example host executable and its composition |
| `embedded` | Separate ARM library checking real HAL init, resources, and task bodies |
| `generated/nucleo-f401re` | Separate generated RTIC firmware project |
| `tasks/blinky` | Independent reusable `blink`/`report` task check workspace |
| `systems/nucleo-f401re/init` | Centralized native Nucleo init check workspace |
| `systems/nucleo-f401re/app_composition` | Host frontend owning the first standalone composition and target choices |
| `systems/nucleo-f401re/gen_app` | Generated standalone RTIC firmware workspace |

The legacy `embedded` and `generated/nucleo-f401re` projects, reusable task
workspace, system init, and standalone generated firmware are separate Cargo
workspaces. Only `systems/nucleo-f401re/app_composition` joins the host workspace;
it reads target source and metadata without importing target packages as host
dependencies.

## Legacy System Source

`embedded/src/lib.rs` currently groups target selection, shared/local
resources, initialization, and the default composition needed for checking.
These defaults are replaced by host composition where supported. The source
does not contain an executable firmware entry point.

The following is included from the actual source so it stays current:

```rust,ignore
{{#include ../../embedded/src/lib.rs}}
```

`#[ferroforge::firmware]` retains an item for ARM compilation and renderer
extraction. The mock macro supplies `init::Context` with real PAC peripherals
and Cortex-M peripherals on ARM. Its host version omits hardware initialization
and cannot serve as a substitute for the ARM check.

Legacy init is parsed as part of `app!`, and its names-only mock task types
depend on that same-crate app declaration.

## Centralized Standalone System

The first new-path system follows the agreed ownership layout:

```text
systems/nucleo-f401re/
|-- init/                 # native HAL init check workspace
|-- app_composition/      # host-only composition/target frontend
`-- gen_app/              # generated real-RTIC firmware workspace
```

Its init source is centralized with the board instead of the reusable tasks:

```rust,ignore
{{#include ../../systems/nucleo-f401re/init/src/lib.rs}}
```

`tasks/blinky` is a separate check workspace containing unchanged reusable
`blink` and `report` definitions. The app-composition crate discovers both
manifests, constructs the explicit host-side composition, generates init
interfaces, and renders `gen_app`. This is the initial frontend choice: normal
Rust using the owned composition model. It deliberately does not settle a
general author-facing macro or data-file grammar.

## Mock Macro Expansion

`ferroforge/src/lib.rs` re-exports `app`, `composition`, `dependency_registry`,
`firmware`, and `task`. Checking expansion is in `ferroforge-macros/src/lib.rs`;
task argument parsing and common declaration validation now live in
`ferroforge-contracts`.

`#[task(...)]` now has two expansion paths. Names-only declarations retain the
original function and create the app-backed module used by the prototype.
Typed standalone declarations use `bounds`, inline resource/configuration
types, inline spawn signatures, or `monotonic` to generate an independent
checking context. Final priorities belong in app/composition entries.

On the legacy path, the surrounding `app!` supplies `LocalResourceSpec` and `SharedResourceSpec`
implementations for selected resources, configuration constants, and spawn
alias methods. This lets the current tasks type-check against the concrete
types declared by this app. It does not yet allow them to compile independently
in unrelated task crates.

The standalone path generates generic resource parameters with their declared
trait bounds, mutable references for local resources, and typed shared-resource
lock proxies. It generates task-local typed `CONFIG` fields for the initial
`u32` scope and typed spawn methods whose failure values match RTIC's zero-,
one-, and multiple-input result shapes. These mocks are compile-only and may
panic if executed. The legacy app-backed stubs still use FerroForge's
`SpawnError::QueueFull`; the standalone transplant uses RTIC's returned inputs,
while legacy-path translation remains open. See the
[mock API review](review.md#3-supported-mock-api-and-real-rtic-translation).

Current task source, including the software task and hardware interrupt task:

```rust,ignore
{{#include ../../embedded/src/tasks.rs}}
```

## Typed Spawn Composition

The original design's typed alias model is present in the mock app expansion.
Each task declares aliases, and its app binds each alias to a selected task.
The following demonstrates current mock syntax; it does not establish that
spawn alias rendering is implemented:

```rust,ignore
use ferroforge::{app, task};

#[task]
async fn task_a(_cx: task_a::Context) {}

#[task]
async fn task_b(_cx: task_b::Context) {}

#[task(spawn = [blink, report])]
async fn task_c(cx: task_c::Context) {
    cx.spawn.blink().unwrap();
    let _ = cx.spawn.report();
}

app! {
    task_a { priority = 1 },
    task_b { priority = 1 },
    task_c {
        priority = 2,
        spawn = {
            blink => task_a,
            report => task_b,
        }
    },
}
```

Generated enums such as `Task`, task-local `Spawn`, and `SpawnSource` represent
bindings in the mock app. Enum checks reject unknown aliases, and count checks
reject missing bindings. Duplicate task names/bindings and targets absent from
the app are rejected; hardware tasks cannot be spawn targets. Unlike the
earliest design notes, the current renderer-facing structures also contain
textual identifiers and original Rust source.

Incoming parameter lists are derived from the task function. On the standalone
path, `spawn = [report(value: u32)]` generates an independent typed checking
method; on the legacy path, outgoing signatures still come from the selected
target in the surrounding app. Composition-driven translation of the standalone
call into a real instance spawn remains open. See
[task inputs and spawning](architecture.md#task-inputs-and-spawning).

## Target and Monotonic Support

The current renderer supports `STM32F401RET6` with `stm32f4xx-hal` 0.23.0 and
the Rust target `thumbv7em-none-eabihf`. It records MCU build/linker properties,
not a board peripheral or pin database. Flash starts at `0x08000000` with 512 KiB,
and RAM starts at `0x20000000` with 96 KiB. The current linker output also has
an F401-specific text offset.

One named monotonic can use SysTick or an STM32 timer. The example uses TIM5
and calls the check-only `Mono::start`/`Mono::delay` API. The app macro rejects
a monotonic timer used as a dispatcher, a hardware task interrupt, or a local
resource. These checks do not constitute a complete model of HAL ownership.

The legacy `MockMonotonic<TICK_HZ>` in `ferroforge/src/lib.rs` has
`start(clock_hz: u32)` and `delay(MillisDurationU64)`. It is not yet a faithful
SysTick mock: real SysTick startup takes a `SYST` peripheral as well, and its
duration/instant types depend on the selected profile. The mock delay blocks
the host thread or panics on ARM; it is not a scheduler or a time simulation.
The standalone `ferroforge::mock::systick::Mono` now provides a compile-only
`delay` accepting `fugit::Duration<u32, 1, 1_000>`; the fixture proves that a
legacy `u64` duration is rejected. The composition-generated init checker now
provides the matching `start(SYST, core_clock_hz: u32)` interface and compiles a
real HAL-derived core clock value against it on ARM.

Hardware tasks are synchronous and accept one context argument. Software
tasks use the real RTIC async shape in the example. App checks validate
dispatcher duplication/conflicts and the number required for distinct
nonzero software priorities. Final RTIC validation remains mandatory.

Target properties and parsing are duplicated between the macros, loader, and
renderer. Consolidation is planned before broadening board/HAL support.

## Host Composition

`composer/src/composition.rs` supplies the final scheduling and configuration:

```rust,ignore
{{#include ../../composer/src/composition.rs}}
```

The current composition can select existing task names and replace their
priorities, configuration, spawn binding data, and app dispatchers. Resource
declarations, init source, task bodies, hardware interrupt bindings, target,
monotonic selection, and dependency requirements still come from `embedded`.
Multiple named instances of one reusable definition and resource remapping are
planned capabilities.

## Source Loader

The existing app-backed path remains the firmware renderer's input:

`ferroforge-renderer/src/loader.rs` implements `load_application`. It reads
`src/lib.rs`, finds an `app!` declaration, loads one sibling task file and an
optional dependency registry file, and parses annotated task functions using
`syn`. The host does not compile or link the ARM library to load this data.

Discovery is currently based on that fixed file arrangement and unqualified
function names. Supporting imports/helpers from task files are not collected.
The logical-module supporting-source boundary is agreed, but generalized
package/module discovery, instance identity, and scope-preserving extraction
remain [implementation and verification work](review.md#4-supporting-source-and-reference-scope).

### Standalone Discovery Foundation

`ferroforge-contracts` now parses the new typed `bounds`, `local`, `shared`,
`config`, inline spawn signatures, and a `monotonic` source path. `TaskContract`
also retains the context parameter, incoming argument names/types, and returning
versus divergent SW task shape. It checks declaration structure, not body
semantics or compatibility with actual system bindings. `InitContract` validates
the bounded standalone init name, context, synchronous shape, and return pair.

The app-backed loader still uses its `LegacyTaskArguments` adapter, while the
task macro selects the standalone expansion for typed declarations. Names-only
prototype syntax still works. Repeated task argument keys,
duplicate declarations, overlapping local/shared claims, and malformed dependency
arguments now fail consistently instead of being accumulated by one parser and
overwritten by the other. Typed contracts remain rejected explicitly by the
legacy loader because the new rendering path does not consume them yet.

The new read-only `ferroforge_renderer::source::discover_tasks(&crate_root)` API
discovers ordinary file-backed/inline modules and `mod.rs` children. It keeps
logical module IDs, original file text, full function ASTs, module attributes,
visibility, imports, and ordinary supporting items. `TaskSources::select` picks
definitions individually under instance names, rejects missing definitions and
duplicate/invalid instance names, and borrows the same support module for repeated
instances. This is source-model identity, not a proof of generated Rust type
identity or private-helper accessibility.

The lower-level caller may supply an explicit source root. The package entry
point now asks Cargo to resolve an exact manifest to its package identity and
library target/root, then performs the same source discovery without compiling
or linking the package. IDs still use the canonical library source path plus the
logical module/task path; package IDs are retained separately. Files must stay
inside that source root's directory, and the library root must stay inside the
package. Conditional source (`cfg`/`cfg_attr`), custom
`#[path]`, item macro expansion, task-attribute/crate aliases, and attributes
outside the documented initial implementation's allowlist produce diagnostics.
Recognized declaration attributes are `#[task]`, `#[ferroforge::task]`,
`#[init]`, and `#[ferroforge::init]`; the initial checker further requires the
qualified init marker. Supported source attributes are `doc`, lint levels
(`allow`, `warn`, `deny`, `forbid`),
`deprecated`, `inline`, `cold`, `must_use`, `no_std`, `derive`, and `repr`.
Expression macro tokens are retained without expansion or remapping by
discovery. The later standalone transplant now parses the supported native
logging argument forms; general macro expansion remains outside this API.
Cross-module reference resolution and generated visibility layout are likewise
renderer responsibilities rather than discovery behavior.

A Cargo package-layout fixture lives under
`ferroforge-renderer/tests/fixtures/sw/` and is exercised by
`tests/source_discovery.rs` and `tests/standalone_check.rs`. Its manifest and
library target are resolved through Cargo, and `cargo check --lib --locked
--offline` proves the typed task source independently in that package. The
compile-fail harness checks invalid resource methods/types, an escaping shared
borrow, spawn arguments, configuration uses, a conflicting `CONFIG` binding,
and the SysTick delay type. Independent init checking now has a separate native
fixture and generated ARM checker, and the first Phase 4 regression transplants
that init into a generated real-RTIC ARM program. The standalone project writer
now emits the complete initial STM32F401RE target package and checks and
release-links that generated project. The real `tasks/blinky` and centralized
Nucleo system now exercise the same boundary; orchestration and additional
target profiles remain work.
`load_application` and the composer have not been switched to these fixtures.

### Composition-Generated Init Checker

`ferroforge_renderer::source::discover_init_package` finds one qualified
crate-root `#[ferroforge::init]` declaration and validates its initial
`fn init(init::Context) -> (Shared, Local)` signature. The fixture at
`ferroforge-renderer/tests/fixtures/init` owns real STM32F4 HAL clock/GPIO setup
and resource construction. FerroForge does not replace those HAL APIs.

`ferroforge_renderer::init_check::render_init_check` writes a generated
interface and Cargo configuration to a caller-selected directory. The interface
supplies real PAC/Core context fields, spawn functions derived from selected
task-instance signatures, and the initial `Mono::start(SYST, u32)` API. Cargo
then target-checks the original no-std init package and its authored manifest;
the check-only `ferroforge` dependency remains available there and is reserved
for exclusion from later firmware output. The package is checked, never run.

The init attribute parses the generated interface during expansion instead of
emitting a nested `include!`, which Rust Analyzer cannot load. A source-adjacent
interface/config deployment now proves focused authored-line diagnostics with
Rust Analyzer 1.98.0. The initial implementation still requires self-contained
crate-root support, STM32F401, and the 1 kHz/u32 SysTick profile. The standalone
renderer now transplants this checked init scope; init configuration, `cx.cs`,
init-local storage, and child modules remain unsupported.

`ferroforge_renderer::composition` now provides the next standalone foundation:
an owned host-side model and structural validator. It resolves definitions to
named instances and requires complete local/shared resource mappings,
configuration bindings, and spawn aliases. It rejects missing/unknown/duplicate
bindings, resource category conflicts, local-resource reuse, missing spawn
targets, obvious spawn argument-count mismatches, and profiles other than the
agreed SysTick 1 kHz/32-bit profile. Shared resources may be reused across task
instances. Configuration type comparison is syntactic and values are parsed as
Rust expressions; concrete value/type validity and spawn input type compatibility
remain compiler responsibilities.

This model deliberately does not select a new author-facing composition macro
grammar. The existing `composition!` and composer still drive only the
app-backed prototype. The first standalone system uses the owned model directly
from ordinary Rust in `systems/nucleo-f401re/app_composition`; generalized
frontend syntax and ergonomics remain open.

`ferroforge_renderer::transplant::render_rtic_app` is now an early renderer-owned
bridge from a validated standalone composition to real RTIC source. It emits
each selected logical module's ordinary support once, omits unselected sibling
tasks, renames definitions to instance names, and rewrites direct local/shared
context fields to their bound resource names. Deterministic private namespaces
and per-handler imports isolate colliding support names across sibling source
modules while keeping one RTIC app. Support items are made public only inside
the generated private parent namespace where cross-module access and RTIC's
public resource interfaces require it; authors do not change their source
visibility. The compatibility entry point `render_single_module_rtic_app`
retains the earlier one-module gate.

Direct task-body `CONFIG.FIELD` expressions are now resolved syntax-aware to
typed constants generated before the RTIC resource structs under
`__ferroforge_config::<instance>::<FIELD>`. This provides one readable overview
grouped by composed instance. Repeated instances of one definition can retain
different values without substituting literals into their bodies. Bare
`CONFIG`, unknown fields, and shadowing are diagnosed. Supported native logging
macros receive the same syntax-aware expression rewrites; configuration in
other macro token streams remains diagnosed.

Direct task-side `cx.spawn.<alias>(...)` calls are also rewritten to the bound
RTIC instance's `spawn` function. Arguments and surrounding `Result` handling
remain in place; ARM fixtures cover zero, one, and multiple input failure
shapes with aliases bound to differently named instances. Using the spawn
handle outside a direct call or inside a macro token stream is diagnosed.

For the validated initial SysTick profile, the transplant emits a real
`rtic-monotonics` SysTick declaration at 1 kHz and rewrites direct declared
`Mono::delay(...)` calls to it. The combined ARM proof rewrites the independently
checked init package's `Mono::start(...)` call and starts that generated
monotonic with its real `SYST` value and HAL-derived `u32` core frequency. The
task retains its `u32`/`ExtU32` duration conversion. Bare uses, other operations,
qualified source spellings, and macro-contained references are diagnosed as
outside the initial scope.

The transplant recognizes qualified and module-imported/aliased `defmt` level
and `println!` macros plus `rtt-target` `rprint!`/`rprintln!`. It parses their
ordinary comma-separated Rust expression arguments and applies the same
configuration, resource, spawn, clock, and contained-path transforms in place.
The macro path and delimiter remain intact, and format literal contents are not
rewritten. The ARM fixture proves qualified and aliased calls from both logging
families with mapped configuration/shared-resource arguments and literal text
that resembles those expressions. RTT terminal-selector forms and general
custom macros remain outside this bounded parser.

`render_rtic_app_with_init` now consumes the discovered `InitPackage` rather
than init source fragments: it carries root support and authored resource
structs/body into the app, replaces the checking-only marker, and rejects a
surviving `ferroforge` reference. The compatibility renderer still accepts a
parsed shell for earlier focused tests. Selecting both an ancestor source module
and its descendant is rejected to prevent duplicated support. Task-body
`self::` paths and `crate::` paths contained by the selected logical module are
rewritten to its generated namespace. Descendant supporting modules retain
`super::` paths that stay inside that boundary. Escaping `super::`/`crate::`
references and relative `use` declarations are diagnosed; macro-token relative
paths remain outside this slice. Context-parameter shadowing is also rejected so
resource mapping cannot silently change another binding; this is not the
complete renderer path. `render_standalone_project` now emits a manifest and
the complete initial STM32F401RE Cargo/linker/probe target files driven by the
collected standalone requirements and board profile.

Discovered task packages now also retain their conservative normal dependency
requirements and explicit check-only names. The initial collector supports
unconditional, non-optional, unrenamed registry dependencies, including unused
package dependencies needed by retained support. It diagnoses other runtime
manifest forms rather than guessing. Authored version requirement strings are
read from TOML because Cargo metadata presents a normalized requirement; Cargo
metadata remains authoritative for package/source/default-feature information.
The standalone dependency merger requires exact source, authored version, and
effective default-feature matches, unions features, and reports contributing
manifests/system selections on conflicts. The standalone project writer now
emits this merged data and checks and release-links the result on ARM; it does
not drive the legacy renderer's generated `Cargo.toml`.

`ferroforge_renderer::standalone::render_standalone_project` is the bounded
Phase 4 project boundary. It accepts one discovered task package, one discovered
init package, their validated composition, target-owned RTIC shell data, a
board target profile, and system runtime requirements. It emits `src/main.rs`,
`Cargo.toml`, `.cargo/config.toml`, `memory.x`, `Embed.toml`, and a target ignore
rule. The STM32F401RE profile owns its Rust target, probe-rs chip, flash/RAM
layout, vector-table text offset, and defmt filter. The positive regression
retains even an unused normal task dependency, excludes the check-only framework
dependency, checks the result on ARM, and completes an optimized release link.
Negative cases reject invalid target data and identify conflicting init/system
dependency contributors before creating output. General frontend syntax,
additional board profiles, and release-build orchestration remain outside this
project writer.

## Renderer

`ferroforge-renderer/src/lib.rs` exposes `render`, `render_loaded`,
`render_composed`, and `render_loaded_composed`. The example uses the latter
with `load_application`, keeping the host and ARM dependency graphs separate.
The definition-based APIs also exist, but importing a target-only library into
the host composer is not the example's workflow.

Renderer source:

```rust,ignore
{{#include ../../composer/src/main.rs}}
```

The renderer writes complete RTIC source, a Cargo manifest, `.cargo/config.toml`,
`memory.x`, `Embed.toml`, and a target ignore rule. It supplies the RTIC backend,
monotonic, HAL, Cortex-M, logging transport, and panic dependencies, then merges
selected task dependencies from the registry.

The generated module contains `Shared`, `Local`, `#[init]`, and real
`#[task(...)]` handlers under `#[rtic::app(...)]`. Configuration references
become ordinary task-prefixed constants, including references inside init.
The current input form is `task::Config::FIELD`; the renderer does not yet
recognize the preferred [task-local `CONFIG.FIELD` syntax](architecture.md#task-local-configuration-access).
It emits named, typed constants at the start of the app, before the resource
structs and init, rather than replacing every read with a literal. The selected
design builds on this placement with explicit grouping by composed task
instance; generalized task instancing and `CONFIG.FIELD` rewriting remain work.
Generated firmware has no FerroForge dependency, `build.rs` renderer, or
`OUT_DIR` include.

In this legacy renderer, spawn bindings are validated and stored but are not
used to rewrite task spawn aliases. Its macro-token configuration rewrite also
uses string replacement; preserving literals and scope is part of the remaining
work. The separate standalone transplant implements direct spawn calls and
direct configuration expressions but is not yet connected to this output path.

## Using the Mock API From Another Project

For local development, add `ferroforge` by path:

```toml
[dependencies]
ferroforge = { path = "../rtic-app-builder-v4/ferroforge" }
```

The macro crate is re-exported, so the small task/app example above needs only
that dependency. Hardware declarations also need their own actual target
dependencies. No publication status is assumed by these instructions.

## Verified Baseline

On 2026-09-13, 82 top-level workspace tests passed and two Rust Analyzer tests
remained opt-in/ignored: 10 contract, 23 macro, 7 renderer/loader, 10 discovery,
6 composition, 6 dependency, 4 standalone checking (one ignored), 8 standalone
transplant, 3 standalone project, 4 independent-init tests (one ignored), and 3
first-system frontend/orchestration tests. The latter include the real-process
negative pipeline matrix.
Nested checks cover one positive package, eight focused expected compiler
failures with authored-source locations, the fixed real-RTIC layout, and
renderer-emitted ARM source, including distinct configuration values for
repeated instances, zero/one/multiple-input spawn
translations, the real 1 kHz SysTick backend/delay, and mapped native logging
arguments with unchanged literals. They also cover the independently checked
native init transplanted beside selected real RTIC tasks. The standalone project
also emits its manifest and complete initial STM32F401RE target package from
collected requirements/profile data, then checks and release-links on ARM. The
opt-in diagnostic test separately passed with Rust
Analyzer 1.98.0, locating its E0599/E0308 errors on authored
task-body lines. Strict workspace Clippy, the existing ARM source check, and the
generated firmware release build also passed offline. The init suite also
checks a generated native-HAL ARM package plus expected failures for stale
composition interfaces, spawn arguments, startup types, and a missing profile.
The actual one-command pipeline independently checks `tasks/blinky` on ARM,
checks the centralized Nucleo init against its generated interface, checks the
real-RTIC `gen_app`, and release-links it using the emitted target package. Its
failure-order regressions both inject unsuccessful command stages and run real
negative source/composition/init/render/check/link cases. The latter includes a
concrete resource mismatch caught by the generated Rust/RTIC check and a
malformed test-owned linker script rejected by the real release link. Each case
asserts that later Cargo stages are skipped.
These results close the bounded Phase 2 gate and preserve the single-system
prototype. A separate opt-in Rust Analyzer 1.98.0 init regression locates its
focused E0107/E0308 diagnostics on authored init lines, closing the bounded
Phase 3 gate. The results do not validate general cross-boundary references,
generalized frontend syntax, every Rust Analyzer diagnostic, or cross-system
reuse.
