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
document states its own size limit. Keep implementation details, acceptance
evidence, progress, and discussion history in the linked book chapters.

Keep agreed design, current implementation, and open proposals distinct.
Follow the current discussion recorded in `docs/src/review.md`. Keep remaining
implementation work visible; agreement on an approach does not mean it is
implemented.

Codex only: when an actual agreement is reached, update the decision record and
introduce the next open discussion point in the same response.

## Documentation Map

Read the smallest thing that answers the question. Line counts flag the cost;
prefer a named section over a whole file.

- `governing-requirements.md` (62) - G1-G7 and the authority order. Loaded at
  launch for Claude; read it first otherwise.
- `composition-repair-plan.md` (691) - the active repair: unified target
  contract, agreed workspace layout, implementation sequence, acceptance
  evidence. Read when doing or planning current work.
- `prototype.md` (515) - what the code does today, component by component.
  Prefer this over `architecture.md` for questions about current behavior.
- `architecture.md` (1176) - the agreed source-transplant model, validation
  stages, and transplant boundary. Read only when changing the model or
  checking a design invariant.
- `review.md` (204) - "Current Decision" holds the live decisions and the next
  open point; "Binding Decisions Carried Forward" holds older decisions that
  still govern. Both are in force. History is archived, not here.
- `implementation-plan.md` (964) - phase gates and acceptance criteria at
  "Phase 1" onward. The "Progress Record" sections are historical.
- `dependencies.md` (223) - manifest-based requirements, check-only metadata,
  merge and conflict policy.
- `workflow.md` (295) - commands for checking, rendering, building, and
  Rust Analyzer setup.

## Always-Loaded Context Budget

`CLAUDE.md`, `AGENTS.md`, and every file `CLAUDE.md` imports are loaded into
each session before any work starts. Their combined line count is one shared
budget, not a per-file allowance:

- **At 250 lines combined**, stop and ask the user how to resolve it before
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
  implemented as of ..."). Phase evidence belongs in the repair plan; what
  changed and why belongs in the commit message.
- Restatements of what the code does. A reader can open the file; describe
  intent, constraints, and rationale instead.
- Completion notes for a task you just finished.
- A new document when an existing chapter covers the topic. Check the
  documentation map above first.

## Pending Documentation Fixes

`docs/stale-docs-audit.md` lists book passages that are stale, contradict
each other, or no longer match the code (audited 2026-09-14). It is a fix list,
not design authority. Where an audit entry and a book passage disagree about
current behavior, verify against the code before relying on either. When you
edit a listed chapter, fix the related entries and remove them from the audit.
Delete the file once it is empty.

Scope: audit entries are specific passages with verified corrections, so fix
them in place. A whole document or section that is spent or superseded is
handled differently:

1. Find what still binds. Old material is not automatically dead - a decision
   that was never superseded, only buried under newer text, is still in force.
2. Promote those decisions into the active document first, grouped by topic.
3. Then archive the remainder verbatim, under `archive/<date>/` mirroring the
   original relative path, with a header naming the source file, the archival
   date, and the fact that binding decisions were promoted out of it.

Never archive before promoting. Archived material needs explicit permission to
read, so a binding decision left behind is lost in practice. Do not rewrite
spent material to look current, and do not delete it instead of archiving.

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
