use std::{error::Error, path::PathBuf};

fn main() -> Result<(), Box<dyn Error>> {
    let composition_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let system_dir = composition_dir
        .parent()
        .expect("app_composition must be inside the system directory");
    let repository_root = system_dir
        .parent()
        .and_then(|systems| systems.parent())
        .expect("system directory must be inside the repository systems directory");
    let rendered =
        ferroforge_nucleo_f401re_fast_blink_composer::run_nucleo_f401re_fast_blink_pipeline(
            repository_root,
            &system_dir.join(".ferroforge/init-check"),
            &system_dir.join("gen_app"),
        )?;
    println!(
        "generated {} and completed the Nucleo-F401RE fast-blink pipeline",
        rendered.firmware.root.display()
    );
    Ok(())
}
