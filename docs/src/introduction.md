# FerroForge

FerroForge composes reusable RTIC tasks and project-owned initialization into a
real RTIC firmware application. A reusable task is an ordinary Rust function in
its own crate; a firmware's `app!` expands in place into a real `#[rtic::app]`
that calls it. Nothing is copied between crates and no separate project is
generated.

The macros borrow RTIC's words for RTIC's ideas: `#[ferroforge::task]` marks a
definition, `ferroforge::app!` declares the application that selects it.

FerroForge is not mature. Its macro-generated interfaces are a versioned ABI
between task crates and firmware, and it is free to break that ABI while the
design settles.

This mdBook is the single active documentation set for the repository.

## Reading Guide

`AGENTS.md` in the repository root holds a task-oriented map; start there when
you already know what you are working on. This guide describes the chapters.

- [Governing requirements](governing-requirements.md) are G1-G7, the
  requirements authority. Every other chapter defers to them.
- [Architecture](architecture.md) records the agreed model: how a reusable task
  is authored, how a firmware composes one, and what the compiler checks. It
  owns the canonical authoring design.
- [Current state](prototype.md) records what exists today and what does not.
- [Dependency management](dependencies.md) records where requirements live now
  that Cargo resolves them.
- [Workflow](workflow.md) gives the commands.
- [Review](review.md) is the decision record: current decisions, the next open
  point, and older decisions still in force.
- [Remaining work](implementation-plan.md) records what is left, in the order
  that unblocks the rest.

## Status Conventions

**Agreed** describes the intended product. **Current state** describes
implemented behaviour. **Open** identifies choices that still need discussion;
recording a proposal does not adopt it.

## Superseded Designs

This repository exists partly to test different methods, and two earlier designs
were built and measured before being set aside.

| Design | Task bodies | Init | Recoverable from |
| --- | --- | --- | --- |
| Full transplantation | copied, rewritten | copied | `main` |
| Transplanted init | called | copied into a generated project | `test/new_task_method` |
| Call-through (current) | called | authored in place | this branch |

Their chapters are preserved under `archive/2026-09-16/`. Archived content is
excluded from this book and requires explicit permission to read; see
[the archive policy](documentation.md#archive-access-policy).
