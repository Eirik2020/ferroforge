# Dependency Management

The agreed direction uses ordinary task/init Cargo manifests as dependency
requirements for both independent checking and generated firmware. The
standalone path now collects, merges, and emits an initial supported manifest
subset, while the legacy rendering pipeline still uses a separate registry and
per-task dependency IDs. This chapter distinguishes the selected direction,
implemented standalone path, and legacy rendering behavior.

## Agreed Manifest-Based Requirements

Task and init crates declare dependencies once in their normal `Cargo.toml`.
The renderer reads those requirements as data when constructing the firmware
manifest; authors do not repeat them in a per-task `dependencies = [...]` list
or duplicate their version requirements in a Rust registry. This is an agreed
replacement direction, not an implemented migration of the existing registry.

Dependency inclusion follows the retained source, not only the selected task
bodies. Under the agreed logical-module support boundary, selecting a task
retains ordinary supporting imports, helpers, types, implementations, and
constants without instantiating sibling tasks. Those retained items must
compile even when unused.

For example, a module can contain a `toggle` task using `embedded-hal`, a
`report` task, and an ordinary `log_count` helper using `defmt`. Selecting only
`toggle` omits the `report` handler but retains `log_count` and the module's
imports. Its generated dependency requirements therefore still include `defmt`.

Initial inclusion is deliberately conservative at the source-crate level:
carry the participating crate's applicable normal dependencies, excluding
explicitly identified check-only dependencies. This may also retain dependencies
used only by unselected modules. Precise helper/dependency pruning and arbitrary
import-based dependency inference are not initial requirements. Init's manifest
likewise supplies requirements for its transplanted body and supporting code.

This does not add target-specific task/init implementation crates as host
renderer dependencies, nor add the transplanted source crate itself as a
generated firmware dependency. Normal external dependencies needed by that
source belong to the generated target graph.

The initial collector supports unconditional, non-optional, unrenamed registry
dependencies. It diagnoses target-specific, optional, renamed, git, path, and
workspace-inherited runtime dependencies. Build and development dependencies
are not normal source requirements and are not collected. This intentionally
narrow support implements the rule that broader forms need explicit handling or
diagnostics; conservative inclusion is not a promise to copy every manifest
entry blindly.

## Agreed Check-Only Dependency Setting

Source task/init crates explicitly identify checking-only dependencies in their
ordinary Cargo manifest:

```toml
[package.metadata.ferroforge]
check-only-dependencies = ["ferroforge"]
```

The listed dependency remains declared in the source crate's normal dependency
table and available for independent checking. The renderer excludes its entry
from the generated firmware manifest. Classification is explicit, not inferred
from crate names or whether a dependency provides procedural macros. Ordinary
external dependencies such as `embedded-hal` and `defmt` remain eligible for
inclusion under the agreed conservative rule.

Transplantation must also remove or translate the corresponding authoring
attributes, mock imports, and supported checking-only references to their real
RTIC/firmware equivalents. Omitting a Cargo entry alone does not make retained
source valid. The setting is not a general implementation for replacing
arbitrary custom mocks; unsupported uses still require support or diagnostics.

The standalone package collector now parses this metadata, rejects unknown or
duplicate entries and names that are not normal dependencies, and excludes the
listed requirements before runtime-source validation. A check-only path
dependency is therefore permitted in the first fixture. The bounded init
transplant now replaces its `ferroforge` marker, removes a direct check-only root
import, and rejects a surviving `ferroforge` reference. The centralized Nucleo
frontend and complete initial target package exercise that path. Broader
checking-only source translation remains incomplete; final target checking is
still required.

## Agreed Initial Merging and Conflict Policy

The system supplies HAL, RTIC, monotonic, logging-backend, and panic dependency
choices. Task/init manifests contribute requirements that these selections
must respect; system choices do not silently override source requirements.

For the initial implementation:

- Merge entries for the same package only when package sources, version
  requirement strings, and effective `default-features` settings match.
- Combine their requested features into one feature set. This is not proof
  that every resulting feature combination is valid.
- If sources, version requirements, or default-feature settings differ, emit
  a clear conflict diagnostic identifying the contributing manifests or
  system/backend selections and their conflicting values.
- Require explicit alignment before proceeding. Even differently written
  requirements that might be compatible are initially rejected; smarter
  compatibility handling is deferred.

This is a deliberately conservative generator rule, not an attempt to replace
Cargo's resolver or describe every combination Cargo could accept. Matching
version requirements also do not guarantee identical resolved versions across
independent workspaces. Final target-aware Rust/RTIC checking and the firmware
build remain mandatory to catch incompatible feature combinations, concrete
type mismatches, and other integration failures.

