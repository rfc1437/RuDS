use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemKind {
    Post,
    Page,
    Media,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemStatus {
    New,
    Update,
    Conflict,
    ContentDuplicate,
    Missing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportResolution {
    Ignore,
    Overwrite,
    Import,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaxonomyKind {
    Category,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImportedSite {
    pub title: String,
    pub url: Option<String>,
    pub language: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportCandidate {
    pub kind: ImportItemKind,
    pub source_id: Option<i64>,
    pub title: String,
    pub slug: Option<String>,
    pub filename: Option<String>,
    pub relative_path: Option<String>,
    pub status: ImportItemStatus,
    pub resolution: Option<ImportResolution>,
    pub existing_id: Option<String>,
    pub author: Option<String>,
    pub excerpt: Option<String>,
    pub content: Option<String>,
    pub source_status: Option<String>,
    #[serde(default)]
    pub categories: Vec<String>,
    #[serde(default)]
    pub tags: Vec<String>,
    pub source_path: Option<String>,
    pub parent_source_id: Option<i64>,
    pub created_at: Option<i64>,
    pub updated_at: Option<i64>,
    pub published_at: Option<i64>,
    pub checksum: Option<String>,
    pub mime_type: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomyCandidate {
    pub kind: TaxonomyKind,
    pub name: String,
    pub slug: Option<String>,
    pub exists_in_project: bool,
    pub mapped_to: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImportCounts {
    pub new_count: usize,
    pub update_count: usize,
    pub conflict_count: usize,
    pub duplicate_count: usize,
    pub missing_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportDateBucket {
    pub year: i32,
    pub post_count: usize,
    pub media_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportMacroUsage {
    pub name: String,
    pub total_count: usize,
    #[serde(default)]
    pub post_slugs: Vec<String>,
    #[serde(default)]
    pub parameters: Vec<BTreeMap<String, String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImportReport {
    pub source_file: String,
    pub uploads_folder: Option<String>,
    pub site: ImportedSite,
    #[serde(default)]
    pub posts: Vec<ImportCandidate>,
    #[serde(default)]
    pub pages: Vec<ImportCandidate>,
    #[serde(default)]
    pub media: Vec<ImportCandidate>,
    #[serde(default)]
    pub taxonomies: Vec<TaxonomyCandidate>,
    pub post_counts: ImportCounts,
    pub page_counts: ImportCounts,
    pub media_counts: ImportCounts,
    #[serde(default)]
    pub date_distribution: Vec<ImportDateBucket>,
    #[serde(default)]
    pub macros: Vec<ImportMacroUsage>,
}

impl ImportReport {
    pub fn importable_count(&self) -> usize {
        let taxonomy = self
            .taxonomies
            .iter()
            .filter(|item| !item.exists_in_project && item.mapped_to.is_none())
            .count();
        taxonomy
            + self
                .posts
                .iter()
                .chain(&self.media)
                .chain(&self.pages)
                .filter(|item| match item.status {
                    ImportItemStatus::New => true,
                    ImportItemStatus::Conflict => matches!(
                        item.resolution,
                        Some(ImportResolution::Overwrite | ImportResolution::Import)
                    ),
                    _ => false,
                })
                .count()
    }
}

#[derive(
    Debug,
    Clone,
    PartialEq,
    Eq,
    Serialize,
    Deserialize,
    diesel::Queryable,
    diesel::Selectable,
    diesel::Insertable,
    diesel::AsChangeset,
)]
#[diesel(
    table_name = crate::db::schema::import_definitions,
    check_for_backend(diesel::sqlite::Sqlite)
)]
#[diesel(treat_none_as_default_value = false, treat_none_as_null = true)]
pub struct ImportDefinition {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub wxr_file_path: Option<String>,
    pub uploads_folder_path: Option<String>,
    pub last_analysis_result: Option<String>,
    pub created_at: i64,
    pub updated_at: i64,
}

impl ImportDefinition {
    pub fn analysis(&self) -> Result<Option<ImportReport>, serde_json::Error> {
        self.last_analysis_result
            .as_deref()
            .map(serde_json::from_str)
            .transpose()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportPhase {
    Taxonomy,
    Posts,
    Media,
    Pages,
    Complete,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportProgress {
    pub phase: ImportPhase,
    pub current: usize,
    pub total: usize,
    pub detail: String,
    pub eta_ms: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImportExecutionCounts {
    pub imported: usize,
    pub skipped: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct ImportExecutionResult {
    pub taxonomy: ImportExecutionCounts,
    pub posts: ImportExecutionCounts,
    pub media: ImportExecutionCounts,
    pub pages: ImportExecutionCounts,
}
