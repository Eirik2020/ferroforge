# Adopting FerroForge in ferro-wasp

The [decision record](review.md) says FerroForge co-evolves with the
[ferro-wasp](https://github.com/Eirik2020/ferro-wasp) flight controller. This
chapter is the plan for that: what ferro-wasp needs from FerroForge before it can
adopt it, and the order to migrate in. It was written against FerroForge 0.2.0
and ferro-wasp `370460b`.

Short answer: the macros fit ferro-wasp's architecture, and the CLI cannot
manage ferro-wasp's firmware until a few decisions are made. The macros are
usable without the CLI, so adoption can start with the next macro release.

## What Already Fits

- ferro-wasp declares `systick_monotonic!(Mono, 1000)`, which is the agreed
  1 kHz `u32` profile in [task authoring](architecture.md#task-authoring).
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

None of these block using the macros alone.

- **Layout.** [G5](governing-requirements.md) requires `firmware/`, and
  ferro-wasp uses `apps/`. Its apps are already one Cargo workspace each, as G5
  requires, so the change is to rename ferro-wasp's directory, not to make
  FerroForge's layout configurable.
- **HAL source.** ferro-wasp pins `stm32f4xx-hal` to a git revision that is
  version 0.22.1, and the F405 backend selects 0.23.0 from crates.io.
  `[patch.crates-io]` cannot bridge a semver-incompatible version, so either
  ferro-wasp moves to 0.23.0, once someone checks that it has everything the
  revision was pinned for, or a firmware gets a way to override a backend's
  platform crate source. The second touches G2a. **Needs a decision.** The HAL
  features it enables (`rtic2`, `defmt`, `usb_fs`, `uart4`) are not a problem:
  `ferrowasp-stm32f4` already enables them, and Cargo unifies features.
- **Probe and environment settings.** `sync` rewrites `.cargo/config.toml`.
  ferro-wasp's apps use `probe-rs attach`, `--protocol swd` and a `CHIPSERIE`
  environment variable, which would be lost. G5 says nothing but target files
  is generated, and these are firmware choices inside a generated file.
  **Needs a decision** on where a firmware records them. `[env]` in a parent
  `.cargo/config.toml` would carry the variable. Runner arguments have no
  such route.
- **Linker search path.** The generated config adds `-L.`, and ferro-wasp's
  `build.rs` copies `memory.x` into `OUT_DIR`. Both work. The copy can go, but
  the build script stays for its git metadata.

## Migration Order

Each phase has an entry condition. Safety-relevant tasks come last, and every
ferro-wasp phase re-runs the tests its own
[test catalog](https://github.com/Eirik2020/ferro-wasp/tree/main/project_meta/testing)
selects for the tasks it touched.

1. **FerroForge release.** Release the macro changes. Entry: nothing.
2. **Bring-up app.** Move ferro-wasp's NUCLEO-F401RE app under `firmware/` and
   express it with `app!`, depending on `ferroforge` only, not the CLI. Entry:
   phase 1 released.
3. **Foxeer leaf tasks.** Convert tasks that can neither arm nor actuate:
   `heartbeat`, then `osd_refresh` and `uart4_tx_worker`, then
   `esc_manager_task`. Keep the rest as plain RTIC tasks inside the same
   `app!`. Entry: phase 2 builds and runs on hardware.
4. **I/O and DMA tasks.** The SPI, UART and ADC paths, which exercise the
   hardware-task model hardest. Entry: phase 3 bench-tested.
5. **Safety and control.** `safety_master`, `actuator_output`, `dshot_service`
   and `control_loop`, one at a time, with the full bench plan and a controlled
   flight after each. Entry: phase 4 bench-tested, with the release build's
   timing and `.text` size compared to the plain-RTIC build.
6. **CLI adoption and cleanup.** Adopt `ferroforge sync` and `run` once the
   CLI decisions are made. Retire ferro-wasp's `tools/rtic-app-builder`,
   which overlaps with FerroForge, so the two do not drift. Entry: phase 5
   flown.

## FCU3 Drift

Where FCU3's copy of a task matches Foxeer's, both select one definition.
Where it differs, FCU3 keeps its own until someone decides which behaviour
is right; unifying changes one board's behaviour or its logs. Foxeer is the
golden app, so the default answer is Foxeer's, but each needs a decision:

| Task | How FCU3 differs |
| --- | --- |
| `actuator_output` and `ActuatorHardware` | Safety-relevant. FCU3's wrapper refuses a non-finite or out-of-range throttle vector itself, stops the motors on any rejected command, and aborts arming by commanding low throttle; Foxeer's validates armed commands in the task and relies on the bank's own rejection in the wrapper, then forces the motors off. Both appear to fail safe, differently. |
| `dshot_service`, DShot DMA handlers | Log wording; FCU3 reports a spurious DMA interrupt once rather than every time. |
| `esc_manager_task` | Log wording only; its motor map is a table where Foxeer's is a function, with the same values. |
| `usart1_rx_dma_transfer`, `usart1_rx_peripheral` | Log wording and layout only. |
| `safety_master` | No actuator-output inhibit or capped bench mode; a different arming-failure report. |
| `heartbeat` | A much smaller status report. |
| `spi1_poll`, `spi1_parser` | An older, timer-driven, MPU6500-only IMU path. |
| `dma_adc1` | A fixed cell count and an older current formula. |
| `usb_fs` | A much smaller USB task, without Foxeer's storage CLI or configurator. |

## Open Decisions

- Each row of [FCU3 drift](#fcu3-drift): unify on Foxeer's behaviour, or keep
  FCU3's.
- Overriding a backend's platform crate source (CLI gaps, HAL source).
- Where a firmware records probe arguments and environment variables (CLI
  gaps, probe and environment settings).

When one of these is settled, record it in the [review chapter](review.md) and
specify it in its owning chapter, as [maintaining this book](documentation.md)
requires.
