use ferroforge::task;

const STEP: u32 = 1;

fn advance(counter: &mut u32) {
    *counter = counter.wrapping_add(STEP);
}

/// A synchronous handler, so the contract reads it as a hardware task.
/// Composition supplies the interrupt it binds; the definition does not name
/// one, because the same handler can be bound to different interrupts by
/// different firmware.
#[task(local = [ticks: u32])]
pub fn on_tick(cx: on_tick::Context) {
    advance(cx.local.ticks);
}
