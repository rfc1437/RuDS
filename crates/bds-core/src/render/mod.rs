mod generation;
mod macros;
mod markdown;
mod page_renderer;
mod routes;
mod site;
mod template_lookup;

pub use generation::{
    CalendarArchiveData, GeneratedWriteOutcome, build_calendar_json, build_core_generation_paths,
    write_generated_bytes, write_generated_file,
};
pub use markdown::render_markdown_to_html;
pub(crate) use page_renderer::render_liquid_template_with_host;
pub use page_renderer::{RenderError, render_liquid_template};
pub(crate) use routes::{PostLanguageVariant, select_post_language_variant};
pub use routes::{
    RenderedPage, build_canonical_post_path, render_starter_list_page,
    render_starter_list_page_with_media_map, render_starter_single_post_page,
    render_starter_single_post_page_with_media_map,
};
pub use site::{
    PagefindDocument, PreviewRenderResult, SitePage, SiteRenderArtifacts, build_preview_response,
    build_site_render_artifacts, build_site_section_render_artifacts,
    build_targeted_site_section_render_artifacts,
};
pub use template_lookup::{
    RenderCategorySettings, RenderTemplateLookup, TemplateLookupError, resolve_post_template,
};
