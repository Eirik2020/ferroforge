# FerroForge Renderer

`ferroforge-renderer` consumes the `ApplicationDefinition` emitted by the
existing FerroForge macros and writes a standalone RTIC firmware project. The
declaration remains the single source of truth: init and task bodies are
carried as original Rust syntax and emitted into the generated RTIC module.

The current backend targets `STM32F401RET6` with `stm32f4xx-hal`. It renders:

- Cargo dependencies with merged task features
- the Cortex-M target, probe runner, and `cargo-embed` configuration
- `memory.x` and linker setup
- the selected RTIC monotonic
- shared/local resources, init, software tasks, and hardware tasks
- `defmt-rtt` logging and `panic-probe`

`render_composed` applies a host-side `CompositionDefinition` to the embedded
source definition and writes a complete standalone firmware workspace:

```rust
ferroforge_renderer::render_composed(
    &ferroforge_example_embedded::APPLICATION,
    &COMPOSITION,
    RenderOptions {
        package_name: "nucleo-f401re-rtic",
        output_dir: &output_dir,
    },
)?;
```

The composition replaces embedded check defaults for dispatchers, priorities,
task configuration, and spawn bindings. Resource declarations, init, task
bodies, interrupt bindings, target identity, monotonic selection, and task
dependency requirements remain owned by the embedded source workspace.
Configuration values are emitted as ordinary task-prefixed constants such as
`BLINK_PERIOD_MS`; the final RTIC source contains no FerroForge configuration
module or runtime lookup.

The root host workspace invokes the renderer explicitly:

```text
cargo run -p ferroforge-example-composer
```

The resulting firmware is then checked or built independently:

```text
cd generated/nucleo-f401re
cargo check --all-targets
cargo build --release
```

Running from the firmware directory is significant: Cargo discovers the ARM
target and linker flags in that project's `.cargo/config.toml` relative to the
current directory, not to `--manifest-path`.

The generated workspace contains no FerroForge dependency, rendering build
script, or `OUT_DIR` include. Its `src/main.rs` is the complete RTIC app and is
directly indexed by Rust Analyzer.
