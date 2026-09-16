# Governing Requirements

FerroForge composes reusable RTIC tasks and project-owned initialization into a
real RTIC firmware application.

Agreed direction, updated 2026-09-16. This document is limited to two pages
maximum.

**Authority.** `AGENTS.md` in the repository root defines the authority order and
how conflicts are resolved. These requirements override conflicting earlier
proposals and every document below this one. If this document's structure proves
impossible or impractical, call a review with the user rather than editing around
it. Implementation status does not change the requirements; the current tree
(`systems/`, `tasks/`) predates G5 and is being migrated by the repair plan.

1. **G1 - Reusable task libraries.** Software task crates must be reusable
   across all hardware and projects. Hardware task crates contain HAL-specific
   tasks reusable across projects using that HAL. Firmware selects and
   instantiates definitions without changing their authored source.
2. **G2 - Minimal backends, separate HAL helpers.**
   - **G2a:** A backend is the minimum chip-family data FerroForge needs before
     a build: memory layout, probe identity, Rust target triple, device path,
     and the platform crate selections. It is build-time input, consumed by the
     CLI to emit target files and scaffold a firmware manifest, not a dependency
     of the firmware, and it is independent of project tasks.
   - **G2b:** HAL helper libraries are separate from backends.
     `ferroforge-stm32f4` is the FerroForge-oriented STM32F4 helper, sharing
     helpers/types between init and hardware tasks.
3. **G3 - One task model.** Hardware and software tasks share the same authoring
   and composition model. Hardware tasks require an interrupt binding;
   software tasks do not.
4. **G4 - Typed interrupts.** An interrupt binding names a variant of the
   selected device's own interrupt enum, so an interrupt the chip does not have
   is a compile error on the authored line. FerroForge does not define, own, or
   validate a separate interrupt list.
5. **G5 - Firmware structure.** A project's `firmware/` directory must contain
   all its firmware applications. Each `firmware/<name>/` is its own Cargo
   workspace whose authored package is the firmware binary: it holds the
   `composition!` declaration - target selection, task wiring, and the
   handwritten init with its `Shared` and `Local` resources. Whether that is
   one file or several is the author's choice. The CLI generates that
   firmware's target files from the selected chip; nothing else is generated.
   Firmware selects one authoritative target, and every generated file and
   build consumer uses it.
6. **G6 - Extend RTIC.** Task definitions and init should feel as close to normal
   RTIC as possible. Keep ordinary Rust bodies, native HAL initialization, and
   familiar task/context/resource access. Additional declarations serve reuse
   and composition. FerroForge must not introduce a separate authoring DSL.
7. **G7 - CLI conventions and library marker.**
   - **G7a:** FerroForge is a CLI with an expected project layout and a helper
     command to create new projects, and must support library reuse across
     project boundaries. Layout changes cause errors when FerroForge can no
     longer recognize required inputs.
   - **G7b:** Libraries must explicitly mark themselves as FerroForge libraries;
     attempting to use an unmarked library causes an error. The marker applies
     to crates selected as FerroForge libraries, not to ordinary Cargo
     dependencies. Marker absence is the only library check; broader library
     checks are out of scope.

**Open:** library placement and backend packaging.

Implementation details belong in [architecture](architecture.md), what is left
to build in [remaining work](implementation-plan.md), and decisions in the
[review record](review.md). Keep this page concise when updating it.
