# Tracking Task Dependencies in RTIC Task Definitions

## Goal

Each reusable task should declare which external Rust crates it requires, for example:

- `heapless`
- `sbus-rs`
- `fugit`

The task should not own crate versions. A central dependency registry should be the single source of truth for package names, versions, default features, and other Cargo metadata.

---

## Recommended Architecture

```text
Task definition
    |
    | declares DependencyId values
    v
Dependency requirements
    |
    v
Application composition
    |
    | collect + deduplicate
    v
Central dependency registry
    |
    v
Cargo dependency set
```

The task declares **what it needs**.

The registry defines **how that dependency is supplied**.

---

## 1. Define Typed Dependency IDs

Avoid dependency names as strings.

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DependencyId {
    Heapless,
    SbusRs,
    Fugit,
}
```

A typed ID gives compile-time checking:

```rust
DependencyId::Heapless
```

instead of an unchecked string:

```rust
"heaples"
```

---

## 2. Store Dependencies in the Task Definition

A task definition can contain a dependency slice:

```rust
pub struct TaskDefinition {
    pub name: &'static str,
    pub dependencies: &'static [DependencyId],
}
```

Example:

```rust
pub const SBUS_RX: TaskDefinition = TaskDefinition {
    name: "sbus_rx",
    dependencies: &[
        DependencyId::Heapless,
        DependencyId::SbusRs,
    ],
};
```

---

## 3. Use a Macro for Cleaner Declarations

A small `macro_rules!` macro can remove repetitive syntax:

```rust
macro_rules! dependencies {
    ($($dep:ident),* $(,)?) => {
        &[
            $(
                DependencyId::$dep,
            )*
        ]
    };
}
```

The task definition then becomes:

```rust
pub const SBUS_RX: TaskDefinition = TaskDefinition {
    name: "sbus_rx",
    dependencies: dependencies![
        Heapless,
        SbusRs,
    ],
};
```

This keeps task declarations compact while preserving typed dependency IDs.

---

## 4. Define the Dependency Registry Once

A second macro can define the dependency catalog and generate both the IDs and the resolver.

Example input:

```rust
define_dependencies! {
    Heapless => {
        package: "heapless",
        version: "0.9",
    },

    SbusRs => {
        package: "sbus-rs",
        version: "0.3",
    },

    Fugit => {
        package: "fugit",
        version: "0.3",
    },
}
```

The macro can generate:

```rust
pub enum DependencyId {
    Heapless,
    SbusRs,
    Fugit,
}
```

and:

```rust
impl DependencyId {
    pub const fn resolve(self) -> Dependency {
        match self {
            DependencyId::Heapless => Dependency {
                package: "heapless",
                version: "0.9",
            },

            DependencyId::SbusRs => Dependency {
                package: "sbus-rs",
                version: "0.3",
            },

            DependencyId::Fugit => Dependency {
                package: "fugit",
                version: "0.3",
            },
        }
    }
}
```

This gives one source of truth for dependency metadata.

---

## 5. Keep Versions Out of Task Definitions

Do not do this:

```rust
dependencies: [
    ("heapless", "0.9"),
    ("sbus-rs", "0.3"),
]
```

This duplicates version information across tasks.

Prefer:

```rust
dependencies: dependencies![
    Heapless,
    SbusRs,
]
```

Then resolve versions centrally.

This makes dependency upgrades much easier.

---

## 6. Dependency Features

If individual tasks need different Cargo features, model the task requirement separately from the base dependency.

For example:

```rust
pub struct DependencyRequirement {
    pub dependency: DependencyId,
    pub features: &'static [&'static str],
}
```

A task could then require:

```rust
DependencyRequirement {
    dependency: DependencyId::Serde,
    features: &["derive"],
}
```

When multiple tasks require the same crate, merge the feature sets.

Example:

```text
Task A -> serde ["derive"]
Task B -> serde ["alloc"]

Application requirement:
serde -> ["derive", "alloc"]
```

---

## 7. Collection During App Composition

The app macro or composition layer can collect dependencies from all selected tasks.

Conceptually:

```text
SbusRx
 |- Heapless
 `- SbusRs

Telemetry
 |- Heapless
 `- Fugit
```

After deduplication:

```text
Heapless
SbusRs
Fugit
```

The registry then resolves those IDs to Cargo dependency metadata.

---

## 8. Important Cargo Limitation

A Rust procedural macro cannot dynamically add new crates to the application's `Cargo.toml` during compilation.

Cargo resolves the dependency graph before procedural macro expansion.

Therefore this will not work as a normal build flow:

```text
cargo build
    |
    v
task macro discovers sbus-rs
    |
    v
macro edits Cargo.toml
    |
    X
```

The required dependencies must already be visible to Cargo.

---

## 9. Recommended Solution for Macro-Only Builds

Make the framework crate own the optional external dependencies.

Example:

```toml
[features]
sbus = ["dep:sbus-rs"]
heapless = ["dep:heapless"]

[dependencies]
sbus-rs = {
    version = "0.3",
    optional = true
}

heapless = {
    version = "0.9",
    optional = true
}
```

The application selects framework features:

```toml
[dependencies]
ferrowasp = {
    version = "...",
    features = [
        "sbus",
        "heapless",
    ]
}
```

The framework can re-export dependencies:

```rust
pub use heapless;
pub use sbus_rs;
```

Generated code can then use:

```rust
ferrowasp::heapless::Vec
```

instead of requiring the application crate to reference `heapless` directly.

---

## 10. Suggested Final Model

Task definition:

```rust
#[ferro_task(
    dependencies = [
        Heapless,
        SbusRs,
    ]
)]
async fn sbus_rx(cx: Context) {
    // ...
}
```

Internal representation:

```rust
TaskDefinition {
    name: "sbus_rx",
    dependencies: &[DependencyId::Heapless, DependencyId::SbusRs],
}
```

Central registry:

```rust
define_dependencies! {
    Heapless => {
        package: "heapless",
        version: "0.9",
    },

    SbusRs => {
        package: "sbus-rs",
        version: "0.3",
    },
}
```

Application composition:

```text
Collect dependencies from tasks
        |
        v
Deduplicate IDs
        |
        v
Merge requested features
        |
        v
Validate that required framework/Cargo features are enabled
        |
        v
Generate the RTIC application
```

---

## Recommendation

Use three layers:

1. **Task definition**  
   Declares typed dependency requirements.

2. **Dependency registry**  
   Defines package names, versions, default features, and other Cargo metadata.

3. **Application macro**  
   Collects requirements, deduplicates them, merges features, and validates that the required dependencies are enabled.

This keeps task definitions reusable and version-independent while still allowing `cargo build` to produce the complete RTIC application without a separate code-generator step.
