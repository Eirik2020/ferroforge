# FerroForge

FerroForge composes reusable [RTIC](https://rtic.rs) tasks into firmware. A task
is an ordinary function in its own crate; a firmware selects it with
`ferroforge::app!`, which expands into a real `#[rtic::app]`.

```text
cargo install ferroforge-cli
ferroforge new hello --chip stm32f401re
cd hello && ferroforge run
```

Installing and using it: [`ferroforge-cli`](ferroforge-cli/README.md). Writing
tasks and applications: [`ferroforge`](ferroforge/README.md).

## Developing FerroForge

The [FerroForge book](docs/src/introduction.md) is the single active documentation
set. Start with its [table of contents](docs/src/SUMMARY.md) or the
[design decisions and remaining implementation work](docs/src/review.md).

Build or serve it from the repository root:

```text
mdbook build docs
mdbook serve docs --hostname 127.0.0.1 --port 3000
```

The HTML entry point is `docs/book/index.html`. Legacy documents are preserved
under `archive/`; agents require explicit user permission to access them, as
specified in the repository `AGENTS.md`.

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at
your option.
