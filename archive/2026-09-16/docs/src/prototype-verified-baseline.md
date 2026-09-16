# Prototype Verified Baseline - Archived 2026-09-16

Archived from `docs/src/prototype.md` on 2026-09-16. A dated record of which
tests passed on 2026-09-14, with counts. It contained no design decisions.

Test counts and pass/fail state are derived from the code: run the suite. This
record was already stale when archived (see `docs/stale-docs-audit.md`).

---

## Verified Baseline

On 2026-09-14, 83 top-level workspace tests passed and two Rust Analyzer tests
remained opt-in/ignored: 10 contract, 23 macro, 7 renderer/loader, 10 discovery,
6 composition, 6 dependency, 4 standalone checking (one ignored), 8 standalone
transplant, 3 standalone project, 4 independent-init tests (one ignored), 3
first-system frontend/orchestration tests, and 1 second-system reuse test. The
first-system tests include the real-process negative pipeline matrix.
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
The second `systems/nucleo-f401re-fast-blink` command uses the same bounded
Nucleo pipeline mechanics with its own init and composition profile. It maps
`blink`/`report` to `heartbeat`/`diagnostics`, maps the task resources to
`activity_led`, `pulse_count`, and `heartbeat_enabled`, and emits a 125 ms
period rather than the first system's 500 ms value. Its render regression
compares the reusable task file before and after generation, and its complete
ARM pipeline checks and release-links.
These results close the bounded Phase 2 gate and preserve the single-system
prototype. A separate opt-in Rust Analyzer 1.98.0 init regression locates its
focused E0107/E0308 diagnostics on authored init lines, closing the bounded
Phase 3 gate. The results do not validate general cross-boundary references,
generalized frontend syntax, every Rust Analyzer diagnostic, or portability to
another MCU/HAL target.
