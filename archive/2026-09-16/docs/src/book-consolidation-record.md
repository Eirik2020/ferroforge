# Book Consolidation Record - Archived 2026-09-16

Archived from `docs/src/documentation.md` on 2026-09-16. A record of the
2026-09-05 migration that created this book from eight legacy Markdown files.
The migration is complete and the mapping table describes documents that no
longer exist as active files. No binding decisions were in it.

---

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
| `embedded/README.md` | [System source](prototype.md#legacy-system-source), [workflow](workflow.md) |
| `ferroforge_design.md` | [Mock expansion](prototype.md#mock-macro-expansion), [typed spawn model](prototype.md#typed-spawn-composition) |
| `task_dependency_tracking.md` | [Dependency management](dependencies.md) |
| `ferroforge_source_transplant_architecture.md` | [Architecture](architecture.md) |
| `ferroforge_task_composition_plan.md` | [Review](review.md), [implementation plan](implementation-plan.md) |

Obsolete macro implementation listings, task-owned priority examples,
macro-only dependency proposals, and incorrect generated RTIC attribute
examples remain historical material. The active book documents the clarified
architecture and current source behavior. The originals have not been deleted.
