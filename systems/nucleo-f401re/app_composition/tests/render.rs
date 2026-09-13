use std::{
    env, fs,
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

struct TempOutput(PathBuf);

impl TempOutput {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        Self(env::temp_dir().join(format!(
            "ferroforge-nucleo-f401re-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        )))
    }
}

impl Drop for TempOutput {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn repository_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("system package must be three levels below the repository root")
        .to_path_buf()
}

#[test]
fn renders_the_centralized_nucleo_system() {
    let output = TempOutput::new();
    let rendered = ferroforge_nucleo_f401re_composer::render_nucleo_f401re(
        &repository_root(),
        &output.0.join("init-check"),
        &output.0.join("gen_app"),
    )
    .unwrap();

    let source = fs::read_to_string(rendered.firmware.main_source).unwrap();
    let manifest = fs::read_to_string(rendered.firmware.manifest).unwrap();
    let memory = fs::read_to_string(rendered.firmware.memory_layout).unwrap();
    let init_interface = fs::read_to_string(rendered.init_check.interface_source).unwrap();

    assert!(source.contains("async fn status_blink"));
    assert!(source.contains("async fn telemetry"));
    assert!(source.contains("blink_enabled: bool"));
    assert!(source.contains("status_blink::spawn().unwrap()"));
    assert!(source.contains("mod __ferroforge_config"));
    assert!(source.contains("__ferroforge_config::status_blink::PERIOD_MS"));
    assert!(!source.contains("ferroforge::"));
    assert!(!manifest.contains("ferroforge"));
    assert!(manifest.contains("stm32f4xx-hal"));
    assert!(memory.contains("FLASH : ORIGIN = 0x08000000, LENGTH = 512K"));
    assert!(init_interface.contains("pub mod status_blink"));
    assert!(init_interface.contains("pub mod telemetry"));
}
