# FerroForge

Reusable [RTIC](https://rtic.rs) tasks, selected into firmware. This crate
provides the two macros: `#[ferroforge::task]` marks a definition, and
`ferroforge::app!` declares the application that selects it.

A task is an ordinary function in its own crate. It names what it needs, and
compiles on its own without any firmware:

```rust,ignore
#[ferroforge::task(
    local = [count: u32],
    config = [period_ms: u32],
    monotonic = Mono,
)]
pub async fn heartbeat(cx: heartbeat::Context) -> ! {
    loop {
        *cx.local.count = cx.local.count.wrapping_add(1);
        defmt::info!("heartbeat {=u32}", *cx.local.count);
        Mono::delay(CONFIG.PERIOD_MS.millis()).await;
    }
}
```

A firmware writes an ordinary RTIC application - resources, `init`, native HAL
calls - and declares each task instance by naming its definition and binding
its needs to the firmware's own resources and values:

```rust,ignore
ferroforge::app! {
    device = stm32f4xx_hal::pac,
    dispatchers = [SPI1],

    use rtic_monotonics::systick::prelude::*;
    systick_monotonic!(Mono, 1000);

    use my_tasks::heartbeat;

    #[shared]
    struct Shared {}

    #[local]
    struct Local { heartbeat_count: u32 }

    #[init]
    fn init(cx: init::Context) -> (Shared, Local) {
        // clocks, peripherals, Mono::start(..)
        status::spawn().unwrap();
        (Shared {}, Local { heartbeat_count: 0 })
    }

    #[task(
        from = heartbeat,
        priority = 1,
        local = [count = heartbeat_count],
        config = [period_ms: u32 = 1000],
    )]
    async fn status(cx: status::Context) -> !;
}
```

`app!` expands in place into `#[rtic::app]`, and each declaration becomes a
handler that calls the definition. Nothing is copied or generated into another
project, so every mistake is an ordinary compile error at the line that made it,
and checking is RTIC's and Rust's - FerroForge adds none of its own.

The quickest start is the `ferroforge` command, which writes a working project:
see [`ferroforge-cli`](https://crates.io/crates/ferroforge-cli). The authoring
model in full is in the
[architecture chapter](https://github.com/Eirik2020/ferroforge/blob/main/docs/src/architecture.md).

Licensed under either of MIT or Apache-2.0, at your option.
