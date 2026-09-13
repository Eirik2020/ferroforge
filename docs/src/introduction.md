# FerroForge

FerroForge composes complete reusable RTIC tasks and system-owned initialization
source into a real RTIC firmware application. Developers write task and init
bodies using mock RTIC interfaces, check them in the appropriate Rust target
environment, and let a host renderer transplant those bodies using each
system's app composition.

This mdBook is the single active documentation set for the repository. It
consolidates the earlier design notes, task-composition plan, architecture
discussion, dependency proposal, and workspace READMEs.

## Reading Guide

- [Architecture](architecture.md) records the agreed source-transplant model,
  system ownership, workspace separation, and validation stages.
- [Current prototype](prototype.md) describes the API and implementation that
  exist today, including their limits.
- [Dependency management](dependencies.md) records the agreed manifest-based
  requirements and conservative inclusion rule, the current registry prototype,
  explicit check-only setting, and conservative dependency merge policy.
- [Workflow](workflow.md) gives commands for checking source, rendering,
  building firmware, and using Rust Analyzer.
- [Review](review.md) records the initial walkthrough agreements and remaining
  implementation details. Initial
  SW checking and composition-generated init interfaces are agreed, with
  native HAL initialization retained. Native logging and the logical-module
  support boundary, initial dependency policy, and initial implementation order
  are also agreed. The walkthrough is ready for a bounded implementation/proof
  step; remaining contract details and integration stay open.
- [Implementation plan](implementation-plan.md) describes the agreed initial work
  sequence and acceptance criteria.

## Status Conventions

**Agreed architecture** describes the intended product. **Current prototype**
describes implemented behavior. **Open** and **proposed** identify choices that
still need discussion; documenting a proposal does not adopt it.

The current example checks real STM32F401 HAL code and generates a buildable
RTIC application. Separate reusable task crates and independent system init
crates remain planned work. The book's proposed syntax is explicitly marked,
and embedded Rust examples are not configured to run in an online playground.

## Documentation Archive

Superseded source documents are preserved under `archive/2026-09-05/` with their
original relative paths. Archived content is excluded from this book and its
search index. Agents require explicit user permission to access the archive;
ordinary documentation work does not grant that permission. See
[the archive policy](documentation.md#archive-access-policy).
