#![no_std]

pub mod indicators;
pub mod layout;
pub mod layout_other;
pub mod interrupts;

const ROOT_STEP: u32 = 1;

#[ferroforge::task]
pub async fn root_task(cx: root_task::Context) {}

pub mod inline {
    use ferroforge::task;

    const LABEL: &str = "CONFIG.PERIOD_MS must remain literal text";

    #[task]
    pub async fn report(cx: report::Context) {}
}
