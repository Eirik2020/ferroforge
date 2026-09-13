#![no_main]
#![no_std]

use panic_probe as _;

// This outer module represents one transplanted logical source module. Its
// ordinary support stays private, while the real RTIC app is its child and can
// therefore retain the source module's access rules without runtime wrappers.
mod indicators {
    // RTIC exposes resource types through generated interfaces, so the
    // transplant widens only such support types. The authored item can remain
    // private; this visibility is generator-owned access wiring.
    pub struct Sample(u32);

    impl Sample {
        fn advance(&mut self) {
            self.0 = self.0.wrapping_add(STEP);
        }
    }

    const STEP: u32 = 1;

    fn update(sample: &mut Sample) {
        sample.advance();
    }

    #[rtic::app(device = stm32f4xx_hal::pac, dispatchers = [USART1])]
    mod app {
        use super::*;

        #[shared]
        struct Shared {
            sample: Sample,
        }

        #[local]
        struct Local {}

        #[init]
        fn init(_cx: init::Context) -> (Shared, Local) {
            first::spawn().unwrap();
            second::spawn().unwrap();
            (Shared { sample: Sample(0) }, Local {})
        }

        // Both complete handlers resolve the same private `Sample` and `update`
        // items from their one enclosing source module.
        #[task(priority = 1, shared = [sample])]
        async fn first(mut cx: first::Context) {
            cx.shared.sample.lock(update);
        }

        #[task(priority = 1, shared = [sample])]
        async fn second(mut cx: second::Context) {
            cx.shared.sample.lock(update);
        }
    }
}
