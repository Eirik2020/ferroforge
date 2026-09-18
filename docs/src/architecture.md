# Architecture

FerroForge composes reusable RTIC tasks and project-owned initialization into a
real RTIC firmware application. Nothing is generated into a separate project and
nothing is copied between crates: a reusable task is an ordinary Rust function
in its own crate, and the firmware's `app!` expands in place into a real
`#[rtic::app]` that calls it.

The [governing requirements](governing-requirements.md) take precedence over
anything here.

## The Shape

Three pieces, and only the middle one is FerroForge's:

```text
task crate                     firmware crate                 the binary
  #[ferroforge::task]            ferroforge::app! { .. }        one cargo build
  async fn blink(cx)       ->    #[rtic::app] mod app      ->   thumbv7em ELF
  real trait bounds              init, Shared, Local
                                 one adapter per instance
```

A task crate checks on its own with `cargo check`. The firmware crate is both
what the author writes and what links. There is no host renderer, no generated
project, and no second Cargo invocation.

## Why Bodies Are Not Moved

An earlier design transplanted task bodies into a generated application,
rewriting their references as it went. That required a mock RTIC layer so tasks
could compile standalone, and a renderer that rewrote configuration paths, spawn
aliases, monotonic calls, logging arguments and module paths. Whole categories
of ordinary Rust - macro token streams, relative imports, cross-module
references - were unsupported because the rewriter could not follow them.

Calling the task instead removes all of it. The body compiles once, in place,
against real traits:

| Authored | Resolves to |
| --- | --- |
| `cx.local.led` | `&mut Led` where `Led: StatefulOutputPin` |
| `cx.shared.enabled.lock(..)` | a generic bounded by `rtic::Mutex<T = bool>` - RTIC's own proxy |
| `cx.spawn.report(v)` | a method over a real `Fn(u32) -> Result<(), u32>` |
| `CONFIG.PERIOD_MS` | an associated const on the firmware's config type |
| `Mono::delay(..)` | a type parameter bounded by `rtic_monotonics::Monotonic` |

Because those are real types, independent checking has nothing left to diverge
from: the compiler that checks the task standalone is the one that compiles it
into the firmware.

## Task Authoring

A reusable task declares what it requires and writes an ordinary RTIC-shaped
body. These forms are agreed; see the [decision record](review.md) for where
each was settled.

- Resource-keyed bounds, `bounds = [led: StatefulOutputPin]`, alongside
  `local = [led]` or `shared = [led]`. Concrete resources carry inline types,
  such as `count: u32`. There is no `impl` marker and no author-visible generic
  placeholder.
- RTIC-familiar access: `cx.local.<name>`, and `cx.shared.<name>.lock(..)` for
  shared resources. Both categories are in scope, including state that persists
  across invocations.
- Configuration is read as `CONFIG.FIELD`, declared `config = [period_ms: u32]`.
- Incoming inputs are ordinary parameters after the context. Outgoing calls use
  inline aliases, `spawn = [report(value: u32)]`, with RTIC 2 result shapes:
  `()` for no inputs, the value for one, a tuple for several.
- The monotonic is named as imported, `monotonic = Mono`. The initial profile
  is 1 kHz with `u32` time values, which the firmware satisfies by declaring a
  matching monotonic of its own.
- Related tasks share a source module with imports declared once at module
  scope. A task crate may hold several modules.

A resource need not be bounded at all. A concrete inline type is the plainer
choice and often the right one: a task that must touch a peripheral's interrupt
flags cannot be written against `embedded-hal`, because no portable trait models
them, so it names the HAL type the firmware will hold and its body is the handler
anyone would write by hand. Such a task is reusable across projects using that
HAL rather than across HALs, which is what G1 asks of it.

Portability has a second, sharper limit, and it is the ecosystem's rather than
this design's: a bound is only as portable as the HALs implementing it.
`tasks/blinky` bounds on `embedded-hal` 1.0, and `stm32h7xx-hal` 0.16 still
implements only 0.2, so that task cannot be selected on an STM32H7 at all - while
a task naming no HAL, like `report`, is selected there unchanged. "Portable
across all hardware" means across the hardware whose HALs have migrated.

Bounds earn their place when a definition must serve resources of different
types. They are not free: a bound has to be nameable and satisfiable by the type
a firmware actually holds, and inventing a trait to bridge that gap buys
generality at the cost of a layer between the author and the HAL. See
[dependencies](dependencies.md) for the trade, and for how a HAL-specific crate
names a chip without choosing one.

