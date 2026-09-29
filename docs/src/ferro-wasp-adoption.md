# Adopting FerroForge in ferro-wasp

The [decision record](review.md) says FerroForge co-evolves with the
[ferro-wasp](https://github.com/Eirik2020/ferro-wasp) flight controller. This
chapter is the plan for that: what ferro-wasp needs from FerroForge before it can
adopt it, and the order to migrate in. It was written against FerroForge 0.2.0
and ferro-wasp `370460b`, and last checked against FerroForge 0.4.0 and
ferro-wasp `b3721aa`.

Short answer: the macros fit ferro-wasp's architecture, and so does the CLI.
The macros are usable without the CLI, so adoption started with the macro
release and the CLI is no longer what holds the rest back.

## What Already Fits

- ferro-wasp's `systick_monotonic!(Mono, 1000)` counts in `u64` with
  `systick-64bit`, as FerroForge 0.4 requires of any rate
  ([task authoring](architecture.md#task-authoring)). Its own APIs take those
  `u64` timestamps rather than narrowing them, so they do not wrap; only its
  fixed log and status formats keep `u32`. Its `u32` durations take
  `u64::from`, and bare literals take `u64` in an app that imports both the
  HAL's prelude and the monotonic's.
- `app!` passes `init`, `#[shared]`, `#[local]`, `dispatchers` and
  `peripherals = true` through unchanged, so the Foxeer F405 V2 `init` - about
  580 lines, including `#[init(local = [..])]` DMA buffers - does not change.
- Every task shape it uses is supported: interrupt-bound hardware tasks
  (`control_loop` on TIM4, the DMA completion handlers), diverging async
  consumers, and tasks locking several shared resources at once.
- `ferrowasp-tasks` is already a HAL-free `no_std` crate under
  `forbid(unsafe_code)`, the shape a task crate should have, and the
  expansion emits no `unsafe`.
- Both of its chips, the STM32F405RG (Foxeer F405 V2, FCU3) and STM32F401RE
  (NUCLEO bring-up), have backends.

## Macro Gaps

None remain. Three were settled, all specified in
[task authoring](architecture.md#task-authoring):

- a definition declares task-local initial values as RTIC does, so
  `flash_manager_task`'s list moves into its definition unchanged;
- configuration is read as `CONFIG::FIELD`, which works inside `defmt::info!`
  and every other macro call;
- a shared resource can be `#[lock_free]`, so the UART receive handlers sharing
  `uart1_rx`, `uart2_rx` and `uart4_rx` can become definitions.

## CLI Gaps

None of these block using the macros alone, and none of them is now undecided.

- **HAL source.** ferro-wasp pins `stm32f4xx-hal` to a git revision that is
  version 0.22.1, and the F405 backend selects 0.23.0 from crates.io.
  `[patch.crates-io]` cannot bridge a semver-incompatible version, so a firmware
  says where a platform crate comes from, specified in [where a platform crate
  comes from](workflow.md#where-a-platform-crate-comes-from). ferro-wasp keeps
  its revision and does not have to move to 0.23.0 to adopt the CLI. The HAL
  features it enables (`rtic2`, `defmt`, `usb_fs`, `uart4`) were never the
  problem: `ferrowasp-stm32f4` already enables them, Cargo unifies features, and
  a source may add features of its own besides.
- **Probe and environment settings.** `sync` rewrites `.cargo/config.toml`, so
  the firmware declares what goes in it: `probe-command`, `probe-args` and `env`,
  specified with the other settings in [the CLI](workflow.md#the-cli), with an
  `embed` table for what `cargo embed` reads beyond the probe. Against the
  Foxeer app this reproduces its hand-written runner argument for argument and
  every setting of its `Embed.toml`.
  `CHIPSERIE` needs no route at all: nothing in ferro-wasp reads it.
- **Linker search path.** The generated config adds `-L.`, and ferro-wasp's
  `build.rs` copies `memory.x` into `OUT_DIR`. Both work. The copy can go, but
  the build script stays for its git metadata.

## What Remains

Each step re-runs the tests ferro-wasp's own
[test catalog](https://github.com/Eirik2020/ferro-wasp/tree/main/project_meta/testing)
selects for what it touched, and a change to a safety-relevant task re-gates
flight.

1. **The last two tasks.** `usb_fs` and `flash_manager_task` become definitions,
   as [kept in the app](#kept-in-the-app) describes.
2. **CLI adoption.** Adopt `ferroforge sync` and `run`, and retire ferro-wasp's
   `tools/rtic-app-builder`, which overlaps with FerroForge, so the two do not
   drift.

## Kept in the App

Foxeer's `usb_fs` and `flash_manager_task` were the last two plain RTIC tasks
inside its `app!`. They were held back to be designed against two boards, and
FCU3's retirement voided that reason, so they become definitions shaped by
Foxeer alone: they carry the board's identity from its own build script and
types that exist only with `mspv2_configurator`, and a definition must take
both from its instance.

## FCU3

Obsolete since 2026-09-22. Its firmware stays in ferro-wasp's tree and keeps
compiling in CI so a second board can be revived cheaply, but it has no gates,
no image, and no claim on a definition's design. Where its behaviour differed
from Foxeer's - actuator validation above all - Foxeer's is now simply the
behaviour.

Only Foxeer and the NUCLEO bring-up app remain active, so nothing in
ferro-wasp currently exercises one definition across two boards.

## Open Decisions

None. What remains is migration, not design: the steps above.
