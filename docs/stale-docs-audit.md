# Stale and Incorrect Documentation Audit

Audited 2026-09-14 against the working tree (including the uncommitted book
edits and the untracked `docs/src/composition-repair-plan.md`). The book was
compared with the code in `ferroforge*`, `composer`, `embedded`, `tasks/`,
`systems/`, and the renderer test fixtures. `archive/` was not accessed.

This file is a fix list. It is not design authority; the mdBook remains the
single active documentation set. Delete each entry once it is fixed, and delete
this file when it is empty.

An independent [verification on 2026-09-15](#independent-verification-2026-09-15)
confirms the main findings, with the qualifications recorded below. Verification
does not mark the documentation fixes or the composition repair implemented.

**Method limits:** claims were checked by reading code, manifests, and tests,
not by running the test suite or the two Nucleo pipelines. Test-count claims
were checked by counting `#[test]` functions. `mdbook build docs` succeeds;
mdBook does not validate anchors, so those were checked separately.

## Main Pattern

Most problems come from incremental status edits. A newer sentence says a
feature is implemented, while an older sentence nearby still says it is open
or unproven. The code supports the newer sentence in every case below. The fix
is usually to delete or rewrite the older sentence, not to change the design.

## 1. Incorrect or Broken

| Location | Problem | Evidence | Suggested fix |
| --- | --- | --- | --- |
| `src/documentation.md:68`, `../embedded/README.md:4` | Link to `prototype.md#system-source` is broken. The heading is now "Legacy System Source". | `src/prototype.md:37` | Change the anchor to `#legacy-system-source`. |
| `src/introduction.md:40-42` | Says "Separate reusable task crates and independent system init crates remain planned work." | `tasks/blinky`, `systems/nucleo-f401re/init`, and `systems/nucleo-f401re-fast-blink/init` exist and are used by both pipelines. | Rewrite the status paragraph so the legacy example and the standalone systems are described separately. |
| `src/dependencies.md:199-200` | Says automatic source checking and the final Cargo invocation are "planned orchestration work, not behavior provided by today's composer". Lines 215-218 of the same section say the orchestration is implemented. | `run_blinky_nucleo_f401re_pipeline` in `systems/nucleo-f401re/app_composition/src/lib.rs` runs task check, init check, lockfile, firmware check, and release build. | Limit the sentence to the legacy `composer`. |
| `src/workflow.md:119-123` | Says the init checker does not yet provide "generated-manifest/orchestration integration". | `render_standalone_project` merges init dependencies into the manifest, and both Nucleo pipelines run the init check as a stage. | Remove that item from the list of missing features. |
| `src/workflow.md:267-269` | Says `.vscode/settings.json` covers the workspaces, and "each embedded workspace has its own `.cargo/config.toml`". | `linkedProjects` lists only the root, `embedded`, and `generated/nucleo-f401re`. `tasks/blinky`, both `systems/*/init`, and both `gen_app` are not linked. `tasks/blinky` and the init packages have no committed `.cargo/config.toml`; init's config is generated under `.ferroforge/init-check`. | Document the actual coverage, or add the missing projects and describe how Rust Analyzer gets `FERROFORGE_INIT_INTERFACES`. |
| `src/prototype.md:84-85` | Lists the `ferroforge` re-exports as `app`, `composition`, `dependency_registry`, `firmware`, `task`. | `ferroforge/src/lib.rs:17` also re-exports `init`. | Add `init`. |
| `src/implementation-plan.md:601-603` | The generic "source checks" rule says to run `cargo check --lib --target ...` from each source crate's workspace, and that applies to init as well. | Checking init needs the generated `.cargo/config.toml` that sets `FERROFORGE_INIT_INTERFACES`. The pipeline and `src/workflow.md:196-197` run the check from `.ferroforge/init-check` with `--manifest-path`. | Add the init exception, or link to the workflow command. |
| `src/dependencies.md:74-78` | Describes removal of "a direct check-only root import", which implies the cleanup follows `check-only-dependencies`. | `transplant.rs:333-334`, `640-641`, and `361` match the literal crate name `ferroforge`. Other names in `check-only-dependencies` are removed from the manifest, but their imports are neither removed nor diagnosed. | State that source cleanup currently covers only `ferroforge`, or make the transplant use the metadata list. |

## 2. Stale "Not Implemented / Open" Statements Contradicted by Code

### `src/prototype.md`

| Line | Stale statement | Actual state |
| --- | --- | --- |
| 159-164 | "Composition-driven translation of the standalone call into a real instance spawn remains open." | Implemented. Lines 354-358 of the same chapter say so. |
| 240-243 | Typed contracts are rejected by the legacy loader "because the new rendering path does not consume them yet". | The standalone transplant consumes them. The rejection is still correct, but the reason is not. |
| 206-211 | "Multiple named instances of one reusable definition and resource remapping are planned capabilities." | They are planned only for the legacy composer; the standalone path implements both. Say which path this describes. |
| 446-449 | "Generalized task instancing and `CONFIG.FIELD` rewriting remain work." | They remain work only for the legacy renderer (see line 457). Say which path this describes. |
| 195-196 | Target data is duplicated across "the macros, loader, and renderer". | This understates the duplication. The data is also in `standalone.rs` (`StandaloneTarget::stm32f401re`), and the system runtime versions in `systems/nucleo-f401re/app_composition/src/lib.rs` repeat the renderer's version constants. The repair plan relies on this inventory. |

### `src/implementation-plan.md`

| Line | Stale statement | Actual state |
| --- | --- | --- |
| 13-14 | "The phase gates below describe required evidence, not work already completed." | Phases 2-6 all record "Status: met". |
| 163-165 | "Composition integration and broader types remain to be implemented." | Composition integration is done. Only broader types remain. |
| 170-171 | "The current renderer ... still reads qualified `task::Config::FIELD` paths." | Only the legacy renderer does. The standalone transplant reads `CONFIG.FIELD`. |
| 221-222 | "The end-to-end transplant pipeline still need[s] work." | Phase 5 is met. |
| 452-457 | Remaining work includes "complete orchestration". Also, broader checking-only translation is "later Phase 4 work", although Phase 4 is closed. | Orchestration exists. Move the remaining item to the repair plan. |
| 485-486 | The checks "do not prove ... the end-to-end generated pipeline". | The verification table directly above lists both pipeline commands as passing. |
| 41-63 | The "Intended layout" (`systems/foxeer_f405v2/...`, `tasks/stm32f4/...`, `code_renderer/`) is not marked as superseded. | It is replaced by the agreed `firmware/` grouping with family backends in `src/composition-repair-plan.md`. |

### `src/introduction.md` and `src/workflow.md`

| Location | Problem |
| --- | --- |
| `src/introduction.md:24-32` | The reading guide says "the walkthrough is ready for a bounded implementation/proof step". Phases 2-6 are complete, and the active work is the repair plan. The guide also leaves out `composition-repair-plan.md`, which `SUMMARY.md` includes. |
| `src/workflow.md:3-4` | The sentence "The bounded two-system pipeline are covered in the implementation plan" has a grammar error, and the pipeline commands are in this chapter anyway. |

## 3. Low Severity

- `src/prototype.md:430-434`: the heading "Renderer source:" introduces
  `composer/src/main.rs`, which is the composer, not the renderer.
- `src/prototype.md:19`: describes `ferroforge-renderer` only as "host source
  loader and RTIC firmware renderer". It also contains standalone discovery,
  composition validation, init-check generation, transplant, dependency merge,
  and project emission.
- `src/implementation-plan.md:292-306`: the proposed
  `app!(board = ..., init = ..., tasks = [...])` shape is superseded by the
  agreed per-firmware `composition!` direction.

## 4. Stale Comments and Messages in Code

These are outside the book but repeat the same outdated status:

- `ferroforge/src/lib.rs:5`: the crate doc lists only `app!`,
  `dependency_registry!`, and `task`. It omits `composition!`, `init`,
  `firmware`, and the `mock` module.
- `ferroforge-contracts/src/lib.rs:336`: the legacy adapter error says
  "independent checking expansion/rendering is not implemented yet". Both
  exist; the legacy loader just does not accept typed contracts. The test at
  line 750 asserts this text, so update it too.
- `ferroforge-renderer/src/source.rs:107`: "this does not yet emit RTIC
  namespaces". `transplant.rs` emits them.
- `ferroforge-renderer/src/transplant.rs:5-6`: "Manifest generation and
  complete host orchestration remain separate later stages". Both now exist,
  in `standalone.rs` and the system composers.

## 5. Second Pass: Legacy Composition Path

Scope: `composer/src/composition.rs`, the `composition!` macro
(`expand_composition` in `ferroforge-macros`), `render_loaded_composed` and
`compose_tasks` in `ferroforge-renderer/src/lib.rs`, `loader.rs`, `embedded/`,
and `generated/nucleo-f401re`.

**Evidence:**
- Ran `cargo run -p ferroforge-example-composer --locked --offline`. The
  regenerated `generated/nucleo-f401re` matched the committed files exactly.
- Rendered variant compositions from a throwaway crate outside the repo
  through `render_loaded_composed`.

### Documentation Findings

| Location | Problem | Evidence | Suggested fix |
| --- | --- | --- | --- |
| `src/prototype.md:206-211` | Says the composition "can select existing task names". This implies a subset can be selected. | A composition without `timer_interrupt` renders with no error, but the output cannot compile. `init` still contains the unrewritten `timer_interrupt::Config::FREQUENCY_HZ` (`rewrite_config_paths` only knows selected tasks), and `hello_timer` stays in `Local` with no task claiming it. Dropping `blink` would likewise leave `blink::spawn()` in init. The embedded init is not adjusted to the selection. | Say that init is copied unchanged, so the selection must include every task that init references. Also consider making the renderer reject this case. |
| `src/prototype.md:206-211` | Leaves out the other constraints `compose_tasks` enforces. | Every configuration key must be supplied, and its type must match the embedded check type exactly (`u32` for a `u64` key fails: "must have type `u64`"). Every spawn alias must be bound. `binds` may only repeat the embedded binding. | List these constraints in the Host Composition section. |
| `src/prototype.md:453-457` | Says legacy spawn bindings "are validated and stored but are not used to rewrite task spawn aliases". This understates the problem. | Any task that calls `cx.spawn.<alias>()` renders code that uses a context field real RTIC 2 does not have (see `src/architecture.md:479`), and no error is reported. `compose_tasks` also accepts a hardware task as a spawn target; the `app!` checking macro rejects this (`src/prototype.md:154-155`). The current example uses no aliases, so the problem is latent. | State that the legacy renderer does not support spawn aliases in rendered firmware. |

### Code Observations (Not Documentation)

These are not doc errors, but they affect what the docs can promise for this path:

- `compose_tasks` has no negative tests. The renderer tests in
  `ferroforge-renderer/src/lib.rs` cover only successful overrides. The errors
  above were confirmed by the probe, not by committed tests.
- `composition!` checks only duplicate task names and dispatcher/interrupt
  overlap when the composer compiles. Duplicate or too few dispatchers, missing
  configuration keys, and missing spawn bindings are reported only when
  `cargo run` renders.
- By inspection, not tested: `ConfigPathRewriter::visit_macro_mut`
  (`lib.rs:970-980`) replaces strings in macro token text. If two selected
  tasks are named so that one is a suffix of the other (`blink` and
  `fast_blink`), `fast_blink :: Config :: PERIOD_MS` can become
  `fast_BLINK_PERIOD_MS`, depending on task order.

## Verified Accurate (No Action)

- `src/workflow.md:226-239` (Render the Application): the command, input,
  output path, and "does not run ARM checks" statement are accurate. The
  committed generated output is current.
- `src/prototype.md:436-451`: generated files, backend and registry
  dependency merge, task-prefixed configuration constants (including inside
  init), and no FerroForge dependency are all accurate for this path.
- `src/dependencies.md:293-309`: registry resolution for selected tasks only
  and conflict rejection match `resolve_dependencies`.

- Test counts in `src/prototype.md:474-479` and
  `src/implementation-plan.md:466`: 10 contract, 23 macro, 7 renderer/loader,
  10 discovery, 6 composition, 6 dependency, 4 standalone check, 8 transplant,
  3 project, 4 init, 3 first-system, 1 second-system (85 total, 2 ignored).
- Test names in commands, fixture package/bin names, `--ignored` Rust
  Analyzer tests, and the eight focused compile-fail cases.
- Package names in `cargo run -p ...` commands, the `.ferroforge/init-check`
  and `gen_app` paths, and the pipeline stage order.
- The "Evidence of the Current Gaps" table in `src/composition-repair-plan.md`.
- Dependency collector and merge behavior, and the check-only metadata
  validation, in `src/dependencies.md`.
- The discovery attribute allowlist, the init contract limits, and the
  STM32F401RE target data (memory, text offset `0x198`, probe chip).
- All other internal anchors and `{{#include}}` paths resolve.

## Unrelated Repository Hygiene Found During the Audit

- `ferroforge-renderer/tests/fixtures/init-diagnostics/target/` build output is
  tracked in Git. The other fixture `target/` directories are not.

## Independent Verification (2026-09-15)

Rechecked the cited active passages, implementation, manifests, and regression
tests against the current working tree, including its pre-existing edits.
`archive/` was not accessed. The audit is substantially correct, but its
"code supports the newer sentence in every case" summary is too absolute.

### Confirmed Findings

- The broken `#system-source` links, omitted `init` re-export, inaccurate
  independent-task/init status, contradictory orchestration/manifest status,
  source-check command omission, and incomplete IDE workspace coverage are real.
- Standalone checking, typed `CONFIG` generation, per-instance configuration,
  resource remapping, spawn/monotonic translation, namespace/private-helper
  support, native init integration, complete project emission, and both system
  pipelines exist. The blanket statements that these remain unimplemented
  need the bounded-standalone versus legacy distinction described in the audit.
- The older layout/API examples need explicit supersession labels. The current
  `firmware/` and family-backend agreement is still pending implementation;
  successful existing pipelines do not complete that repair.
- The remaining low-severity wording/example findings are supported. In
  particular, generated configuration child modules use `pub(super)` and their
  constants use `pub(crate)`; the outer configuration module remains private.
- Check-only dependency metadata filters manifest requirements, but source
  import cleanup specifically matches the literal `ferroforge` crate name.
  Arbitrary check-only imports are not removed by that metadata mechanism.
- The legacy adapter's "not implemented yet" diagnostic is stale, including
  the test asserting it. The facade's crate-level macro list is incomplete.
- Git tracks 652 files under the reported init-diagnostics `target/` path;
  the audit understates the scope of tracked build output (see correction 8).

### Corrections and Qualifications to This Audit

1. **`source.rs:107` is accurate for its API.** `TaskSources::select` selects
   source objects; it does not emit RTIC namespaces. `transplant.rs` does that
   separately. Remove "yet" or name the transplant API for clarity, rather
   than changing the comment to claim selection emits code.
2. **`transplant.rs:5-6` describes a real separation of responsibilities.**
   Manifest generation and orchestration are later pipeline stages implemented
   elsewhere. Naming those modules would remove ambiguity; their absence from
   this module is not itself an implementation defect.
3. **The 81-test sentence is dated history.** The current inventory is 85 tests
   (83 enabled, two ignored), but that does not disprove the explicitly dated
   2026-09-13 result. Refresh the current baseline without rewriting history.
4. **Some legacy descriptions already have contextual scope.** The Host
   Composition and Renderer sections in `prototype.md` describe the legacy
   path, and the clock paragraph in `review.md:379-383` begins with that scope.
   Explicit cross-links would help, but legacy instancing/spawn/clock gaps
   should not be erased because the standalone path implements them.
5. **Legacy init is not literally copied unchanged.** `render_main` invokes
   `rewrite_config_paths` on init (`ferroforge-renderer/src/lib.rs:759`). It
   does not prune init statements or resource declarations to match task
   selection. Say "not adapted to removed tasks" in the suggested fix.
6. **The composition macro validates more than the audit lists.**
   `expand_composition` calls `validate_interrupt_bindings`, which also rejects
   duplicate hardware interrupt bindings (`ferroforge-macros/src/lib.rs:1649`).
   Duplicate/insufficient dispatchers and missing configuration/spawn bindings
   still reach renderer-time validation as reported.
7. **One "verified accurate" citation is wrong.** The selected-task registry
   collection and conflict behavior is documented at `src/dependencies.md:165-181`,
   not `293-309`; the current chapter has only 223 lines. Also, the repair-plan
   evidence table originally overstated interrupt selection. That entry now
   says the legacy composer can restate but cannot change embedded bindings.
8. **Other fixture build directories are also tracked.** `git ls-files --
   ferroforge-renderer/tests/fixtures` identifies 276 files under
   `ra-diagnostics/target/`, 800 under `rtic-layout/target/`, and 732 under
   `sw/target/`, in addition to init-diagnostics. That is 2,460 tracked build
   files across four fixture directories, contradicting the audit's claim
   that the other fixture target directories are not tracked.

### Reproduced Legacy Defects

An isolated executable under ignored `target/audit-verification/` calls the
public `load_application` / `render_loaded_composed` APIs. It confirms:

- Removing `timer_interrupt` renders successfully while retaining its init
  configuration reference and the `hello_timer` local resource.
- Supplying `u32` for the legacy blink's `u64` configuration is rejected.
- Replacing `TIM2` with `TIM3` is rejected.
- A hardware spawn destination is accepted and `cx.spawn.report()` survives
  rendering unchanged. This is a render-time validation/translation defect;
  the current example does not exercise spawn aliases.
- The previously inspection-only suffix collision is reproduced: with `blink`
  selected before `fast_blink`, a macro argument containing
  `fast_blink::Config::PERIOD_MS` becomes `fast_BLINK_PERIOD_MS`.

The probes inspect emitted source and renderer errors; they do not claim a
separate ARM compilation of each deliberately invalid output. Fixes and
permanent regressions for these defects remain pending.

### Fresh Verification Evidence

- `cargo test --workspace --locked --offline` passed: 83 tests passed and two
  Rust Analyzer tests were ignored. This includes the real pipeline failure
  matrix, standalone ARM transplant checks, and generated project release link.
- `mdbook build docs` passed. Active book Markdown links were checked against
  generated HTML IDs and include paths were checked for existence. The two
  in-book `#system-source` failures and the matching embedded README link are
  confirmed; no additional missing book links/includes were found.
- Both `cargo run -p ferroforge-nucleo-f401re-composer --locked --offline` and
  `cargo run -p ferroforge-nucleo-f401re-fast-blink-composer --locked --offline`
  passed, including ARM source/init/firmware checks and release linking.
- `cargo run -p ferroforge-example-composer --locked --offline` passed;
  `git diff --exit-code -- generated/nucleo-f401re` found no changes.
- The isolated legacy probes above passed.

The opt-in Rust Analyzer tests, strict Clippy, and hardware execution were not
rerun for this verification. Earlier dated results for those remain historical
evidence rather than new verification claims.
