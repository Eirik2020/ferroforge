# FerroForge Task/App Proc-Macro Design

FerroForge implements a small RTIC-like application model with:

- reusable `#[task]` declarations;
- typed spawn aliases;
- an RTIC-like `cx.spawn.<alias>() -> Result<_, _>` API;
- an `app!` macro that binds spawn aliases to concrete tasks;
- generated enums and binding metadata with no string identifiers.

The user-facing syntax is:

```rust
#[task(priority = 1, spawn = [])]
async fn task_a(_cx: task_a::Context) {}

#[task(priority = 1, spawn = [])]
async fn task_b(_cx: task_b::Context) {}

#[task(priority = 2, spawn = [blink, report])]
async fn task_c(cx: task_c::Context) {
    cx.spawn.blink().unwrap();

    if cx.spawn.report().is_err() {
        // Handle spawn failure.
    }
}

app! {
    task_a,
    task_b,

    task_c {
        spawn = {
            blink  => task_a,
            report => task_b,
        }
    },
}
```

The important relationship is:

```text
task_c declaration:

    blink
    report
      │
      │ app composition
      ▼
    blink  ─────→ task_a
    report ─────→ task_b
```

There are no string identifiers in the generated metadata.

## 1. Workspace

```text
ferroforge-workspace/
│
├── Cargo.toml
│
├── ferroforge/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
│
├── ferroforge-macros/
│   ├── Cargo.toml
│   └── src/
│       └── lib.rs
│
└── example/
    ├── Cargo.toml
    └── src/
        └── main.rs
```

Root `Cargo.toml`:

```toml
[workspace]
resolver = "2"

members = [
    "ferroforge",
    "ferroforge-macros",
    "example",
]
```

## 2. `ferroforge`

This is the small runtime/API crate.

`ferroforge/Cargo.toml`:

```toml
[package]
name = "ferroforge"
version = "0.1.0"
edition = "2024"

[dependencies]
ferroforge-macros = { path = "../ferroforge-macros" }
```

`ferroforge/src/lib.rs`:

```rust
pub use ferroforge_macros::{app, task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    QueueFull,
}
```

For now, `SpawnError` only exists to give the task body an RTIC-like interface.

## 3. Proc-macro crate

`ferroforge-macros/Cargo.toml`:

```toml
[package]
name = "ferroforge-macros"
version = "0.1.0"
edition = "2024"

[lib]
proc-macro = true

[dependencies]
proc-macro2 = "1"
quote = "1"
syn = { version = "2", features = ["full"] }
heck = "0.5"
```

`ferroforge-macros/src/lib.rs`:

