mod markdown;
mod generation;
mod page_renderer;
mod routes;
mod template_lookup;

pub use generation::{
	CalendarArchiveData, GeneratedWriteOutcome, build_calendar_json,
	build_core_generation_paths, write_generated_file,
};
pub use markdown::render_markdown_to_html;
pub use page_renderer::{RenderError, render_liquid_template};
pub use routes::{
	RenderedPage, build_canonical_post_path, render_starter_list_page,
	render_starter_list_page_with_media_map, render_starter_single_post_page,
	render_starter_single_post_page_with_media_map,
};
pub use template_lookup::{
	RenderCategorySettings, RenderTemplateLookup, TemplateLookupError,
	resolve_post_template,
};
