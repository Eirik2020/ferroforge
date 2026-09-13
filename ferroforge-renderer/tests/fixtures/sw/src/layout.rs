mod detail;

use ferroforge::mock::systick::Mono;
use fugit::ExtU32 as _;
use defmt::info as log_info;
use rtt_target::rprintln as rtt_log;

struct Sample(u32);

impl Sample {
    fn advance(&mut self) {
        self.0 = self.0.wrapping_add(STEP);
    }
}

const STEP: u32 = 1;

fn update(sample: &mut Sample) {
    detail::touch(sample);
}

#[ferroforge::task(shared = [sample: Sample])]
async fn first(mut cx: first::Context) {
    cx.shared.sample.lock(self::update);
}

#[ferroforge::task(shared = [sample: Sample])]
async fn second(mut cx: second::Context) {
    cx.shared.sample.lock(update);
}

#[ferroforge::task(shared = [sample: Sample], config = [step: u32])]
async fn configured(mut cx: configured::Context) {
    if CONFIG.STEP != 0 {
        cx.shared.sample.lock(self::update);
    }
}

#[ferroforge::task(shared = [sample: Sample], config = [step: u32])]
async fn config_bare(_cx: config_bare::Context) {
    let _ = CONFIG;
}

#[ferroforge::task(shared = [sample: Sample], config = [step: u32])]
async fn config_macro(_cx: config_macro::Context) {
    assert!(CONFIG.STEP == 0);
}

#[ferroforge::task(spawn = [wake(), deliver(value: u32), pair(left: u16, enabled: bool)])]
async fn producer(cx: producer::Context) {
    let _: Result<(), ()> = cx.spawn.wake();
    let _: Result<(), u32> = cx.spawn.deliver(7);
    let _: Result<(), (u16, bool)> = cx.spawn.pair(11, true);
}

#[ferroforge::task]
async fn consumer(_cx: consumer::Context, value: u32) {
    core::hint::black_box(value);
}

#[ferroforge::task]
async fn no_args(_cx: no_args::Context) {}

#[ferroforge::task]
async fn pair_inputs(_cx: pair_inputs::Context, left: u16, enabled: bool) {
    core::hint::black_box((left, enabled));
}

#[ferroforge::task(spawn = [deliver(value: u32)])]
async fn spawn_bare(cx: spawn_bare::Context) {
    let _ = &cx.spawn;
}

#[ferroforge::task(spawn = [deliver(value: u32)])]
async fn spawn_macro(cx: spawn_macro::Context) {
    assert!(cx.spawn.deliver(7).is_ok());
}

#[ferroforge::task(config = [period_ms: u32], monotonic = Mono)]
async fn periodic(_cx: periodic::Context) {
    Mono::delay(CONFIG.PERIOD_MS.millis()).await;
}

#[ferroforge::task(monotonic = Mono)]
async fn monotonic_bare(_cx: monotonic_bare::Context) {
    let _ = Mono;
}

#[ferroforge::task(monotonic = Mono)]
async fn monotonic_macro(_cx: monotonic_macro::Context) {
    assert!({
        Mono::delay(1_u32.millis()).await;
        true
    });
}

#[ferroforge::task(shared = [sample: Sample], config = [step: u32])]
async fn logged(mut cx: logged::Context) {
    log_info!("literal CONFIG.STEP and cx.shared.sample; step={=u32}", CONFIG.STEP);
    rtt_log!("sample={}", cx.shared.sample.lock(|sample| sample.0));
    defmt::warn!(
        "combined={=u32}",
        CONFIG.STEP + cx.shared.sample.lock(|sample| sample.0)
    );
}

#[ferroforge::task(shared = [sample: Sample])]
async fn shadows(mut cx: shadows::Context) {
    let cx = 1_u32;
    let _ = cx;
}

#[ferroforge::task(shared = [sample: Sample])]
async fn parent_ref(mut cx: parent_ref::Context) {
    if super::ROOT_STEP != 0 {
        cx.shared.sample.lock(self::update);
    }
}

#[ferroforge::task(shared = [sample: Sample])]
async fn crate_escape(mut cx: crate_escape::Context) {
    if crate::ROOT_STEP != 0 {
        cx.shared.sample.lock(self::update);
    }
}
