struct Sample(bool);

impl Sample {
    fn advance(&mut self) {
        self.0 = !self.0;
    }
}

const STEP: bool = true;

fn update(sample: &mut Sample) {
    if self::STEP {
        sample.advance();
    }
}

#[ferroforge::task(shared = [sample: Sample])]
async fn other(mut cx: other::Context) {
    cx.shared.sample.lock(crate::layout_other::update);
}
