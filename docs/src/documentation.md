# Maintaining This Book

The only active documentation set is the mdBook rooted at `docs/book.toml`.
Its table of contents is `docs/src/SUMMARY.md`. Root and crate READMEs are short
entry points linking here; `AGENTS.md` files contain agent instructions.

## Updating Documentation

Edit the appropriate chapter and add new chapters to `SUMMARY.md`. Cross-link
to other book chapters using relative Markdown paths. Keep current behavior,
agreed architecture, and open proposals clearly distinguished.

One topic, one owner. Each design topic is specified in exactly one chapter;
every other mention links to it instead of restating it. The ownership list is
in the documentation map in the root `AGENTS.md`. When a decision is reached,
record it in the [review chapter](review.md) as a one-line entry naming where it
is specified, and write the specification itself in the owning chapter. Do not
update several chapters "consistently" with the same rule - that is how the same
decision ends up in three places and drifts apart.

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

The repository root `AGENTS.md` owns this policy in full; it is not restated
here. In short: an agent needs explicit user permission before reading anything
under `archive/`, and ordinary documentation work does not grant it.

For the book specifically: the archive is not part of the book source, includes,
or search index, and `.ignore` excludes it from ordinary ripgrep searches. This
is an agent instruction, not a filesystem permission boundary.

