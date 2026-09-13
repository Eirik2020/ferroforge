# Nucleo-F401RE Fast Blink System

This is the second standalone-system proof. It deliberately uses the same
STM32F401RE board target as `systems/nucleo-f401re` so the proof isolates
system-level reuse rather than adding another HAL or target profile.

The system consumes `tasks/blinky` unchanged and owns a distinct init package,
composition, generated project, instance names, resource names, and 125 ms
blink-period binding.

From the repository root, run its full pipeline with:

```text
cargo run -p ferroforge-nucleo-f401re-fast-blink-composer --locked --offline
```
