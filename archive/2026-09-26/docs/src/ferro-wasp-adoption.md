# Adopting FerroForge in ferro-wasp - archived sections

Archived on 2026-09-26 from `docs/src/ferro-wasp-adoption.md`. Its migration
order is spent: phases 1 to 5 were carried out, and the Foxeer app flew on
FerroForge's `app!` on 2026-09-24. The rule that bound every phase - each re-runs
the tests ferro-wasp's catalog selects for what it touched - was promoted into
the chapter's "What Remains" section, with what is left of the order. The CLI
gap it listed for layout was closed when ferro-wasp moved its applications to
`firmware/`. This file is not active design authority. The sections follow
verbatim.

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

- **Layout.** [G5](governing-requirements.md) requires `firmware/`, and
  ferro-wasp uses `apps/`. Its apps are already one Cargo workspace each, as G5
  requires, so the change is to rename ferro-wasp's directory, not to make
  FerroForge's layout configurable.