```rust
use std::collections::HashSet;

use heck::ToUpperCamelCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    braced, bracketed,
    parse::{Parse, ParseStream},
    parse_macro_input,
    punctuated::Punctuated,
    Error, Ident, ItemFn, LitInt, Result, Token,
};

// ============================================================
// Utilities
// ============================================================

fn variant_name(ident: &Ident) -> Ident {
    let name = ident.to_string().to_upper_camel_case();

    format_ident!(
        "{}",
        name,
        span = ident.span()
    )
}

// ============================================================
// #[task(...)]
// ============================================================

struct TaskArgs {
    priority: Option<LitInt>,
    spawns: Vec<Ident>,
}

impl Parse for TaskArgs {
    fn parse(input: ParseStream) -> Result<Self> {
        let mut priority = None;
        let mut spawns = Vec::new();

        while !input.is_empty() {
            let key: Ident = input.parse()?;

            input.parse::<Token![=]>()?;

            match key.to_string().as_str() {
                "priority" => {
                    priority = Some(input.parse()?);
                }

                "spawn" => {
                    let content;

                    bracketed!(content in input);

                    let items =
                        Punctuated::<Ident, Token![,]>::parse_terminated(
                            &content,
                        )?;

                    spawns.extend(items);
                }

                _ => {
                    return Err(Error::new(
                        key.span(),
                        "expected `priority` or `spawn`",
                    ));
                }
            }

            if input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            } else if !input.is_empty() {
                return Err(input.error("expected `,`"));
            }
        }

        Ok(Self {
            priority,
            spawns,
        })
    }
}

#[proc_macro_attribute]
pub fn task(
    attr: TokenStream,
    item: TokenStream,
) -> TokenStream {
    let args = parse_macro_input!(attr as TaskArgs);
    let function = parse_macro_input!(item as ItemFn);

    match expand_task(args, function) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_task(
    args: TaskArgs,
    function: ItemFn,
) -> Result<TokenStream2> {
    let task_name = &function.sig.ident;
    let visibility = &function.vis;

    let priority = args
        .priority
        .unwrap_or_else(|| LitInt::new("1", task_name.span()));

    // Check duplicate spawn aliases.

    let mut aliases = HashSet::new();

    for spawn in &args.spawns {
        let name = spawn.to_string();

        if !aliases.insert(name) {
            return Err(Error::new(
                spawn.span(),
                "duplicate spawn alias",
            ));
        }
    }

    let spawn_count = args.spawns.len();

    // Generate enum variants such as Blink and Report.

    let spawn_variants = args
        .spawns
        .iter()
        .map(variant_name);

    // Generate RTIC-like spawn methods.

    let spawn_methods = args.spawns.iter().map(|spawn| {
        quote! {
            #[inline]
            pub fn #spawn(
                &self
            ) -> ::core::result::Result<
                (),
                ::ferroforge::SpawnError
            > {
                ::core::result::Result::Ok(())
            }
        }
    });

    Ok(quote! {
        #function

        #visibility mod #task_name {
            #[derive(
                Debug,
                Clone,
                Copy,
                PartialEq,
                Eq
            )]
            pub enum Spawn {
                #(
                    #spawn_variants,
                )*
            }

            #[derive(
                Debug,
                Clone,
                Copy,
                Default
            )]
            pub struct SpawnHandle;

            impl SpawnHandle {
                #(
                    #spawn_methods
                )*
            }

            #[derive(
                Debug,
                Clone,
                Copy,
                Default
            )]
            pub struct Context {
                pub spawn: SpawnHandle,
            }

            pub const PRIORITY: u8 = #priority;

            pub const SPAWN_COUNT: usize = #spawn_count;
        }
    })
}

// ============================================================
// app! { ... }
// ============================================================

struct SpawnBindingInput {
    alias: Ident,
    target: Ident,
}

impl Parse for SpawnBindingInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let alias = input.parse()?;

        input.parse::<Token![=>]>()?;

        let target = input.parse()?;

        Ok(Self {
            alias,
            target,
        })
    }
}

struct AppTask {
    name: Ident,
    bindings: Vec<SpawnBindingInput>,
}

impl Parse for AppTask {
    fn parse(input: ParseStream) -> Result<Self> {
        let name: Ident = input.parse()?;

        let mut bindings = Vec::new();

        // Simple task:
        //
        // task_a

        if !input.peek(syn::token::Brace) {
            return Ok(Self {
                name,
                bindings,
            });
        }

        // Configured task:
        //
        // task_c {
        //     spawn = {
        //         blink => task_a,
        //     }
        // }

        let body;

        braced!(body in input);

        let key: Ident = body.parse()?;

        if key != "spawn" {
            return Err(Error::new(
                key.span(),
                "expected `spawn`",
            ));
        }

        body.parse::<Token![=]>()?;

        let spawn_body;

        braced!(spawn_body in body);

        let parsed_bindings =
            Punctuated::<SpawnBindingInput, Token![,]>::parse_terminated(
                &spawn_body,
            )?;

        bindings.extend(parsed_bindings);

        if !body.is_empty() {
            return Err(
                body.error("unexpected tokens in task configuration")
            );
        }

        Ok(Self {
            name,
            bindings,
        })
    }
}

struct AppInput {
    tasks: Vec<AppTask>,
}

impl Parse for AppInput {
    fn parse(input: ParseStream) -> Result<Self> {
        let tasks =
            Punctuated::<AppTask, Token![,]>::parse_terminated(
                input,
            )?;

        Ok(Self {
            tasks: tasks.into_iter().collect(),
        })
    }
}

#[proc_macro]
pub fn app(input: TokenStream) -> TokenStream {
    let app = parse_macro_input!(input as AppInput);

    match expand_app(app) {
        Ok(output) => output.into(),
        Err(error) => error.to_compile_error().into(),
    }
}

fn expand_app(app: AppInput) -> Result<TokenStream2> {
    // Validate task names.

    let mut task_names = HashSet::new();

    for task in &app.tasks {
        let name = task.name.to_string();

        if !task_names.insert(name) {
            return Err(Error::new(
                task.name.span(),
                "task appears more than once in app",
            ));
        }
    }

    // Validate bindings.

    for task in &app.tasks {
        let mut aliases = HashSet::new();

        for binding in &task.bindings {
            let alias = binding.alias.to_string();

            if !aliases.insert(alias) {
                return Err(Error::new(
                    binding.alias.span(),
                    "spawn alias is bound more than once",
                ));
            }

            if !task_names.contains(
                &binding.target.to_string()
            ) {
                return Err(Error::new(
                    binding.target.span(),
                    "spawn target is not part of this app",
                ));
            }
        }
    }

    // Task enum.

    let task_variants = app.tasks.iter().map(|task| {
        variant_name(&task.name)
    });

    // Spawn source enum.

    let spawn_source_variants =
        app.tasks
            .iter()
            .filter(|task| !task.bindings.is_empty())
            .map(|task| {
                let task_name = &task.name;
                let variant = variant_name(task_name);

                quote! {
                    #variant(#task_name::Spawn)
                }
            });

    // Spawn bindings.

    let bindings =
        app.tasks
            .iter()
            .flat_map(|task| {
                let source_task = &task.name;
                let source_variant =
                    variant_name(source_task);

                task.bindings.iter().map(
                    move |binding| {
                        let alias_variant =
                            variant_name(&binding.alias);

                        let target_variant =
                            variant_name(&binding.target);

                        quote! {
                            SpawnBinding {
                                source:
                                    SpawnSource::#source_variant(
                                        #source_task::Spawn::#alias_variant
                                    ),

                                target:
                                    Task::#target_variant,
                            }
                        }
                    },
                )
            });

    // Compile-time spawn count checks.

    let spawn_count_checks =
        app.tasks.iter().map(|task| {
            let task_name = &task.name;
            let count = task.bindings.len();

            quote! {
                const _: [
                    ();
                    #task_name::SPAWN_COUNT
                ] = [(); #count];
            }
        });

    Ok(quote! {
        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq
        )]
        enum Task {
            #(
                #task_variants,
            )*
        }

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq
        )]
        enum SpawnSource {
            #(
                #spawn_source_variants,
            )*
        }

        #[derive(
            Debug,
            Clone,
            Copy,
            PartialEq,
            Eq
        )]
        struct SpawnBinding {
            source: SpawnSource,
            target: Task,
        }

        const SPAWN_BINDINGS: &[SpawnBinding] = &[
            #(
                #bindings,
            )*
        ];

        #(
            #spawn_count_checks
        )*
    })
}
```