Task kind follows the signature, exactly as in RTIC: an `async fn` is a software
task, a plain `fn` is a hardware task. A hardware task takes only its context
and returns `()`, because an interrupt has no caller to supply inputs and the
handler has to return. The definition never names an interrupt - composition
binds one, so the same handler serves different interrupts in different
firmware.

## Firmware Composition

The firmware authors one `app!` containing its target choices, its `Shared`
and `Local` resources, its handwritten init, and one declaration per task
instance. Init is written here and never moves, so RTIC generates the real
`init::Context` and the real `spawn` functions and Rust checks the body against
them. No checking interfaces are generated.

Every task a firmware runs is declared here, one by one, including tasks that
only work as a set. A library cannot supply declarations: `#[rtic::app]` parses
`mod app` before any inner macro expands, so a generated `#[task]` is never seen
and RTIC reports "cannot find attribute `task` in this scope". Where such tasks
share state, the library can model it as one type, so the firmware binds one
resource rather than several that could be wired apart.

Its header is RTIC's, parsed the way RTIC parses its own - a loop, so order does
not matter, with a default for everything a firmware need not say:

| Argument | |
| --- | --- |
| `device` | required; the PAC, as RTIC's |
| `dispatchers` | optional, empty by default, passed through unchanged |
| `peripherals` | optional, passed through only when stated |
| `monotonic` | optional; names a monotonic the firmware declared itself |

Which interrupts are free to dispatch software tasks depends on the
application's own peripheral use, so the list is the author's and `app!` adds
nothing to it. RTIC checks the choice: a dispatcher that is also bound, or too
few for the priorities in use, is an error on the authored line.

Only `monotonic` is not RTIC's, and it exists because call-through needs it: a
task crate is generic over the clock, so the adapter has to be handed a type.
The monotonic is declared outside `app!` exactly as an RTIC user declares one,
and `init` starts it the same way. An application that needs no clock names none,
and a task that does need one then fails against a type called
`NoMonotonicDeclared`.

Each instance declaration names the definition it comes from and maps the
reusable names onto the firmware's own:

```rust,ignore
#[task(
    from = blink,
    priority = 1,
    local = [led = status_led, count = blink_count],
    shared = [enabled = blink_enabled],
    config = [period_ms: u32 = 500],
    spawn = [report = telemetry],
)]
async fn status_blink(cx: status_blink::Context) -> !;
```

`app!` emits a config impl and an adapter that constructs the definition's
own context and calls it. Every path it emits is a real Rust path, so a wrong
definition, binding, type or interrupt is an ordinary compile error on the
authored line.

The macro reads nothing but its own input. That is a deliberate constraint:
configuration is an associated-const trait and every context carries a monotonic
slot, so the adapter is fully inferable and needs no knowledge of the
definition's generics. Requiring that knowledge would mean parsing the task
crate, which is the coupling this design exists to remove.

Configuration values state their type, `period_ms: u32 = 500`, because an `impl`
must name it. A mismatch fails against the definition's trait.

## What Checking Guarantees

Rust and RTIC do the checking; FerroForge adds no validation layer of its own.
If RTIC does not check something, FerroForge does not check it either. An
interrupt bound to a peripheral other than the one its task drives builds in
plain RTIC, so it builds here.

- A task crate checks independently against its declared bounds.
- The firmware crate checks init, resource bindings, configuration types, spawn
  signatures and interrupt names in one compile.
- Naming an interrupt the chip does not have is a compile error, because RTIC
  resolves `binds` against the selected device's own enum.

Add inexpensive early checks where they give useful feedback, but the final
target build remains the verification step. See
[incremental coverage](#incremental-coverage).

## Incremental Coverage

Support the operations real examples need, then extend as uses appear.
Exhaustive monotonic or peripheral coverage is not a prerequisite, and a clean
editor is not a replacement for the final build. Keep host simulation separate
from compile-time support; it is a distinct optional objective.

## Core Design Principle

Rust is the source of truth for task behaviour, initialization, types and target
API usage. FerroForge generates only what needs whole-application knowledge:
instantiating reusable tasks, binding resources, resolving configuration and
spawn aliases, and assembling the `#[rtic::app]` module.
