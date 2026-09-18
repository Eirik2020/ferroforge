# Repository Instructions

## Authority

This file is the highest-authority document in the repository; only the user
outranks it. On conflict the order is: the user; this file;
`docs/src/governing-requirements.md`; all other documents.

Any other document that diverges from this file or the governing requirements is
wrong; correct it immediately without asking permission. A conflict between this
file and the governing requirements needs the user's explicit decision - raise it
and wait rather than choosing a side.

## Active Documentation

The single active documentation set is the mdBook at `docs/book.toml`.
Start with `docs/src/SUMMARY.md`. Record architecture, current behavior, open
review items, and implementation plans in the book. Root and crate READMEs
are brief entry points, not parallel documentation sets.

Read `docs/src/governing-requirements.md` for the governing requirements. That
document states its own size limit. Implementation details belong in the linked
book chapters. Acceptance evidence, progress, and discussion history do not:
they belong in commit messages or nowhere.

Keep agreed design, current implementation, and open proposals distinct.
Follow the current discussion recorded in `docs/src/review.md`. Keep remaining
implementation work visible; agreement on an approach does not mean it is
implemented.

Codex only: when an actual agreement is reached, update the decision record and
introduce the next open discussion point in the same response.

## Documentation Map

Start from the task, not the chapter. Read the named sections, not whole files.

| Task | Read |
| --- | --- |
| Authoring or changing a reusable task | `architecture.md` Task Authoring |
| Firmware composition, init, or resources | G5; `architecture.md` Firmware Composition |
| Hardware tasks and interrupt binding | G3 and G4; `architecture.md` Task Authoring |
| Dependencies and manifests | `dependencies.md` |
| Building or checking anything | `workflow.md` |
| What the code does today | `prototype.md` |
| What is left to build | `implementation-plan.md` |
| Checking whether something was already decided | `review.md` |

One topic, one owner. Specify a topic in its owning chapter; every other mention
links there instead of restating it.

| Topic | Owner |
| --- | --- |
| Requirements G1-G7 | `governing-requirements.md` |
| Task authoring, firmware composition, what the compiler checks | `architecture.md` |
| Where dependency requirements live | `dependencies.md` |
| What the code does today | `prototype.md` |
| Commands | `workflow.md` |
| What is left to build | `implementation-plan.md` |
| What was decided, and where it is specified | `review.md` (index only) |

The whole book is 1139 lines, so read a chapter when a section is not enough:

- `governing-requirements.md` (64) - G1-G7. Loaded at launch for Claude.
- `architecture.md` (198) - the agreed call-through model. Owns the canonical
  authoring design; other chapters link here rather than restate it.
- `review.md` (273) - current decisions, the next open point, an index of
  older decisions still in force, and what the call-through model superseded.
- `implementation-plan.md` (54) - what is left, in unblocking order.
- `workflow.md` (180) - every command, including the opt-in cross-compiles.
- `prototype.md` (143) - what exists today and what does not.
- `dependencies.md` (112) - where requirements live, and how a HAL-specific
  crate names a chip without choosing one.

Superseded transplant-era chapters are under `archive/2026-09-16/`, and the code
they describe is on branch `main` and `test/new_task_method`.

## Always-Loaded Context Budget

`CLAUDE.md`, `AGENTS.md`, and every file `CLAUDE.md` imports are loaded into
each session before any work starts. Their combined line count is one shared
budget, not a per-file allowance:

- **At 300 lines combined**, stop and ask the user how to resolve it before
  adding anything further. Offer the alternatives: move the content into a
  path-scoped rule under `.claude/rules/`, into a skill, or into an on-demand
  chapter; or trim what is no longer earning its place.
- **At 400 lines combined**, nothing further may be added to any file in this
  tier until the user resolves it.

This budget is separate from the per-chapter rule below, which counts one file
at a time and triggers cleanup rather than a prompt.

## Document Size and Content

A book chapter crossing roughly 400 lines is a signal to apply the spent-material
rule below, not a cap to design around. Splitting a document to get under the
number is not compliance: four 200-line files cost the same context as one
800-line file and add routing surface. The fix is to promote what still binds
and archive the rest, or not to write it at all. `governing-requirements.md`
states its own stricter limit.

Do not write these into the book at all:

- Progress narration and status records ("Phase 4 is now met", "this is
  implemented as of ..."). What changed and why belongs in the commit message.
- Restatements of what the code does. A reader can open the file; describe
  intent, constraints, and rationale instead.
- Completion notes for a task you just finished.
- A new document when an existing chapter covers the topic. Check the
  documentation map above first.

## Retiring Stale Material

Fix a specific stale passage in place. A whole document or section that is spent
or superseded is handled differently:

1. Find what still binds. Old material is not automatically dead - a decision
   that was never superseded, only buried under newer text, is still in force.
2. Promote those decisions into the active document first, grouped by topic.
3. Then archive the remainder verbatim, under `archive/<date>/` mirroring the
   original relative path, with a header naming the source file, the archival
   date, and the fact that binding decisions were promoted out of it.

Never archive before promoting. Archived material needs explicit permission to
read, so a binding decision left behind is lost in practice. Do not rewrite
spent material to look current, and do not delete it instead of archiving.

Verify any move or archive, before and after. A successful `mdbook build` and a
correct-looking heading list prove nothing about content survival:

- **Before**, find what links into the sections you are moving or removing:
  `rg -o '<file>\.md#[a-z0-9-]+' --glob '!archive/**'`. Anchors come from
  heading text, so moving a heading is safe but renaming, retitling, or
  replacing one with a table is not. Repoint every hit in the same change.
  `mdbook build` does not validate anchors; after any such edit, check every
  link in the book against the built HTML rather than trusting the build.
- **After**, diff sorted content against a copy taken before the edit:
  `diff <(sort <backup>) <(sort <file>) | grep '^<'`. The only lines listed
  must be ones you deliberately changed. Anything else is content you dropped
  without noticing.

Both checks exist because both failures happened: archiving `review.md` orphaned
six inbound links, and reordering `prototype.md` silently deleted its workspace
table while still building cleanly.

## Restricted Archive - Explicit User Permission Required

Explicit user permission is required before an agent accesses `archive/`.
Without it, do not list, search, open, read, include, summarize, copy from, or
otherwise inspect anything under `archive/`, and do not recover archived content
through alternate tools or cached copies. Archived documents are never active
design authority.

Reviewing the repo, updating documentation, building the book, or continuing
implementation does not grant access. If archived material is needed, explain
which material and why, then wait. Use only the scope the user grants. Do not
weaken this rule without an explicit user instruction to change the archive
policy.

A user-requested archival operation authorizes checking destination paths for
collisions and moving explicitly identified active files into a new archive
snapshot. It does not authorize reading existing archived content, or scanning
the archive to verify the move afterward.

Keep the archive excluded from repository-wide searches, including searches
using `-uu` or other ignore overrides. Use explicit exclusions such as:

```text
rg --files -uu -g '!archive/**' -g '!docs/book/**' -g '!target/**' -g '!**/target/**'
```

Normal ripgrep searches also honor the archive exclusion in `.ignore`.
The archive is not part of the book source, includes, search index, or build
inputs. These are agent instructions and search exclusions, not filesystem ACLs.