## 4. Original single-crate example

These snippets preserve the original conceptual design. They are illustrative;
the active example is now split between `embedded` and
`generated/nucleo-f401re` as described in `ferroforge/README.md`.

Illustrative `Cargo.toml`:

```toml
[package]
name = "example"
version = "0.1.0"
edition = "2024"

[dependencies]
ferroforge = { path = "../ferroforge" }
```

Illustrative `src/main.rs`:

```rust
use ferroforge::{
    app,
    task,
    SpawnError,
};

// ============================================================
// Reusable task definitions
// ============================================================

#[task(
    priority = 1,
    spawn = []
)]
async fn task_a(
    _cx: task_a::Context
) {
    // Task A implementation.
}

#[task(
    priority = 1,
    spawn = []
)]
async fn task_b(
    _cx: task_b::Context
) {
    // Task B implementation.
}

#[task(
    priority = 2,
    spawn = [
        blink,
        report,
    ]
)]
async fn task_c(
    cx: task_c::Context
) {
    cx.spawn
        .blink()
        .unwrap();

    match cx.spawn.report() {
        Ok(()) => {}

        Err(SpawnError::QueueFull) => {
            // Handle spawn error.
        }
    }
}

// ============================================================
// Application composition
// ============================================================

app! {
    task_a,
    task_b,

    task_c {
        spawn = {
            blink => task_a,
            report => task_b,
        }
    },
}

// ============================================================
// Example
// ============================================================

fn main() {
    println!("{:#?}", SPAWN_BINDINGS);
}
```

