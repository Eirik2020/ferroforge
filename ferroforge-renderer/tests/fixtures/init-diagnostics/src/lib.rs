#![no_std]

struct Shared;
struct Local;

#[ferroforge::init]
fn init(cx: init::Context) -> (Shared, Local) {
    status::spawn(1).unwrap();
    Mono::start(cx.core.SYST, 16_000_000_u64);
    loop {}
}

