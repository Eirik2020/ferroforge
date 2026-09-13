use ferroforge::task;

trait Pin {
    fn toggle(&mut self);
}

#[task(bounds = [led: Pin], local = [led])]
async fn invalid_method(mut cx: invalid_method::Context) {
    cx.local.led.missing();
}

#[task(local = [count: u32])]
async fn invalid_value(mut cx: invalid_value::Context) {
    *cx.local.count = true;
}
