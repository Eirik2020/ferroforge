# Rust Analyzer diagnostics fixture

This package is intentionally invalid. It verifies that the invalid resource
method and wrong concrete value diagnostics produced through
`#[ferroforge::task]` point at the authored task body rather than only at
generated macro code. Keep it outside the root workspace and run it through the
ignored `standalone_check` integration test documented in the mdBook.

The always-run rustc regression harness separately covers the shared-lock
lifetime error, which Rust Analyzer 1.98.0 does not report for this expansion.
