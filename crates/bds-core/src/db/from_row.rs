use diesel::prelude::*;
use diesel::sqlite::Sqlite;

use crate::db::schema;
use crate::model::{
    DbNotification, GeneratedFileHash, Media, MediaTranslation, NotificationAction,
    NotificationEntity, Post, PostLink, PostMedia, PostStatus, PostTranslation, Project, Script,
    ScriptKind, ScriptStatus, Setting, Tag, Template, TemplateKind, TemplateStatus,
};

type ConversionError = Box<dyn std::error::Error + Send + Sync>;

fn invalid_value(kind: &str, value: &str) -> ConversionError {
    format!("invalid {kind}: {value}").into()
}

fn json_strings(value: &str) -> Result<Vec<String>, ConversionError> {
    Ok(serde_json::from_str(value)?)
}

pub fn post_status_to_str(value: &PostStatus) -> &'static str {
    match value {
        PostStatus::Draft => "draft",
        PostStatus::Published => "published",
        PostStatus::Archived => "archived",
    }
}

fn post_status(value: &str) -> Result<PostStatus, ConversionError> {
    match value {
        "draft" => Ok(PostStatus::Draft),
        "published" => Ok(PostStatus::Published),
        "archived" => Ok(PostStatus::Archived),
        _ => Err(invalid_value("PostStatus", value)),
    }
}

pub fn template_kind_to_str(value: &TemplateKind) -> &'static str {
    match value {
        TemplateKind::Post => "post",
        TemplateKind::List => "list",
        TemplateKind::NotFound => "not_found",
        TemplateKind::Partial => "partial",
    }
}

fn template_kind(value: &str) -> Result<TemplateKind, ConversionError> {
    match value {
        "post" => Ok(TemplateKind::Post),
        "list" => Ok(TemplateKind::List),
        "not_found" => Ok(TemplateKind::NotFound),
        "partial" => Ok(TemplateKind::Partial),
        _ => Err(invalid_value("TemplateKind", value)),
    }
}

pub fn template_status_to_str(value: &TemplateStatus) -> &'static str {
    match value {
        TemplateStatus::Draft => "draft",
        TemplateStatus::Published => "published",
    }
}

fn template_status(value: &str) -> Result<TemplateStatus, ConversionError> {
    match value {
        "draft" => Ok(TemplateStatus::Draft),
        "published" => Ok(TemplateStatus::Published),
        _ => Err(invalid_value("TemplateStatus", value)),
    }
}

pub fn script_kind_to_str(value: &ScriptKind) -> &'static str {
    match value {
        ScriptKind::Macro => "macro",
        ScriptKind::Utility => "utility",
        ScriptKind::Transform => "transform",
    }
}

fn script_kind(value: &str) -> Result<ScriptKind, ConversionError> {
    match value {
        "macro" => Ok(ScriptKind::Macro),
        "utility" => Ok(ScriptKind::Utility),
        "transform" => Ok(ScriptKind::Transform),
        _ => Err(invalid_value("ScriptKind", value)),
    }
}

pub fn script_status_to_str(value: &ScriptStatus) -> &'static str {
    match value {
        ScriptStatus::Draft => "draft",
        ScriptStatus::Published => "published",
    }
}

fn script_status(value: &str) -> Result<ScriptStatus, ConversionError> {
    match value {
        "draft" => Ok(ScriptStatus::Draft),
        "published" => Ok(ScriptStatus::Published),
        _ => Err(invalid_value("ScriptStatus", value)),
    }
}

pub fn notification_entity_to_str(value: &NotificationEntity) -> &'static str {
    match value {
        NotificationEntity::Post => "post",
        NotificationEntity::Media => "media",
        NotificationEntity::Script => "script",
        NotificationEntity::Template => "template",
    }
}

