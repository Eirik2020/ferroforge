# Repository Instructions

## Active Documentation

The single active documentation set is the mdBook at `docs/book.toml`.
Start with `docs/src/SUMMARY.md`. Record architecture, current behavior, open
review items, and implementation plans in the book. Root and crate READMEs
are brief entry points, not parallel documentation sets.

Keep agreed design, current implementation, and open proposals distinct.
Follow the current discussion recorded in `docs/src/review.md`. When an actual
agreement is reached, update the decision record and introduce the next open
discussion point in the same response. Keep remaining implementation work
visible; agreement on an approach does not mean it is implemented.

## Restricted Archive - Explicit User Permission Required

The user requires explicit permission before an agent accesses `archive/`.
Without that permission, do not list, search, open, read, include, summarize,
copy from, or otherwise inspect files or directories within `archive/`.
Do not use archived documents as active design authority or recover their
contents through alternate tools/cached copies to bypass this restriction.

A request to review the repo, update documentation, build the book, or continue
implementation does not grant archive access. If archived material is needed,
explain which material and why, ask for explicit access permission, and wait.
Use only the scope the user grants. Do not weaken this rule without an explicit
user instruction to change the archive policy.

For a user-requested archival operation, you may check exact destination paths
for collisions and move explicitly identified active files into a new archive
snapshot. This authorizes the migration and path checks, not reading existing
archived content. Do not scan the archive to verify the move afterward.

Keep the archive excluded from repository-wide searches, including searches
using `-uu` or other ignore overrides. Use explicit exclusions such as:

```text
rg --files -uu -g '!archive/**' -g '!docs/book/**' -g '!target/**' -g '!**/target/**'
```

Normal ripgrep searches also honor the archive exclusion in `.ignore`.
The archive is not part of the book source, includes, search index, or build
inputs. These are agent instructions and search exclusions, not filesystem ACLs.
