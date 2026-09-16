//! Second standalone-system proof: the same reusable tasks and board target as
//! `systems/nucleo-f401re`, with this firmware's own instance names, resource
//! names, and blink period.

use std::{
    error::Error,
    path::{Path, PathBuf},
};

use ferroforge_nucleo_f401re_composer::{
    BlinkyNucleoF401reProfile, render_blinky_nucleo_f401re, run_blinky_nucleo_f401re_pipeline,
};
pub use ferroforge_pipeline::{PipelineError, RenderedFirmware as RenderedNucleoF401re};

pub fn profile() -> BlinkyNucleoF401reProfile {
    BlinkyNucleoF401reProfile {
        init_check_package_name: "nucleo-f401re-fast-blink-init-check".to_owned(),
        firmware_package_name: "nucleo-f401re-fast-blink-rtic".to_owned(),
        blink_instance: "heartbeat".to_owned(),
        report_instance: "diagnostics".to_owned(),
        led_resource: "activity_led".to_owned(),
        count_resource: "pulse_count".to_owned(),
        enabled_resource: "heartbeat_enabled".to_owned(),
        period_ms: 125,
    }
}

pub fn render_nucleo_f401re_fast_blink(
    repository_root: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
) -> Result<RenderedNucleoF401re, Box<dyn Error>> {
    Ok(render_blinky_nucleo_f401re(
        repository_root,
        &init_manifest(repository_root),
        init_check_dir,
        firmware_dir,
        &profile(),
    )?)
}

pub fn run_nucleo_f401re_fast_blink_pipeline(
    repository_root: &Path,
    init_check_dir: &Path,
    firmware_dir: &Path,
) -> Result<RenderedNucleoF401re, PipelineError> {
    run_blinky_nucleo_f401re_pipeline(
        repository_root,
        &init_manifest(repository_root),
        init_check_dir,
        firmware_dir,
        &profile(),
    )
}

fn init_manifest(repository_root: &Path) -> PathBuf {
    repository_root.join("systems/nucleo-f401re-fast-blink/init/Cargo.toml")
}
