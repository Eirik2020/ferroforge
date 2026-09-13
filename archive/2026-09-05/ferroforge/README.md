# FerroForge

FerroForge is an RTIC-like application model for defining and validating
embedded task graphs. It provides `#[task]`,
`dependency_registry!`, and `app!` macros, typed spawn bindings,
shared-resource declarations, priorities, dispatcher validation,
hardware-interrupt metadata, compile-time task configuration, and typed task
dependency requirements.

FerroForge is a model rather than a firmware executor: spawning and monotonic
delays are mocked, while firmware initialization and task bodies are retained
for the embedded side of the declaration.

## Target identity

Firmware-oriented apps declare their exact MCU and HAL once:

```rust
target = {
    mcu = STM32F401RET6,
    hal = stm32f4xx_hal,
},
```

For `STM32F401RET6`, FerroForge records the Rust compilation target, HAL and
`rtic-monotonics` features, and flash/RAM origins and sizes. The target model
is intentionally limited to this build and linker information; it does not
include board metadata or a peripheral and pin database.

HAL imports can stay outside `app!` and be marked as firmware-only. The item is
omitted from the host metadata build, retained as renderer input, and compiled
by the ARM declaration check and generated firmware:

```rust
#[ferroforge::firmware]
use stm32f4xx_hal::{
    gpio::{Output, PA5, PushPull},
    pac::TIM2,
    timer::CounterHz,
};
```

## Monotonics

An RTIC-shaped app can optionally declare one named monotonic backed by either
SysTick or a hardware timer:

```rust
app! {
    target = {
        mcu = STM32F401RET6,
        hal = stm32f4xx_hal,
    },
    dispatchers = [USART1],
    monotonic = Mono {
        source = Timer(TIM5),
        tick_hz = 1_000,
    },

    mod app {
        // Resources, init, and tasks...
    }
}
```

Use `source = SysTick` for a SysTick-backed monotonic. A hardware timer is
reserved for the monotonic and cannot also be used as a dispatcher, hardware
task binding, or local resource. Tasks use the source-independent
`Mono::delay(duration)` API.

## Task dependencies

Reusable tasks declare which external crates they need without choosing crate
versions. An application owns those versions in one registry. For example,
`src/dependencies.rs` can contain:

```rust
use ferroforge::dependency_registry;

dependency_registry! {
    Heapless => {
        package = "heapless",
        version = "0.9",
        default_features = false,
        features = [],
    },
    Serde => {
        package = "serde",
        version = "1",
        default_features = false,
        features = [],
    },
}
```

The task refers only to typed registry IDs and may request additional features:

```rust
#[task(
    local = [queue],
    dependencies = [
        Heapless,
        Serde(features = ["derive"]),
    ],
)]
fn report(_cx: report::Context) {}
```

Reference the registry once, before the target declaration:

```rust
mod dependencies;

app! {
    dependency_registry = crate::dependencies,
    target = {
        mcu = STM32F401RET6,
        hal = stm32f4xx_hal,
    },
    dispatchers = [USART1],

    mod app {
        // Resources, init, and tasks...
    }
}
```

The app exposes `DEPENDENCY_REGISTRY`, `TASK_DEPENDENCIES`, and
`TASK_DEPENDENCY_COUNT`. Compilation fails when a selected task names an ID
that the registry does not define, when a task declares dependencies without a
registry, or when duplicate IDs, packages, or features are declared.

The renderer groups these requirements by ID, unions task features with the
registry's base features, and emits them into the generated firmware
manifest. Target HAL, RTIC, monotonic, logging transport, and panic crates
remain backend dependencies rather than task-owned requirements.

## Rendering firmware

`app!` exposes one renderer-facing `APPLICATION` definition. It contains the
structured target, resource, dependency, and default scheduling data together
with the original Rust source for init and task functions. Its ARM expansion
uses mock RTIC contexts to type-check the actual firmware imports, resource
types, init body, and task bodies. User code such as HAL peripheral
initialization is preserved rather than inferred.

This repository separates the application into three Cargo workspaces:

- The root host workspace contains FerroForge, its renderer, and `composer`.
  The composer owns `COMPOSITION` and combines it with the embedded
  `APPLICATION`.
- `embedded` is an ARM `no_std` library containing HAL-specific imports,
  resources, init code, task implementations, and the exported `APPLICATION`.
- `generated/nucleo-f401re` is the standalone output workspace. Its
  `src/main.rs` is ordinary generated RTIC source that Rust Analyzer can index.

Check the embedded declarations independently:

```text
cd embedded
cargo check --lib
```

Render from the host workspace explicitly:

```text
cargo run -p ferroforge-example-composer
```

This writes the complete firmware source, manifest, target configuration, and
linker memory layout into `generated/nucleo-f401re`. It does not use a firmware
build script or Cargo `OUT_DIR`.

Then check or build the final application:

```text
cd generated/nucleo-f401re
cargo check --all-targets
cargo build --release
```

Run these commands from the shown directories so Cargo discovers each
project's `.cargo/config.toml` and selects `thumbv7em-none-eabihf`.

The current backend supports `STM32F401RET6`, `stm32f4xx-hal`, an optional
SysTick or STM32 timer monotonic, and `defmt` over RTT with `panic-probe`.

## Using it from another project

Until the crates are published, add the runtime crate by path:

```toml
[dependencies]
ferroforge = { path = "../rtic-app-builder-v4/ferroforge" }
```

The runtime crate re-exports its procedural macros, so application code only
needs one dependency:

```rust
use ferroforge::{app, task};

#[task]
async fn blink(_cx: blink::Context) {}

app! {
    blink {
        priority = 1,
    }
}
```

After publication, the dependency can use a version instead:

```toml
[dependencies]
ferroforge = "0.1"
```
