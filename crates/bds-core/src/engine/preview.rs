use crate::engine::{EngineError, EngineResult};
use crate::engine::generation::PublishedPostSource;
use crate::model::ProjectMetadata;
use crate::render::{build_canonical_post_path, render_starter_list_page, render_starter_single_post_page};

pub fn render_preview_path(
    path: &str,
    metadata: &ProjectMetadata,
    posts: &[PublishedPostSource],
) -> EngineResult<Option<String>> {
    let normalized = if path.is_empty() { "/" } else { path };
    let main_language = metadata.main_language.as_deref().unwrap_or("en");

    if normalized == "/" {
        let list_posts = posts
            .iter()
            .map(|source| (source.post.clone(), source.body_markdown.clone()))
            .collect::<Vec<_>>();
        return render_starter_list_page(&list_posts, metadata, main_language)
            .map(|page| Some(page.html))
            .map_err(|error| EngineError::Parse(error.to_string()));
    }

    let (language, route_path) = split_language_prefix(normalized, metadata);
    if let Some(source) = posts.iter().find(|source| {
        build_canonical_post_path(&source.post, &language, main_language) == route_path
    }) {
        return render_starter_single_post_page(&source.post, &source.body_markdown, metadata, &language)
            .map(|page| Some(page.html))
            .map_err(|error| EngineError::Parse(error.to_string()));
    }

    Ok(None)
}

fn split_language_prefix(path: &str, metadata: &ProjectMetadata) -> (String, String) {
    let trimmed = path.trim_start_matches('/');
    let mut segments = trimmed.split('/');
    let first = segments.next().unwrap_or_default();
    if metadata.blog_languages.iter().any(|language| language == first) {
        let remainder = segments.collect::<Vec<_>>().join("/");
        return (first.to_string(), format!("/{first}/{}", remainder.trim_start_matches('/')));
    }

    (
        metadata.main_language.as_deref().unwrap_or("en").to_string(),
        path.to_string(),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Post, PostStatus};

    fn make_metadata() -> ProjectMetadata {
        ProjectMetadata {
            name: "Blog".into(),
            description: None,
            public_url: Some("https://example.com".into()),
            main_language: Some("en".into()),
            default_author: None,
            max_posts_per_page: 50,
            blogmark_category: None,
            pico_theme: None,
            semantic_similarity_enabled: false,
            blog_languages: vec!["en".into(), "de".into()],
        }
    }

    fn make_post() -> PublishedPostSource {
        PublishedPostSource {
            post: Post {
                id: "post-1".into(),
                project_id: "project-1".into(),
                title: "Hello".into(),
                slug: "hello".into(),
                excerpt: None,
                content: Some("Body".into()),
                status: PostStatus::Published,
                author: None,
                language: Some("en".into()),
                do_not_translate: false,
                template_slug: None,
                file_path: String::new(),
                checksum: None,
                tags: vec![],
                categories: vec![],
                published_title: None,
                published_content: None,
                published_tags: None,
                published_categories: None,
                published_excerpt: None,
                created_at: 1_710_000_000_000,
                updated_at: 1_710_000_000_000,
                published_at: Some(1_710_000_000_000),
            },
            body_markdown: "Hello **world**".into(),
        }
    }

    #[test]
    fn root_preview_renders_index_page() {
        let html = render_preview_path("/", &make_metadata(), &[make_post()])
            .unwrap()
            .unwrap();
        assert!(html.contains("post-list"));
    }

    #[test]
    fn preview_renders_single_post_for_canonical_path() {
        let html = render_preview_path("/2024/03/09/hello", &make_metadata(), &[make_post()])
            .unwrap()
            .unwrap();
        assert!(html.contains("<h1>Hello</h1>"));
        assert!(html.contains("<strong>world</strong>"));
    }

    #[test]
    fn preview_renders_language_prefixed_single_post() {
        let html = render_preview_path("/de/2024/03/09/hello", &make_metadata(), &[make_post()])
            .unwrap()
            .unwrap();
        assert!(html.contains("lang=\"de\""));
    }
}