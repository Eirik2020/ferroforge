# Governing Page Review - Archived 2026-09-16

Archived from `docs/src/review.md` on 2026-09-16. This review's findings were
all applied to `docs/src/governing-requirements.md` the same day, leaving no
binding content to carry forward. Preserved verbatim as a record of why the
governing page changed.

---

### Governing Page Review (2026-09-16) - Spent

**Stale, pending archival.** All findings below were applied to the governing
page on 2026-09-16 and are superseded by it. This section is a historical record,
not current guidance. Do not act on it; archive it.

Review findings only; these do not select new requirements or change the current
marker discussion. No direct contradiction was found among the agreed component
roles, task binding rule, and project recognition behavior.

- **Marker scope needs precision (G7).** "Libraries" can be read as every Cargo
  dependency. The repair plan limits the marker requirement to crates selected
  for FerroForge library use. State that boundary on the governing page so
  ordinary Rust dependencies are not inadvertently included.
- **Single target authority is implicit (G2 and the closing paragraph).** The
  page assigns chip definitions to backends but does not explicitly preserve the
  agreed rule that firmware selects one authoritative target and all checking,
  generation, and build consumers use it. Add a short invariant; keep the
  detailed target fields in the repair plan.
- **G7 combines separate responsibilities.** CLI/project discovery and library
  identification are independently changeable requirements. Give the library
  marker its own clause or clearly separated subpoint so its scope and missing
  marker error are easy to find.
- **Requirements, open decisions, and status are interleaved.** G4, G7, the
  tree introduction, and the closing paragraphs repeat unresolved choices or
  implementation status. Keep one status statement and links to the review and
  plan, preserving the agreed deferral of broader library checks as a scope
  requirement. Group the compact tree explicitly by project, reusable libraries,
  and tool support; its current shared block is illustrative, not one required
  repository layout. This should also help retain the one-page limit.

These findings were applied to the governing page on 2026-09-16. Marker syntax,
other open choices, and implementation remain pending.
