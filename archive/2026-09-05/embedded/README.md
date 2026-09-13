# Embedded source workspace

This standalone `no_std` library contains the HAL-specific resources, init
code, and reusable task implementations for the example application.

Its `.cargo/config.toml` selects `thumbv7em-none-eabihf`. The FerroForge mock
contexts type-check the source against the real STM32F4 HAL, but this package
does not contain an entry point and does not produce runnable firmware.

Check it independently with:

```text
cd embedded
cargo check --lib
```

Run the command from this directory so Cargo discovers `.cargo/config.toml`
and checks the declarations for `thumbv7em-none-eabihf`.

The scheduling and configuration values in `src/lib.rs` are defaults used for
this independent ARM check. The host-side `composition!` in
`composer/src/main.rs` replaces them when rendering the final RTIC application
into `generated/nucleo-f401re`.
