use std::{error::Error, path::PathBuf};

use ferroforge_renderer::RenderOptions;

mod composition;

fn main() -> Result<(), Box<dyn Error>> {
    let host_workspace = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("composer package should be inside the host workspace")
        .to_path_buf();
    let application = ferroforge_renderer::load_application(&host_workspace.join("embedded"))?;
    let output_dir = host_workspace.join("generated/nucleo-f401re");

    let rendered = ferroforge_renderer::render_loaded_composed(
        &application,
        &composition::COMPOSITION,
        RenderOptions {
            package_name: "nucleo-f401re-rtic",
            output_dir: &output_dir,
        },
    )?;

    println!("rendered {}", rendered.main_source.display());
    Ok(())
}