`ferroforge_renderer::dependencies` now implements this merge policy for the
standalone model. It retains exact Cargo source identifiers, preserves version
requirements as authored in each manifest, compares effective default-feature
settings reported by Cargo, unions feature sets, and identifies all contributing
manifests or system selections in conflicts. `render_standalone_project` now
wires that result to generated manifest output for the bounded crates.io-only
scope. This does not imply that the prototype registry has been replaced or
that broader source/feature handling is complete.

## Registry and Task Requirements

This section describes the current prototype, not the agreed replacement API.

The embedded app owns a registry through `dependency_registry!`. Its current
contents are included from `embedded/src/dependencies.rs`:

```rust,ignore
{{#include ../../embedded/src/dependencies.rs}}
```

Tasks declare IDs from this registry, for example
`dependencies = [Fugit, Defmt]`, and can request extra features:

```rust,ignore
#[task(dependencies = [Serde(features = ["derive"])])]
async fn report(_cx: report::Context) {
    // The containing app must provide a Serde registry entry.
}
```

The app references its registry before the target declaration:

```rust,ignore
app! {
    dependency_registry = crate::dependencies,
    target = {
        mcu = STM32F401RET6,
        hal = stm32f4xx_hal,
    },
    // Dispatchers, resources, init, and selected tasks follow.
}
```

The macro emits typed IDs and requirement metadata. The current API includes
`DependencyDefinition<Id>`, `DependencyRequirement<Id>`, the app's
`DEPENDENCY_REGISTRY`, `TASK_DEPENDENCIES`, and `TASK_DEPENDENCY_COUNT`.
Renderer-facing `DependencyCatalogEntry` and `TaskDependencyDefinition`
structures carry textual IDs and metadata without requiring a target crate to
be loaded into the host executable.

The macro checks unknown IDs, dependencies declared without a registry, and
duplicate registry IDs/packages/features or task requirements. The source
loader still parses app/registry declarations separately. Task argument parsing
and common validation are now shared through `ferroforge-contracts`; broader
shared validation remains part of the
[implementation plan](implementation-plan.md#development-order).

## Final Manifest Construction

The renderer collects dependencies from selected tasks, resolves them through
the registry, groups them by package, and unions the registry's base features
with task-requested features. For example:

| Requirement | Features |
| --- | --- |
| Registry entry for `serde` | `[]` |
| Task A | `["derive"]` |
| Task B | `["alloc"]` |
| Generated `serde` dependency | `["alloc", "derive"]` |

HAL, RTIC, monotonic, Cortex-M, logging transport, and panic crates are currently
backend-owned dependencies. The renderer rejects conflicting version strings
or default-feature settings for a package it has already inserted. This is
not a general semver compatibility resolver.

The implemented registry describes package names, versions, default features,
and feature lists. The standalone collector deliberately diagnoses
path/git/renamed runtime dependencies in its initial scope. It conservatively
includes every supported normal dependency from a participating task package,
including one used only by retained or unused support. Init package collection
uses the same collector. The new standalone project writer merges both package
inputs with explicit system runtime selections and emits the result. The legacy
renderer's actual output is still driven by backend dependencies and selected-
task registry requirements.

## Cargo and Independent Checks

Cargo resolves dependencies before procedural macro expansion. A macro cannot
add a new dependency to the graph already being compiled. The current host
generator produces the final manifest; developers invoke Cargo separately for
that project using the [workflow commands](workflow.md#check-and-build-firmware).
Automatic source checking and final Cargo invocation are planned orchestration
work, not behavior provided by today's composer.

The dependency registry is renderer data. It does not install dependencies
into the task/init crate performing an independent check. Those crates need
Cargo declarations for every dependency they use. Today the embedded manifest
and Rust registry duplicate some version information.

Keeping versions out of task attributes therefore does not mean independently
compiled task packages have no version requirements. A shared policy or
consistency check must keep the check manifests and generated manifest
compatible. Checking a task against one HAL version/feature set does not prove
that a different final selection works. Rust and RTIC check the final graph.
The manifest source of requirements, conservative inclusion, explicit check-only
setting, and initial merging policy are agreed in
[review item 5](review.md#5-validation-guarantees-and-dependency-consistency).
Their bounded standalone collection, merge, manifest emission, and generated
ARM project check/release-link proof are implemented, including first-system
integration and its initial orchestrated command. Broader failure injection and
stronger cross-workspace resolution guarantees are not claimed.

An early proposal placed optional runtime dependencies behind framework
features to support a macro-only build. The agreed architecture instead uses
the explicit source renderer and generated project described here. It does
not require the host framework to own every target runtime dependency.
