# FerroForge example

This repository demonstrates an RTIC application split into three independent
Cargo workspaces:

- The root host workspace contains FerroForge, the renderer, and the
  application composer.
- `embedded` contains the ARM-checked HAL initialization, resources, and task
  implementations.
- `generated/nucleo-f401re` contains the rendered, standalone RTIC firmware.

Render the firmware from the repository root:

```text
cargo run -p ferroforge-example-composer
```

Check the embedded declarations:

```text
cd embedded
cargo check --lib
```

Check and build the rendered firmware:

```text
cd generated/nucleo-f401re
cargo check --all-targets
cargo build --release
```

Run ARM commands from their project directories so Cargo discovers the local
`.cargo/config.toml`. VS Code links all three workspaces, allowing Rust Analyzer
to check the mock declarations and provide full language support in the
generated `src/main.rs`.