## 5. Approximate task expansion

The task:

```rust
#[task(
    priority = 2,
    spawn = [blink, report]
)]
async fn task_c(cx: task_c::Context) {
    cx.spawn.blink().unwrap();
}
```

produces approximately:

```rust
async fn task_c(
    cx: task_c::Context
) {
    cx.spawn.blink().unwrap();
}

mod task_c {
    pub enum Spawn {
        Blink,
        Report,
    }

    pub struct SpawnHandle;

    impl SpawnHandle {
        pub fn blink(
            &self
        ) -> Result<(), SpawnError> {
            Ok(())
        }

        pub fn report(
            &self
        ) -> Result<(), SpawnError> {
            Ok(())
        }
    }

    pub struct Context {
        pub spawn: SpawnHandle,
    }

    pub const PRIORITY: u8 = 2;
    pub const SPAWN_COUNT: usize = 2;
}
```

## 6. Approximate app expansion

The application:

```rust
app! {
    task_a,
    task_b,

    task_c {
        spawn = {
            blink => task_a,
            report => task_b,
        }
    },
}
```

produces approximately:

```rust
enum Task {
    TaskA,
    TaskB,
    TaskC,
}

enum SpawnSource {
    TaskC(task_c::Spawn),
}

struct SpawnBinding {
    source: SpawnSource,
    target: Task,
}

const SPAWN_BINDINGS: &[SpawnBinding] = &[
    SpawnBinding {
        source: SpawnSource::TaskC(
            task_c::Spawn::Blink
        ),
        target: Task::TaskA,
    },

    SpawnBinding {
        source: SpawnSource::TaskC(
            task_c::Spawn::Report
        ),
        target: Task::TaskB,
    },
];
```

The code generator therefore receives the typed graph:

```text
TaskC::Blink  ─────→ TaskA
TaskC::Report ─────→ TaskB
```

rather than string mappings.

## 7. Compile-time checks

An invalid alias:

```rust
task_c {
    spawn = {
        blink => task_a,
        wrong => task_b,
    }
}
```

causes generation of:

```rust
task_c::Spawn::Wrong
```

which does not exist, so compilation fails.

If a binding is missing:

```rust
task_c {
    spawn = {
        blink => task_a,
    }
}
```

the generated count check fails:

```rust
const _: [(); task_c::SPAWN_COUNT] = [(); 1];
```

because `task_c::SPAWN_COUNT == 2`.

If a binding targets a task that is not part of the app:

```rust
blink => some_other_task
```

the `app!` proc macro reports that the spawn target is not part of the application.

This gives the model:

- RTIC-like task bodies;
- reusable task definitions;
- typed spawn aliases;
- app-level spawn binding;
- no string-based task identity;
- generated metadata suitable for a later RTIC code-generation stage.