fn notification_entity(value: &str) -> Result<NotificationEntity, ConversionError> {
    match value {
        "post" => Ok(NotificationEntity::Post),
        "media" => Ok(NotificationEntity::Media),
        "script" => Ok(NotificationEntity::Script),
        "template" => Ok(NotificationEntity::Template),
        _ => Err(invalid_value("NotificationEntity", value)),
    }
}

pub fn notification_action_to_str(value: &NotificationAction) -> &'static str {
    match value {
        NotificationAction::Created => "created",
        NotificationAction::Updated => "updated",
        NotificationAction::Deleted => "deleted",
    }
}

fn notification_action(value: &str) -> Result<NotificationAction, ConversionError> {
    match value {
        "created" => Ok(NotificationAction::Created),
        "updated" => Ok(NotificationAction::Updated),
        "deleted" => Ok(NotificationAction::Deleted),
        _ => Err(invalid_value("NotificationAction", value)),
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::projects, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub slug: String,
    pub description: Option<String>,
    pub data_path: Option<String>,
    pub is_active: i32,
    pub created_at: i64,
    pub updated_at: i64,
}

impl From<&Project> for ProjectRecord {
    fn from(value: &Project) -> Self {
        Self {
            id: value.id.clone(),
            name: value.name.clone(),
            slug: value.slug.clone(),
            description: value.description.clone(),
            data_path: value.data_path.clone(),
            is_active: value.is_active as i32,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

impl From<ProjectRecord> for Project {
    fn from(value: ProjectRecord) -> Self {
        Self {
            id: value.id,
            name: value.name,
            slug: value.slug,
            description: value.description,
            data_path: value.data_path,
            is_active: value.is_active != 0,
            created_at: value.created_at,
            updated_at: value.updated_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::posts, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct PostRecord {
    pub id: String,
    pub project_id: String,
    pub title: String,
    pub slug: String,
    pub excerpt: Option<String>,
    pub content: Option<String>,
    pub status: String,
    pub author: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
    pub published_at: Option<i64>,
    pub file_path: String,
    pub checksum: Option<String>,
    pub tags: String,
    pub categories: String,
    pub template_slug: Option<String>,
    pub language: Option<String>,
    pub do_not_translate: i32,
    pub published_title: Option<String>,
    pub published_content: Option<String>,
    pub published_tags: Option<String>,
    pub published_categories: Option<String>,
    pub published_excerpt: Option<String>,
}

impl From<&Post> for PostRecord {
    fn from(value: &Post) -> Self {
        Self {
            id: value.id.clone(),
            project_id: value.project_id.clone(),
            title: value.title.clone(),
            slug: value.slug.clone(),
            excerpt: value.excerpt.clone(),
            content: value.content.clone(),
            status: post_status_to_str(&value.status).into(),
            author: value.author.clone(),
            created_at: value.created_at,
            updated_at: value.updated_at,
            published_at: value.published_at,
            file_path: value.file_path.clone(),
            checksum: value.checksum.clone(),
            tags: serde_json::to_string(&value.tags).unwrap_or_else(|_| "[]".into()),
            categories: serde_json::to_string(&value.categories).unwrap_or_else(|_| "[]".into()),
            template_slug: value.template_slug.clone(),
            language: value.language.clone(),
            do_not_translate: value.do_not_translate as i32,
            published_title: value.published_title.clone(),
            published_content: value.published_content.clone(),
            published_tags: value.published_tags.clone(),
            published_categories: value.published_categories.clone(),
            published_excerpt: value.published_excerpt.clone(),
        }
    }
}

impl TryFrom<PostRecord> for Post {
    type Error = ConversionError;
    fn try_from(value: PostRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: value.id,
            project_id: value.project_id,
            title: value.title,
            slug: value.slug,
            excerpt: value.excerpt,
            content: value.content,
            status: post_status(&value.status)?,
            author: value.author,
            language: value.language,
            do_not_translate: value.do_not_translate != 0,
            template_slug: value.template_slug,
            file_path: value.file_path,
            checksum: value.checksum,
            tags: json_strings(&value.tags)?,
            categories: json_strings(&value.categories)?,
            published_title: value.published_title,
            published_content: value.published_content,
            published_tags: value.published_tags,
            published_categories: value.published_categories,
            published_excerpt: value.published_excerpt,
            created_at: value.created_at,
            updated_at: value.updated_at,
            published_at: value.published_at,
        })
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::post_translations, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct PostTranslationRecord {
    pub id: String,
    pub project_id: String,
    pub translation_for: String,
    pub language: String,
    pub title: String,
    pub excerpt: Option<String>,
    pub content: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub published_at: Option<i64>,
    pub file_path: String,
    pub checksum: Option<String>,
}

impl From<&PostTranslation> for PostTranslationRecord {
    fn from(v: &PostTranslation) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            translation_for: v.translation_for.clone(),
            language: v.language.clone(),
            title: v.title.clone(),
            excerpt: v.excerpt.clone(),
            content: v.content.clone(),
            status: post_status_to_str(&v.status).into(),
            created_at: v.created_at,
            updated_at: v.updated_at,
            published_at: v.published_at,
            file_path: v.file_path.clone(),
            checksum: v.checksum.clone(),
        }
    }
}
impl TryFrom<PostTranslationRecord> for PostTranslation {
    type Error = ConversionError;
    fn try_from(v: PostTranslationRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id,
            project_id: v.project_id,
            translation_for: v.translation_for,
            language: v.language,
            title: v.title,
            excerpt: v.excerpt,
            content: v.content,
            status: post_status(&v.status)?,
            file_path: v.file_path,
            checksum: v.checksum,
            created_at: v.created_at,
            updated_at: v.updated_at,
            published_at: v.published_at,
        })
    }
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = schema::post_links, check_for_backend(Sqlite))]
pub struct PostLinkRecord {
    pub id: String,
    pub source_post_id: String,
    pub target_post_id: String,
    pub link_text: Option<String>,
    pub created_at: i64,
}
impl From<&PostLink> for PostLinkRecord {
    fn from(v: &PostLink) -> Self {
        Self {
            id: v.id.clone(),
            source_post_id: v.source_post_id.clone(),
            target_post_id: v.target_post_id.clone(),
            link_text: v.link_text.clone(),
            created_at: v.created_at,
        }
    }
}
impl From<PostLinkRecord> for PostLink {
    fn from(v: PostLinkRecord) -> Self {
        Self {
            id: v.id,
            source_post_id: v.source_post_id,
            target_post_id: v.target_post_id,
            link_text: v.link_text,
            created_at: v.created_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable)]
#[diesel(table_name = schema::post_media, check_for_backend(Sqlite))]
pub struct PostMediaRecord {
    pub id: String,
    pub project_id: String,
    pub post_id: String,
    pub media_id: String,
    pub sort_order: i32,
    pub created_at: i64,
}
impl From<&PostMedia> for PostMediaRecord {
    fn from(v: &PostMedia) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            post_id: v.post_id.clone(),
            media_id: v.media_id.clone(),
            sort_order: v.sort_order,
            created_at: v.created_at,
        }
    }
}
impl From<PostMediaRecord> for PostMedia {
    fn from(v: PostMediaRecord) -> Self {
        Self {
            id: v.id,
            project_id: v.project_id,
            post_id: v.post_id,
            media_id: v.media_id,
            sort_order: v.sort_order,
            created_at: v.created_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::media, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct MediaRecord {
    pub id: String,
    pub project_id: String,
    pub filename: String,
    pub original_name: String,
    pub mime_type: String,
    pub size: i64,
    pub width: Option<i32>,
    pub height: Option<i32>,
    pub title: Option<String>,
    pub alt: Option<String>,
    pub caption: Option<String>,
    pub author: Option<String>,
    pub file_path: String,
    pub sidecar_path: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub checksum: Option<String>,
    pub tags: String,
    pub language: Option<String>,
}
impl From<&Media> for MediaRecord {
    fn from(v: &Media) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            filename: v.filename.clone(),
            original_name: v.original_name.clone(),
            mime_type: v.mime_type.clone(),
            size: v.size,
            width: v.width,
            height: v.height,
            title: v.title.clone(),
            alt: v.alt.clone(),
            caption: v.caption.clone(),
            author: v.author.clone(),
            file_path: v.file_path.clone(),
            sidecar_path: v.sidecar_path.clone(),
            created_at: v.created_at,
            updated_at: v.updated_at,
            checksum: v.checksum.clone(),
            tags: serde_json::to_string(&v.tags).unwrap_or_else(|_| "[]".into()),
            language: v.language.clone(),
        }
    }
}
impl TryFrom<MediaRecord> for Media {
    type Error = ConversionError;
    fn try_from(v: MediaRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id,
            project_id: v.project_id,
            filename: v.filename,
            original_name: v.original_name,
            mime_type: v.mime_type,
            size: v.size,
            width: v.width,
            height: v.height,
            title: v.title,
            alt: v.alt,
            caption: v.caption,
            author: v.author,
            language: v.language,
            file_path: v.file_path,
            sidecar_path: v.sidecar_path,
            checksum: v.checksum,
            tags: json_strings(&v.tags)?,
            created_at: v.created_at,
            updated_at: v.updated_at,
        })
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::media_translations, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct MediaTranslationRecord {
    pub id: String,
    pub project_id: String,
    pub translation_for: String,
    pub language: String,
    pub title: Option<String>,
    pub alt: Option<String>,
    pub caption: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl From<&MediaTranslation> for MediaTranslationRecord {
    fn from(v: &MediaTranslation) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            translation_for: v.translation_for.clone(),
            language: v.language.clone(),
            title: v.title.clone(),
            alt: v.alt.clone(),
            caption: v.caption.clone(),
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}
impl From<MediaTranslationRecord> for MediaTranslation {
    fn from(v: MediaTranslationRecord) -> Self {
        Self {
            id: v.id,
            project_id: v.project_id,
            translation_for: v.translation_for,
            language: v.language,
            title: v.title,
            alt: v.alt,
            caption: v.caption,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::tags, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct TagRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub color: Option<String>,
    pub post_template_slug: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl From<&Tag> for TagRecord {
    fn from(v: &Tag) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            name: v.name.clone(),
            color: v.color.clone(),
            post_template_slug: v.post_template_slug.clone(),
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}
impl From<TagRecord> for Tag {
    fn from(v: TagRecord) -> Self {
        Self {
            id: v.id,
            project_id: v.project_id,
            name: v.name,
            color: v.color,
            post_template_slug: v.post_template_slug,
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::templates, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct TemplateRecord {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub enabled: i32,
    pub version: i32,
    pub file_path: String,
    pub status: String,
    pub content: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl From<&Template> for TemplateRecord {
    fn from(v: &Template) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            slug: v.slug.clone(),
            title: v.title.clone(),
            kind: template_kind_to_str(&v.kind).into(),
            enabled: v.enabled as i32,
            version: v.version,
            file_path: v.file_path.clone(),
            status: template_status_to_str(&v.status).into(),
            content: v.content.clone(),
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}
impl TryFrom<TemplateRecord> for Template {
    type Error = ConversionError;
    fn try_from(v: TemplateRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id,
            project_id: v.project_id,
            slug: v.slug,
            title: v.title,
            kind: template_kind(&v.kind)?,
            enabled: v.enabled != 0,
            version: v.version,
            file_path: v.file_path,
            status: template_status(&v.status)?,
            content: v.content,
            created_at: v.created_at,
            updated_at: v.updated_at,
        })
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::scripts, check_for_backend(Sqlite))]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct ScriptRecord {
    pub id: String,
    pub project_id: String,
    pub slug: String,
    pub title: String,
    pub kind: String,
    pub entrypoint: String,
    pub enabled: i32,
    pub version: i32,
    pub file_path: String,
    pub status: String,
    pub content: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}
impl From<&Script> for ScriptRecord {
    fn from(v: &Script) -> Self {
        Self {
            id: v.id.clone(),
            project_id: v.project_id.clone(),
            slug: v.slug.clone(),
            title: v.title.clone(),
            kind: script_kind_to_str(&v.kind).into(),
            entrypoint: v.entrypoint.clone(),
            enabled: v.enabled as i32,
            version: v.version,
            file_path: v.file_path.clone(),
            status: script_status_to_str(&v.status).into(),
            content: v.content.clone(),
            created_at: v.created_at,
            updated_at: v.updated_at,
        }
    }
}
impl TryFrom<ScriptRecord> for Script {
    type Error = ConversionError;
    fn try_from(v: ScriptRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: v.id,
            project_id: v.project_id,
            slug: v.slug,
            title: v.title,
            kind: script_kind(&v.kind)?,
            entrypoint: v.entrypoint,
            enabled: v.enabled != 0,
            version: v.version,
            file_path: v.file_path,
            status: script_status(&v.status)?,
            content: v.content,
            created_at: v.created_at,
            updated_at: v.updated_at,
        })
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::settings, check_for_backend(Sqlite))]
pub struct SettingRecord {
    pub key: String,
    pub value: String,
    pub updated_at: i64,
}
impl From<SettingRecord> for Setting {
    fn from(v: SettingRecord) -> Self {
        Self {
            key: v.key,
            value: v.value,
            updated_at: v.updated_at,
        }
    }
}

#[derive(Queryable, Selectable, Insertable, AsChangeset)]
#[diesel(table_name = schema::generated_file_hashes, check_for_backend(Sqlite))]
pub struct GeneratedFileHashRecord {
    pub project_id: String,
    pub relative_path: String,
    pub content_hash: String,
    pub updated_at: i64,
}
impl From<&GeneratedFileHash> for GeneratedFileHashRecord {
    fn from(v: &GeneratedFileHash) -> Self {
        Self {
            project_id: v.project_id.clone(),
            relative_path: v.relative_path.clone(),
            content_hash: v.content_hash.clone(),
            updated_at: v.updated_at,
        }
    }
}
impl From<GeneratedFileHashRecord> for GeneratedFileHash {
    fn from(v: GeneratedFileHashRecord) -> Self {
        Self {
            project_id: v.project_id,
            relative_path: v.relative_path,
            content_hash: v.content_hash,
            updated_at: v.updated_at,
        }
    }
}

#[derive(Queryable, Selectable)]
#[diesel(table_name = schema::db_notifications, check_for_backend(Sqlite))]
pub struct DbNotificationRecord {
    pub id: i32,
    pub entity_type: String,
    pub entity_id: String,
    pub action: String,
    pub from_cli: i32,
    pub seen_at: Option<i64>,
    pub created_at: i64,
}
impl TryFrom<DbNotificationRecord> for DbNotification {
    type Error = ConversionError;
    fn try_from(v: DbNotificationRecord) -> Result<Self, Self::Error> {
        Ok(Self {
            id: i64::from(v.id),
            entity_type: notification_entity(&v.entity_type)?,
            entity_id: v.entity_id,
            action: notification_action(&v.action)?,
            from_cli: v.from_cli != 0,
            seen_at: v.seen_at,
            created_at: v.created_at,
        })
    }
}

pub(crate) fn convert<T, U>(value: T) -> diesel::QueryResult<U>
where
    U: TryFrom<T, Error = ConversionError>,
{
    value
        .try_into()
        .map_err(diesel::result::Error::DeserializationError)
}

pub(crate) fn convert_all<T, U>(values: Vec<T>) -> diesel::QueryResult<Vec<U>>
where
    U: TryFrom<T, Error = ConversionError>,
{
    values.into_iter().map(convert).collect()
}
