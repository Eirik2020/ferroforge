use ferroforge::{CompositionDefinition, composition};

pub(crate) const COMPOSITION: CompositionDefinition = composition! {
    dispatchers = [USART1],

    blink {
        priority = 1,
        config = {
            period_ms: u64 = 2000,
        },
    },

    timer_interrupt {
        binds = TIM2,
        priority = 2,
        config = {
            frequency_hz: u32 = 1,
            message: &'static str = "REEEEEEEEEEEEEEEEE",
        },
    },
};
