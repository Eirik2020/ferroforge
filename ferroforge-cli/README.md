# ferroforge-cli

The `ferroforge` command. FerroForge composes reusable [RTIC](https://rtic.rs)
tasks into firmware: a task is an ordinary function in its own crate, and a
firmware selects it with `ferroforge::app!`, which expands into a real
`#[rtic::app]`.

## Install

```text
cargo install ferroforge-cli
rustup target add thumbv7em-none-eabihf
cargo install probe-rs-tools --locked   # for `ferroforge run`
```

Rust 1.88 or newer.

## First project

```text
ferroforge new hello --chip stm32f401re
cd hello
ferroforge run
```

`new` writes a firmware and a task crate. The firmware runs one task from that
crate, which logs a heartbeat over RTT; `run` flashes it through a debug probe
and prints the log. It starts from the internal oscillator and uses no pins, so
it runs on any board with the chip.

## Commands

| Command | Does |
| --- | --- |
| `ferroforge new <path> [--chip <name>]` | Create a project |
| `ferroforge add <name> --chip <name>` | Add a firmware to the project you are in |
| `ferroforge check [<firmware>]` | Sync, then `cargo check` |
| `ferroforge build [<firmware>]` | Sync, then `cargo build --release` |
| `ferroforge run [<firmware>]` | Sync, then flash and run with probe-rs |
| `ferroforge sync [<firmware>]` | Rewrite the files the chip implies |
| `ferroforge check --all`, `build --all`, `sync --all` | The same for every firmware, one status line each |
| `ferroforge drift` | Compare code marked as copied between firmwares |
| `ferroforge chips [--names]` | List the supported chips, with HAL, target and memory |

A project is the nearest directory above you holding a `firmware/`. Each
firmware names its chip in its `Cargo.toml`, and `sync` derives `memory.x`,
`.cargo/config.toml`, `Embed.toml` and the platform crates from it. Arguments
after `--` go to Cargo.

## Status

A 0.1 release for testing. Supported chips are the STM32F401RE, F405RG, F411RE
and H753ZI. The context a task receives is an interface between task crates and
firmware and may change between 0.x versions.

Known limits: a task that reads time needs a 1 kHz monotonic, and tasks bounded
on `embedded-hal` 1.0 cannot be used with HALs still on 0.2, which includes
`stm32h7xx-hal`.

How tasks are written and selected:
[`ferroforge`](https://crates.io/crates/ferroforge). Design and decisions: the
[FerroForge book](https://github.com/Eirik2020/ferroforge/blob/main/docs/src/SUMMARY.md).

Licensed under either of MIT or Apache-2.0, at your option.
