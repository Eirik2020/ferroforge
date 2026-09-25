# Embassy spike (experimental branch only)

Compile-only experiment: embassy-stm32 as the HAL under `ferroforge::app!`,
and embassy-time alongside RTIC. Not part of the book, the examples, or the
agreed design. Nothing here has been flashed.

- `firmware-h753zi/` - Nucleo-H753ZI on embassy-stm32; selects `blink` from
  `examples/tasks/blinky` unchanged.
- `tasks-async/` - portable tasks on `embedded-io-async` and embassy-time,
  with no `monotonic = Mono`.

Build: `cargo build --release` in `firmware-h753zi/`. It lives outside
`examples/firmware/` because `ferroforge sync` would rewrite its hand-picked
platform crates back to `stm32h7xx-hal`.
