#![no_std]

struct Shared {
    blink_enabled: bool,
}

struct Local {
    status_led: u32,
    blink_count: u32,
}

#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    Mono::start(cx.core.SYST, 16_000_000);
    status_blink::spawn().unwrap();

    (
        Shared {
            blink_enabled: true,
        },
        Local {
            status_led: 0,
            blink_count: 0,
        },
    )
}
