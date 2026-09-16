# FerroForge

FerroForge composes complete reusable RTIC tasks and project-owned
initialization source into a real RTIC firmware application. Developers write
task and init bodies against mock RTIC interfaces, check them in the appropriate
Rust target environment, and let a host renderer transplant those bodies using
each firmware's composition.

This mdBook is the single active documentation set for the repository. It
consolidates the earlier design notes, task-composition plan, architecture
discussion, dependency proposal, and workspace READMEs.

## Reading Guide

`AGENTS.md` in the repository root holds a task-oriented map; start there when
you already know what you are working on. This guide describes the chapters.

- [Governing requirements](governing-requirements.md) are G1-G7, the
  requirements authority. Every other chapter defers to them.
- [Architecture](architecture.md) records the agreed source-transplant model,
  firmware ownership, workspace separation, and validation stages. It owns the
  canonical task-authoring design that other chapters link to.
- [Current prototype](prototype.md) describes what exists today, separated into
  the active standalone path and the legacy app-backed path.
- [Dependency management](dependencies.md) records the agreed manifest-based
  requirements and conservative inclusion rule, the current registry prototype,
  explicit check-only setting, and conservative dependency merge policy.
- [Workflow](workflow.md) gives commands for checking source, rendering,
  building firmware, and using Rust Analyzer.
- [Review](review.md) is the decision record: current decisions, the next open
  point, and older decisions still in force.
- [Implementation plan](implementation-plan.md) describes the goal, the
  workspace/task/init design, the acceptance bar, and the verification rules.
- [Composition repair plan](composition-repair-plan.md) is the active work:
  per-firmware `composition!`, family backends, and one unified target.

## Status Conventions

**Agreed architecture** describes the intended product. **Current prototype**
describes implemented behavior. **Open** and **proposed** identify choices that
still need discussion; documenting a proposal does not adopt it.

Two Nucleo-F401RE pipelines check real STM32F401 HAL code and produce buildable
RTIC applications from unchanged reusable task source. Separate reusable task
crates and independent init crates exist today: `tasks/blinky` and both
`systems/*/init` packages. The agreed `firmware/` layout, family backends,
typed interrupts, and unified target are not yet implemented. The book's
proposed syntax is explicitly marked, and embedded Rust examples are not
configured to run in an online playground.

## Documentation Archive

Superseded source documents are preserved under `archive/<date>/` with their
original relative paths. Archived content is excluded from this book and its
search index. Agents require explicit user permission to access the archive;
ordinary documentation work does not grant that permission. See
[the archive policy](documentation.md#archive-access-policy).
