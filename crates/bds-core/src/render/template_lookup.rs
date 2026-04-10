use std::collections::HashMap;

use crate::model::{Post, Tag, Template, TemplateKind};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderCategorySettings {
    pub post_template_slug: Option<String>,
}

#[derive(Debug, Clone)]
pub struct RenderTemplateLookup<'a> {
    pub post: &'a Post,
    pub templates: &'a [Template],
    pub tags: &'a [Tag],
    pub category_settings: &'a HashMap<String, RenderCategorySettings>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplateLookupError {
    MissingExplicitTemplate(String),
    MissingDefaultTemplate,
}

pub fn resolve_post_template<'a>(lookup: RenderTemplateLookup<'a>) -> Result<&'a Template, TemplateLookupError> {
    if let Some(explicit_slug) = lookup.post.template_slug.as_deref() {
        return lookup
            .templates
            .iter()
            .find(|template| is_enabled_post_template(template, explicit_slug))
            .ok_or_else(|| TemplateLookupError::MissingExplicitTemplate(explicit_slug.to_string()));
    }

    for post_tag in &lookup.post.tags {
        if let Some(template_slug) = lookup
            .tags
            .iter()
            .find(|tag| tag.name.eq_ignore_ascii_case(post_tag))
            .and_then(|tag| tag.post_template_slug.as_deref())
        {
            if let Some(template) = lookup
                .templates
                .iter()
                .find(|template| is_enabled_post_template(template, template_slug))
            {
                return Ok(template);
            }
        }
    }

    for category_name in &lookup.post.categories {
        if let Some(template_slug) = lookup
            .category_settings
            .get(category_name)
            .and_then(|settings| settings.post_template_slug.as_deref())
        {
            if let Some(template) = lookup
                .templates
                .iter()
                .find(|template| is_enabled_post_template(template, template_slug))
            {
                return Ok(template);
            }
        }
    }

    lookup
        .templates
        .iter()
        .find(|template| is_enabled_post_template(template, "post"))
        .ok_or(TemplateLookupError::MissingDefaultTemplate)
}

fn is_enabled_post_template(template: &Template, slug: &str) -> bool {
    template.enabled && template.kind == TemplateKind::Post && template.slug == slug
}