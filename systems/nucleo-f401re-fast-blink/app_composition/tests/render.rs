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
            "ferroforge-nucleo-f401re-fast-blink-{}-{}",
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
fn renders_a_second_system_without_modifying_the_reusable_task() {
    let repository_root = repository_root();
    let task_source_path = repository_root.join("tasks/blinky/src/lib.rs");
    let task_source_before = fs::read(&task_source_path).unwrap();
    let output = TempOutput::new();
    let rendered =
        ferroforge_nucleo_f401re_fast_blink_composer::render_nucleo_f401re_fast_blink(
            &repository_root,
            &output.0.join("init-check"),
            &output.0.join("gen_app"),
        )
        .unwrap();

    let source = fs::read_to_string(rendered.firmware.main_source).unwrap();
    let manifest = fs::read_to_string(rendered.firmware.manifest).unwrap();
    let init_interface = fs::read_to_string(rendered.init_check.interface_source).unwrap();

    assert!(source.contains("async fn heartbeat"));
    assert!(source.contains("async fn diagnostics"));
    assert!(source.contains("heartbeat_enabled: bool"));
    assert!(source.contains("activity_led: PA5"));
    assert!(source.contains("pulse_count: u32"));
    assert!(source.contains("heartbeat::spawn().unwrap()"));
    assert!(source.contains("__ferroforge_config::heartbeat::PERIOD_MS"));
    assert!(source.contains("pub(crate) const PERIOD_MS: u32 = 125"));
    assert!(!source.contains("status_blink"));
    assert!(!source.contains("blink_enabled"));
    assert!(manifest.contains("name = \"nucleo-f401re-fast-blink-rtic\""));
    assert!(init_interface.contains("pub mod heartbeat"));
    assert!(init_interface.contains("pub mod diagnostics"));
    assert_eq!(fs::read(task_source_path).unwrap(), task_source_before);
}
