# Maintaining This Book

The only active documentation set is the mdBook rooted at `docs/book.toml`.
Its table of contents is `docs/src/SUMMARY.md`. Root and crate READMEs are short
entry points linking here; `AGENTS.md` files contain agent instructions.

## Updating Documentation

Edit the appropriate chapter and add new chapters to `SUMMARY.md`. Cross-link
to other book chapters using relative Markdown paths. Keep current behavior,
agreed architecture, and open proposals clearly distinguished. Record design
decisions in the [review chapter](review.md) as they are discussed, then update
the architecture and implementation plan consistently.

The prototype chapter includes selected live Rust files using mdBook's include
preprocessor. Embedded and proposed code examples are marked `rust,ignore`
because they need their own target, app context, or an as-yet undecided API;
validate actual source with the commands in the [workflow](workflow.md).

Build the book after edits:

```text
mdbook build docs
```

Missing summary chapters fail the build (`create-missing = false`). Check
internal links, the decision record, and implementation phase gates too.
`docs/book/` is disposable build output and is ignored by Git and ordinary
source searches.

## Archive Access Policy

The user requires explicit permission before an agent accesses `archive/`.
The repository root `AGENTS.md` records this rule so an agent sees it before
entering or searching the archive. Additional instructions in the archive
repeat it.

Without an explicit user request granting archive access, agents must not
list, search, open, read, include, summarize, or otherwise inspect archive
contents. General requests to inspect the repository, review plans, update
documentation, or build this book do not grant that permission. Search only
active files, explicitly excluding `archive/**` when using searches that
otherwise include ignored files.

Authorized archival work may move named current files into a new archive
snapshot. That authorization does not grant permission to read existing
archived documents. If historical content is necessary, explain which files
are needed and why, and wait for explicit access permission. Do not change or
remove the restriction to bypass it.

This is an agent instruction, not an operating-system permission boundary.
`.ignore` excludes the archive from ordinary ripgrep searches, and the book
does not include or index it. Filesystem ACLs and the user's own access have
not been changed.

## Consolidation Record

On 2026-09-05, eight legacy Markdown files were consolidated into this book.
The complete originals, including the review notes added during that session,
were moved into `archive/2026-09-05/`, preserving their original relative paths.
There was no pre-existing `book.toml` or `SUMMARY.md` in the source repository.

| Former active document | Active coverage |
| --- | --- |
| Root `README.md` | [Introduction](introduction.md), [workflow](workflow.md) |
| `ferroforge/README.md` | [Prototype](prototype.md), [dependencies](dependencies.md) |
| `ferroforge-renderer/README.md` | [Renderer](prototype.md#renderer), [workflow](workflow.md) |
| `embedded/README.md` | [System source](prototype.md#system-source), [workflow](workflow.md) |
| `ferroforge_design.md` | [Mock expansion](prototype.md#mock-macro-expansion), [typed spawn model](prototype.md#typed-spawn-composition) |
| `task_dependency_tracking.md` | [Dependency management](dependencies.md) |
| `ferroforge_source_transplant_architecture.md` | [Architecture](architecture.md) |
| `ferroforge_task_composition_plan.md` | [Review](review.md), [implementation plan](implementation-plan.md) |

Obsolete macro implementation listings, task-owned priority examples,
macro-only dependency proposals, and incorrect generated RTIC attribute
examples remain historical material. The active book documents the clarified
architecture and current source behavior. The originals have not been deleted.
