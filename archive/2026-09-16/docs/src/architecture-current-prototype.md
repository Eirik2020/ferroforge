# Architecture Chapter - Current Prototype Section - Archived 2026-09-16

Archived from `docs/src/architecture.md` on 2026-09-16. A snapshot of what the
prototype did, plus dated test-count results from 2026-09-13 and 2026-09-15.
The section labelled its own results "historical".

Two reasons it was archived rather than corrected: `prototype.md` owns "what the
code does today", so this was a second description of the same thing; and test
counts and pass/fail state are derived from the code, so they belong to a test
run, not a design chapter.

---

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

The legacy prototype successfully checks and builds the generated STM32F401RE
RTIC firmware. The separate standalone path now discovers task workspaces,
preserves the initial supporting-source boundary, distinguishes definitions
from instances, rewrites resource/configuration/spawn/clock/logging uses, and
ARM-checks emitted RTIC source. A separate composition-generated checker now
ARM-checks the native system init body, and the bounded Phase 4 path transplants
that authored init into the real-RTIC output. The same path now emits a merged
manifest and the complete initial STM32F401RE target package, then checks and
release-links the generated project. Its initial explicit-Rust frontend now
lives under `systems/nucleo-f401re` and `systems/nucleo-f401re-fast-blink`, where
init, composition, and generated firmware have separate owned directories.
Both reuse `tasks/blinky`; the second composer uses the first composer's pipeline
implementation. These paths do not replace the legacy composer or implement the
required `firmware/` hierarchy and family backend repair.

On 2026-09-13, all 81 default top-level workspace tests passed, with two
additional Rust Analyzer diagnostic tests opt-in/ignored. Strict Clippy, the ARM
source library check, and the existing generated firmware release build also
passed offline. These are historical results. On 2026-09-15,
`cargo test --workspace --locked --offline` passed 83 tests with two Rust Analyzer
tests ignored, and both current Nucleo pipelines passed ARM checks and release
links. That verification did not rerun strict Clippy or the opt-in Rust Analyzer
tests. The workspace suite includes an independently checked
standalone SW package, eight expected compiler failures with authored-source
locations, a fixed real-RTIC ARM layout, and renderer-emitted ARM source with
repeated per-instance configuration and zero/one/multiple-input spawn
translation plus the real 1 kHz SysTick backend. The emitted ARM cases also
cover mapped arguments in qualified and aliased native logging calls while
preserving lookalike literal text, and one combines the independently checked
native init with selected real RTIC tasks. Another emits the merged manifest,
linker memory map, Cargo runner/linker settings, and probe configuration, then
checks and release-links that generated project on ARM. First-system frontend
regressions also render the centralized Nucleo init/composition/task selection
and prove command-stage failure short-circuiting.
The
opt-in Rust Analyzer 1.98.0 test locates E0599/E0308 at authored task-body lines.
These checks close the bounded Phase 2 gate and preserve the existing prototype.
The default init tests add a
native-HAL ARM check plus expected stale-interface/spawn/startup/profile
failures. A second opt-in Rust Analyzer 1.98.0 regression locates the focused
E0107/E0308 errors on authored init lines, closing the bounded Phase 3 gate. The
new system composer now provides the first one-command pipeline: child Cargo
processes check reusable tasks and init, then check and release-build the
rendered firmware. Command-stage failures stop later stages;
`real_failures_stop_at_their_pipeline_boundaries` also covers the required
injected source, render, and link failure classes, closing the bounded Phase 5
gate. The second same-board pipeline supplies Phase 6 reuse evidence.

