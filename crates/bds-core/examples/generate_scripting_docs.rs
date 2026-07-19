use std::fs;
use std::path::Path;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let docs = root.join("docs/scripting");
    let manifest = bds_core::scripting::api_manifest();
    fs::write(docs.join("API_REFERENCE.md"), manifest.render_reference())?;
    fs::write(docs.join("TYPES.md"), manifest.render_types())?;
    fs::write(docs.join("completions.json"), manifest.render_completions())?;
    Ok(())
}
