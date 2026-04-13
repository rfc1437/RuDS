use std::fs;
use std::path::Path;

use rusqlite::Connection;

use crate::engine::{EngineError, EngineResult};
use crate::render::{GeneratedWriteOutcome, write_generated_bytes};

#[derive(Debug, Clone, Copy)]
pub(crate) struct BundledSiteAsset {
    pub relative_path: &'static str,
    pub bytes: &'static [u8],
}

const BUNDLED_SITE_ASSETS: &[BundledSiteAsset] = &[
    BundledSiteAsset { relative_path: "assets/bds.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/bds.css") },
    BundledSiteAsset { relative_path: "assets/calendar-runtime.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/calendar-runtime.js") },
    BundledSiteAsset { relative_path: "assets/code-enhancements.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/code-enhancements.js") },
    BundledSiteAsset { relative_path: "assets/d3.layout.cloud.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/d3.layout.cloud.js") },
    BundledSiteAsset { relative_path: "assets/highlight.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/highlight.min.css") },
    BundledSiteAsset { relative_path: "assets/highlight.min.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/highlight.min.js") },
    BundledSiteAsset { relative_path: "assets/lightbox.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/lightbox.min.css") },
    BundledSiteAsset { relative_path: "assets/lightbox.min.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/lightbox.min.js") },
    BundledSiteAsset { relative_path: "assets/pico.amber.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.amber.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.blue.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.blue.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.cyan.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.cyan.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.fuchsia.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.fuchsia.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.green.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.green.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.grey.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.grey.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.indigo.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.indigo.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.jade.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.jade.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.lime.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.lime.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.orange.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.orange.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.pink.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.pink.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.pumpkin.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.pumpkin.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.purple.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.purple.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.red.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.red.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.sand.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.sand.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.slate.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.slate.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.violet.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.violet.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.yellow.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.yellow.min.css") },
    BundledSiteAsset { relative_path: "assets/pico.zinc.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/pico.zinc.min.css") },
    BundledSiteAsset { relative_path: "assets/search-runtime.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/search-runtime.js") },
    BundledSiteAsset { relative_path: "assets/tag-cloud.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/tag-cloud.js") },
    BundledSiteAsset { relative_path: "assets/vanilla-calendar.min.css", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/vanilla-calendar.min.css") },
    BundledSiteAsset { relative_path: "assets/vanilla-calendar.min.js", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/assets/vanilla-calendar.min.js") },
    BundledSiteAsset { relative_path: "images/close.png", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/images/close.png") },
    BundledSiteAsset { relative_path: "images/loading.gif", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/images/loading.gif") },
    BundledSiteAsset { relative_path: "images/next.png", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/images/next.png") },
    BundledSiteAsset { relative_path: "images/prev.png", bytes: include_bytes!("../../../../fixtures/golden-generated-sites/rfc1437-sample/images/prev.png") },
];

pub(crate) fn copy_bundled_site_assets(project_dir: &Path) -> EngineResult<()> {
    for asset in BUNDLED_SITE_ASSETS {
        let target = project_dir.join(asset.relative_path);
        if target.exists() {
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(target, asset.bytes)?;
    }
    Ok(())
}

pub(crate) fn write_bundled_site_assets(
    conn: &Connection,
    output_dir: &Path,
    project_id: &str,
    report: &mut crate::engine::generation::GenerationReport,
) -> EngineResult<()> {
    for asset in BUNDLED_SITE_ASSETS {
        match write_generated_bytes(conn, output_dir, project_id, asset.relative_path, asset.bytes)
            .map_err(|error| EngineError::Parse(error.to_string()))?
        {
            GeneratedWriteOutcome::Written => report.written_paths.push(asset.relative_path.to_string()),
            GeneratedWriteOutcome::SkippedUnchanged => report.skipped_paths.push(asset.relative_path.to_string()),
        }
    }
    Ok(())
}