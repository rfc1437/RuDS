//! Local multilingual semantic indexing and duplicate detection.
//!
//! SQLite vectors are authoritative. Per-project USearch files are disposable,
//! validated caches that are rebuilt from those vectors when absent or corrupt.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use fastembed::{EmbeddingModel, TextEmbedding, TextInitOptions};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use usearch::{Index, IndexOptions, MetricKind, ScalarKind};

use crate::db::DbConnection as Connection;
use crate::db::queries::{embedding as qe, post as qp};
use crate::engine::{EngineError, EngineResult};
use crate::model::{DismissedDuplicatePair, EmbeddingKey, Post};
use crate::util::{application_data_dir, now_unix_ms};

pub const MODEL_ID: &str = "Xenova/multilingual-e5-small";
pub const MODEL_REPOSITORY: &str = "intfloat/multilingual-e5-small";
pub const DIMENSIONS: usize = 384;
pub const BATCH_SIZE: usize = 16;
pub const SEQUENCE_LENGTH: usize = 256;
pub const DUPLICATE_THRESHOLD: f32 = 0.92;
pub const DUPLICATE_PAGE_SIZE: usize = 500;
pub const DUPLICATE_NEIGHBOR_COUNT: usize = 21;
const SAVE_DEBOUNCE: Duration = Duration::from_secs(5);

pub trait EmbeddingBackend: Send + Sync {
    fn embed(&self, prefixed_texts: &[String]) -> Result<Vec<Vec<f32>>, String>;
}

struct NeuralBackend {
    model: Mutex<Option<TextEmbedding>>,
    cache_dir: PathBuf,
}

impl NeuralBackend {
    fn global() -> &'static Arc<Self> {
        static BACKEND: OnceLock<Arc<NeuralBackend>> = OnceLock::new();
        BACKEND.get_or_init(|| {
            Arc::new(Self {
                model: Mutex::new(None),
                cache_dir: application_data_dir().join("models"),
            })
        })
    }

    fn options(&self) -> TextInitOptions {
        let options = TextInitOptions::new(EmbeddingModel::MultilingualE5Small)
            .with_cache_dir(self.cache_dir.clone())
            .with_max_length(SEQUENCE_LENGTH)
            .with_show_download_progress(false);
        #[cfg(target_os = "macos")]
        let options = options
            .with_execution_providers(vec![ort::ep::CoreML::default().build().fail_silently()]);
        #[cfg(target_os = "windows")]
        let options = options
            .with_execution_providers(vec![ort::ep::DirectML::default().build().fail_silently()]);
        options
    }
}

impl EmbeddingBackend for NeuralBackend {
    fn embed(&self, prefixed_texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
        if prefixed_texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut guard = self
            .model
            .lock()
            .map_err(|_| "embedding model lock poisoned")?;
        if guard.is_none() {
            fs::create_dir_all(&self.cache_dir).map_err(|error| error.to_string())?;
            *guard = Some(
                TextEmbedding::try_new(self.options())
                    .map_err(|error| format!("could not load {MODEL_REPOSITORY}: {error}"))?,
            );
        }
        guard
            .as_mut()
            .expect("model initialized")
            .embed(prefixed_texts, Some(BATCH_SIZE))
            .map_err(|error| error.to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimilarPost {
    pub post_id: String,
    pub title: String,
    pub similarity: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DuplicatePair {
    pub post_id_a: String,
    pub title_a: String,
    pub post_id_b: String,
    pub title_b: String,
    pub similarity: f32,
    pub exact_match: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct DuplicateSearchResult {
    pub pairs: Vec<DuplicatePair>,
    pub has_more: bool,
}

#[derive(Debug, Serialize, Deserialize)]
struct IndexMetadata {
    dimensions: usize,
    labels: Vec<(u64, String)>,
}

struct CachedIndex {
    index: Index,
    labels: HashMap<u64, String>,
    dirty_since: Option<Instant>,
    index_path: PathBuf,
}

fn registry() -> &'static Mutex<HashMap<PathBuf, CachedIndex>> {
    static REGISTRY: OnceLock<Mutex<HashMap<PathBuf, CachedIndex>>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(HashMap::new()))
}

pub struct EmbeddingService<'a> {
    conn: &'a Connection,
    data_dir: &'a Path,
    cache_root: PathBuf,
    backend: Arc<dyn EmbeddingBackend>,
}

impl<'a> EmbeddingService<'a> {
    pub fn production(conn: &'a Connection, data_dir: &'a Path) -> Self {
        Self {
            conn,
            data_dir,
            cache_root: application_data_dir(),
            backend: NeuralBackend::global().clone(),
        }
    }

    pub fn with_backend(
        conn: &'a Connection,
        data_dir: &'a Path,
        cache_root: PathBuf,
        backend: Arc<dyn EmbeddingBackend>,
    ) -> Self {
        Self {
            conn,
            data_dir,
            cache_root,
            backend,
        }
    }

    pub fn enabled(&self) -> bool {
        crate::engine::meta::read_project_json(self.data_dir)
            .map(|metadata| metadata.semantic_similarity_enabled)
            .unwrap_or(false)
    }

    pub fn indexing_progress(&self, project_id: &str) -> EngineResult<(usize, usize)> {
        let indexed = qe::list_keys(self.conn, project_id)?.len();
        let total = qp::list_posts_by_project(self.conn, project_id)?.len();
        Ok((indexed, total))
    }

    pub fn content_hash_for_post(&self, post: &Post) -> EngineResult<String> {
        self.embedding_text(post).map(|text| hash_text(&text))
    }

    pub fn sync_post(&self, post: &Post) -> EngineResult<bool> {
        if !self.enabled() {
            return Ok(false);
        }
        let text = self.embedding_text(post)?;
        let content_hash = hash_text(&text);
        if qe::get_key_for_post(self.conn, &post.project_id, &post.id)?.is_some_and(|key| {
            key.content_hash == content_hash && decode_vector(&key.vector).is_ok()
        }) {
            return Ok(false);
        }
        let vector = self.embed_texts(&[text])?.pop().expect("one embedding");
        let existing = qe::get_key_for_post(self.conn, &post.project_id, &post.id)?;
        let key = EmbeddingKey {
            label: existing
                .map(|key| key.label)
                .unwrap_or(qe::max_label(self.conn)? + 1),
            post_id: post.id.clone(),
            project_id: post.project_id.clone(),
            content_hash,
            vector: encode_vector(&vector),
        };
        qe::upsert_key(self.conn, &key)?;
        self.rebuild_cached_index(&post.project_id, false)?;
        Ok(true)
    }

    pub fn remove_post(&self, project_id: &str, post_id: &str) -> EngineResult<()> {
        qe::delete_key_for_post(self.conn, project_id, post_id)?;
        qe::delete_dismissals_for_post(self.conn, project_id, post_id)?;
        self.rebuild_cached_index(project_id, false)
    }

    pub fn index_unindexed(&self, project_id: &str) -> EngineResult<Vec<String>> {
        self.index_unindexed_with_progress(project_id, |_, _| true)
    }

    pub fn index_unindexed_with_progress(
        &self,
        project_id: &str,
        mut on_progress: impl FnMut(usize, usize) -> bool,
    ) -> EngineResult<Vec<String>> {
        if !self.enabled() {
            return Ok(Vec::new());
        }
        let posts = qp::list_posts_by_project(self.conn, project_id)?;
        let live_ids = posts.iter().map(|post| post.id.clone()).collect::<Vec<_>>();
        qe::delete_stale_keys(self.conn, project_id, &live_ids)?;
        qe::delete_orphan_dismissals(self.conn, project_id, &live_ids)?;

        let existing = qe::list_keys(self.conn, project_id)?
            .into_iter()
            .map(|key| (key.post_id.clone(), key))
            .collect::<HashMap<_, _>>();
        let mut prepared = Vec::new();
        for post in &posts {
            let text = self.embedding_text(post)?;
            let hash = hash_text(&text);
            if existing
                .get(&post.id)
                .is_none_or(|key| key.content_hash != hash || decode_vector(&key.vector).is_err())
            {
                prepared.push((post, text, hash));
            }
        }
        let total = prepared.len();
        if !on_progress(0, total) {
            self.rebuild_cached_index(project_id, false)?;
            return Err(EngineError::Validation(
                "embedding indexing cancelled".into(),
            ));
        }
        let mut next_label = qe::max_label(self.conn)? + 1;
        let mut completed = 0;
        for chunk in prepared.chunks(BATCH_SIZE) {
            let vectors = self.embed_texts(
                &chunk
                    .iter()
                    .map(|(_, text, _)| text.clone())
                    .collect::<Vec<_>>(),
            )?;
            for ((post, _, hash), vector) in chunk.iter().zip(vectors) {
                let label = existing
                    .get(&post.id)
                    .map(|key| key.label)
                    .unwrap_or_else(|| {
                        let label = next_label;
                        next_label += 1;
                        label
                    });
                qe::upsert_key(
                    self.conn,
                    &EmbeddingKey {
                        label,
                        post_id: post.id.clone(),
                        project_id: project_id.to_string(),
                        content_hash: hash.clone(),
                        vector: encode_vector(&vector),
                    },
                )?;
            }
            completed += chunk.len();
            if !on_progress(completed, total) {
                self.rebuild_cached_index(project_id, false)?;
                return Err(EngineError::Validation(
                    "embedding indexing cancelled".into(),
                ));
            }
        }
        self.rebuild_cached_index(project_id, false)?;
        Ok(qe::list_keys(self.conn, project_id)?
            .into_iter()
            .map(|key| key.post_id)
            .collect())
    }

    pub fn reindex_all(&self, project_id: &str) -> EngineResult<Vec<String>> {
        self.reindex_all_with_progress(project_id, |_, _| true)
    }

    pub fn reindex_all_with_progress(
        &self,
        project_id: &str,
        mut on_progress: impl FnMut(usize, usize) -> bool,
    ) -> EngineResult<Vec<String>> {
        if !self.enabled() {
            return Ok(Vec::new());
        }
        let posts = qp::list_posts_by_project(self.conn, project_id)?;
        let live_ids = posts.iter().map(|post| post.id.clone()).collect::<Vec<_>>();
        qe::delete_stale_keys(self.conn, project_id, &live_ids)?;
        qe::delete_orphan_dismissals(self.conn, project_id, &live_ids)?;
        let existing = qe::list_keys(self.conn, project_id)?
            .into_iter()
            .map(|key| (key.post_id.clone(), key.label))
            .collect::<HashMap<_, _>>();
        let texts = posts
            .iter()
            .map(|post| self.embedding_text(post))
            .collect::<EngineResult<Vec<_>>>()?;
        let total = posts.len();
        if !on_progress(0, total) {
            self.rebuild_cached_index(project_id, false)?;
            return Err(EngineError::Validation(
                "embedding indexing cancelled".into(),
            ));
        }
        let mut next_label = qe::max_label(self.conn)? + 1;
        let mut completed = 0;
        for (post_chunk, text_chunk) in posts.chunks(BATCH_SIZE).zip(texts.chunks(BATCH_SIZE)) {
            let vectors = self.embed_texts(text_chunk)?;
            for ((post, text), vector) in post_chunk.iter().zip(text_chunk).zip(vectors) {
                let label = existing.get(&post.id).copied().unwrap_or_else(|| {
                    let label = next_label;
                    next_label += 1;
                    label
                });
                qe::upsert_key(
                    self.conn,
                    &EmbeddingKey {
                        label,
                        post_id: post.id.clone(),
                        project_id: project_id.to_string(),
                        content_hash: hash_text(text),
                        vector: encode_vector(&vector),
                    },
                )?;
            }
            completed += post_chunk.len();
            if !on_progress(completed, total) {
                self.rebuild_cached_index(project_id, false)?;
                return Err(EngineError::Validation(
                    "embedding indexing cancelled".into(),
                ));
            }
        }
        self.rebuild_cached_index(project_id, false)?;
        Ok(live_ids)
    }

    pub fn semantic_search(
        &self,
        project_id: &str,
        query: &str,
        limit: usize,
    ) -> EngineResult<Vec<SimilarPost>> {
        if !self.enabled() || query.trim().is_empty() || limit == 0 {
            return Ok(Vec::new());
        }
        self.index_unindexed(project_id)?;
        let vector = self
            .embed_texts(&[query.trim().to_string()])?
            .pop()
            .expect("one embedding");
        self.search_index(project_id, &vector, limit, None)
    }

    pub fn find_similar(&self, post_id: &str, limit: usize) -> EngineResult<Vec<SimilarPost>> {
        if !self.enabled() || limit == 0 {
            return Ok(Vec::new());
        }
        let post = qp::get_post_by_id(self.conn, post_id)?;
        self.sync_post(&post)?;
        let Some(key) = qe::get_key_for_post(self.conn, &post.project_id, post_id)? else {
            return Ok(Vec::new());
        };
        self.search_index(
            &post.project_id,
            &decode_vector(&key.vector)?,
            limit,
            Some(key.label as u64),
        )
    }

    pub fn compute_similarities(
        &self,
        source_post_id: &str,
        target_post_ids: &[String],
    ) -> EngineResult<HashMap<String, f32>> {
        if !self.enabled() {
            return Ok(HashMap::new());
        }
        let source = qp::get_post_by_id(self.conn, source_post_id)?;
        self.sync_post(&source)?;
        let Some(source_key) = qe::get_key_for_post(self.conn, &source.project_id, source_post_id)?
        else {
            return Ok(HashMap::new());
        };
        let source_vector = decode_vector(&source_key.vector)?;
        let targets = target_post_ids.iter().collect::<HashSet<_>>();
        Ok(qe::list_keys(self.conn, &source.project_id)?
            .into_iter()
            .filter(|key| key.post_id != source_post_id && targets.contains(&key.post_id))
            .filter_map(|key| {
                decode_vector(&key.vector)
                    .ok()
                    .map(|vector| (key.post_id, cosine(&source_vector, &vector)))
            })
            .collect())
    }

    pub fn suggest_tags(&self, post_id: &str) -> EngineResult<Vec<String>> {
        if !self.enabled() {
            return Ok(Vec::new());
        }
        let source = qp::get_post_by_id(self.conn, post_id)?;
        let current = source
            .tags
            .iter()
            .map(|tag| tag.to_lowercase())
            .collect::<HashSet<_>>();
        let Some(key) = qe::get_key_for_post(self.conn, &source.project_id, post_id)? else {
            return Ok(Vec::new());
        };
        let similar = self.search_index(
            &source.project_id,
            &decode_vector(&key.vector)?,
            10,
            Some(key.label as u64),
        )?;
        let mut scores = HashMap::<String, (String, f32)>::new();
        for neighbor in similar {
            if let Ok(post) = qp::get_post_by_id(self.conn, &neighbor.post_id) {
                for tag in post.tags {
                    let normalized = tag.to_lowercase();
                    if !current.contains(&normalized) {
                        scores
                            .entry(normalized)
                            .and_modify(|(_, score)| *score += neighbor.similarity)
                            .or_insert((tag, neighbor.similarity));
                    }
                }
            }
        }
        let mut ranked = scores.into_values().collect::<Vec<_>>();
        ranked.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        Ok(ranked.into_iter().take(5).map(|(tag, _)| tag).collect())
    }

    pub fn find_duplicates(
        &self,
        project_id: &str,
        page: usize,
    ) -> EngineResult<DuplicateSearchResult> {
        if !self.enabled() {
            return Ok(DuplicateSearchResult::default());
        }
        self.index_unindexed(project_id)?;
        let keys = qe::list_keys(self.conn, project_id)?;
        let dismissed = qe::list_dismissed_pairs(self.conn, project_id)?
            .into_iter()
            .map(|pair| (pair.post_id_a, pair.post_id_b))
            .collect::<HashSet<_>>();
        let posts = qp::list_posts_by_project(self.conn, project_id)?
            .into_iter()
            .map(|post| (post.id.clone(), post))
            .collect::<HashMap<_, _>>();
        let mut seen = HashSet::new();
        let mut pairs = Vec::new();
        for key in &keys {
            let vector = decode_vector(&key.vector)?;
            for neighbor in self.search_raw(
                project_id,
                &vector,
                DUPLICATE_NEIGHBOR_COUNT,
                Some(key.label as u64),
            )? {
                if neighbor.1 < DUPLICATE_THRESHOLD {
                    continue;
                }
                let Some(other_id) = neighbor.0 else { continue };
                let (a, b) = canonical_pair(&key.post_id, &other_id);
                if !seen.insert((a.clone(), b.clone()))
                    || dismissed.contains(&(a.clone(), b.clone()))
                {
                    continue;
                }
                let (Some(post_a), Some(post_b)) = (posts.get(&a), posts.get(&b)) else {
                    continue;
                };
                let exact_match = neighbor.1 >= 0.999_999
                    && post_a.title == post_b.title
                    && self.post_body(post_a)? == self.post_body(post_b)?;
                pairs.push(DuplicatePair {
                    post_id_a: a,
                    title_a: post_a.title.clone(),
                    post_id_b: b,
                    title_b: post_b.title.clone(),
                    similarity: neighbor.1,
                    exact_match,
                });
            }
        }
        pairs.sort_by(|a, b| {
            b.exact_match
                .cmp(&a.exact_match)
                .then_with(|| b.similarity.total_cmp(&a.similarity))
                .then_with(|| a.post_id_a.cmp(&b.post_id_a))
                .then_with(|| a.post_id_b.cmp(&b.post_id_b))
        });
        let end = page
            .saturating_add(1)
            .saturating_mul(DUPLICATE_PAGE_SIZE)
            .min(pairs.len());
        let has_more = end < pairs.len();
        pairs.truncate(end);
        Ok(DuplicateSearchResult { pairs, has_more })
    }

    pub fn dismiss_duplicate_pair(&self, post_id_a: &str, post_id_b: &str) -> EngineResult<()> {
        let post_a = qp::get_post_by_id(self.conn, post_id_a)?;
        let post_b = qp::get_post_by_id(self.conn, post_id_b)?;
        if post_id_a == post_id_b || post_a.project_id != post_b.project_id {
            return Err(EngineError::Validation(
                "duplicate pair must contain two posts in one project".into(),
            ));
        }
        let (a, b) = canonical_pair(post_id_a, post_id_b);
        qe::insert_dismissed_pair(
            self.conn,
            &DismissedDuplicatePair {
                id: uuid::Uuid::new_v4().to_string(),
                project_id: post_a.project_id,
                post_id_a: a,
                post_id_b: b,
                dismissed_at: now_unix_ms(),
            },
        )?;
        Ok(())
    }

    pub fn dismiss_duplicate_pairs(&self, pair_ids: &[(String, String)]) -> EngineResult<()> {
        let post_ids = pair_ids
            .iter()
            .flat_map(|(a, b)| [a.as_str(), b.as_str()])
            .collect::<HashSet<_>>();
        let posts = post_ids
            .into_iter()
            .map(|post_id| {
                qp::get_post_by_id(self.conn, post_id).map(|post| (post_id.to_string(), post))
            })
            .collect::<Result<HashMap<_, _>, _>>()?;
        let mut seen = HashSet::new();
        let mut dismissals = Vec::new();
        for (post_id_a, post_id_b) in pair_ids {
            let post_a = &posts[post_id_a];
            let post_b = &posts[post_id_b];
            if post_id_a == post_id_b || post_a.project_id != post_b.project_id {
                return Err(EngineError::Validation(
                    "duplicate pair must contain two posts in one project".into(),
                ));
            }
            let (a, b) = canonical_pair(post_id_a, post_id_b);
            if seen.insert((post_a.project_id.clone(), a.clone(), b.clone())) {
                dismissals.push(DismissedDuplicatePair {
                    id: uuid::Uuid::new_v4().to_string(),
                    project_id: post_a.project_id.clone(),
                    post_id_a: a,
                    post_id_b: b,
                    dismissed_at: now_unix_ms(),
                });
            }
        }
        for chunk in dismissals.chunks(100) {
            qe::insert_dismissed_pairs(self.conn, chunk)?;
        }
        Ok(())
    }

    pub fn flush_project(&self, project_id: &str) -> EngineResult<()> {
        let path = self.index_path(project_id);
        let mut registry = registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?;
        if let Some(cached) = registry.get_mut(&path) {
            persist_cached(cached)?;
        }
        Ok(())
    }

    pub fn flush_due() -> EngineResult<()> {
        let mut registry = registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?;
        for cached in registry.values_mut() {
            if cached
                .dirty_since
                .is_some_and(|at| at.elapsed() >= SAVE_DEBOUNCE)
            {
                persist_cached(cached)?;
            }
        }
        Ok(())
    }

    pub fn flush_all() -> EngineResult<()> {
        let mut registry = registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?;
        for cached in registry.values_mut() {
            persist_cached(cached)?;
        }
        Ok(())
    }

    pub fn forget_project(project_id: &str) {
        let index_path = application_data_dir()
            .join("projects")
            .join(project_id)
            .join("embeddings.usearch");
        if let Ok(mut indexes) = registry().lock() {
            indexes.remove(&index_path);
        }
        if let Some(project_cache_dir) = index_path.parent() {
            let _ = fs::remove_dir_all(project_cache_dir);
        }
    }

    fn embedding_text(&self, post: &Post) -> EngineResult<String> {
        Ok(format!("{}\n\n{}", post.title, self.post_body(post)?))
    }

    fn post_body(&self, post: &Post) -> EngineResult<String> {
        if let Some(content) = post.content.as_ref().filter(|content| !content.is_empty()) {
            return Ok(content.clone());
        }
        if post.file_path.is_empty() {
            return Ok(String::new());
        }
        let raw = match fs::read_to_string(self.data_dir.join(&post.file_path)) {
            Ok(raw) => raw,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => return Err(error.into()),
        };
        crate::util::frontmatter::read_post_file(&raw)
            .map(|(_, body)| body)
            .map_err(EngineError::Parse)
    }

    fn embed_texts(&self, texts: &[String]) -> EngineResult<Vec<Vec<f32>>> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed = texts
            .iter()
            .map(|text| format!("query: {text}"))
            .collect::<Vec<_>>();
        let vectors = self
            .backend
            .embed(&prefixed)
            .map_err(EngineError::Validation)?;
        if vectors.len() != texts.len() || vectors.iter().any(|vector| vector.len() != DIMENSIONS) {
            return Err(EngineError::Validation(format!(
                "{MODEL_ID} returned invalid embedding dimensions"
            )));
        }
        Ok(vectors.into_iter().map(normalize).collect())
    }

    fn search_index(
        &self,
        project_id: &str,
        vector: &[f32],
        limit: usize,
        exclude: Option<u64>,
    ) -> EngineResult<Vec<SimilarPost>> {
        let raw = self.search_raw(project_id, vector, limit, exclude)?;
        Ok(raw
            .into_iter()
            .filter_map(|(post_id, similarity)| {
                let post_id = post_id?;
                qp::get_post_by_id(self.conn, &post_id)
                    .ok()
                    .map(|post| SimilarPost {
                        post_id,
                        title: post.title,
                        similarity,
                    })
            })
            .collect())
    }

    fn search_raw(
        &self,
        project_id: &str,
        vector: &[f32],
        limit: usize,
        exclude: Option<u64>,
    ) -> EngineResult<Vec<(Option<String>, f32)>> {
        self.ensure_index(project_id)?;
        let path = self.index_path(project_id);
        let registry = registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?;
        let cached = registry.get(&path).expect("index ensured");
        if cached.index.size() == 0 {
            return Ok(Vec::new());
        }
        let count = (limit + usize::from(exclude.is_some())).min(cached.index.size());
        let matches = cached.index.search(vector, count).map_err(index_error)?;
        Ok(matches
            .keys
            .into_iter()
            .zip(matches.distances)
            .filter(|(label, _)| Some(*label) != exclude)
            .take(limit)
            .map(|(label, distance)| {
                (
                    cached.labels.get(&label).cloned(),
                    (1.0 - distance).max(0.0),
                )
            })
            .collect())
    }

    fn ensure_index(&self, project_id: &str) -> EngineResult<()> {
        let path = self.index_path(project_id);
        if registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?
            .contains_key(&path)
        {
            return Ok(());
        }
        let keys = qe::list_keys(self.conn, project_id)?;
        let expected = keys
            .iter()
            .map(|key| (key.label as u64, key.post_id.clone()))
            .collect::<HashMap<_, _>>();
        if let Ok(cached) = load_cached(&path, &expected) {
            registry()
                .lock()
                .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?
                .insert(path, cached);
            return Ok(());
        }
        self.rebuild_cached_index(project_id, true)
    }

    fn rebuild_cached_index(&self, project_id: &str, persist_now: bool) -> EngineResult<()> {
        let keys = qe::list_keys(self.conn, project_id)?;
        let index = new_index(keys.len())?;
        let mut labels = HashMap::new();
        for key in keys {
            let vector = decode_vector(&key.vector)?;
            index.add(key.label as u64, &vector).map_err(index_error)?;
            labels.insert(key.label as u64, key.post_id);
        }
        let path = self.index_path(project_id);
        let mut cached = CachedIndex {
            index,
            labels,
            dirty_since: Some(Instant::now()),
            index_path: path.clone(),
        };
        if persist_now {
            persist_cached(&mut cached)?;
        }
        registry()
            .lock()
            .map_err(|_| EngineError::Validation("embedding index lock poisoned".into()))?
            .insert(path, cached);
        Ok(())
    }

    fn index_path(&self, project_id: &str) -> PathBuf {
        self.cache_root
            .join("projects")
            .join(project_id)
            .join("embeddings.usearch")
    }
}

pub fn sync_post_best_effort(conn: &Connection, data_dir: &Path, post: &Post) {
    if let Err(error) = EmbeddingService::production(conn, data_dir).sync_post(post) {
        eprintln!("embedding unavailable for post {}: {error}", post.id);
    }
}

pub fn remove_post_best_effort(
    conn: &Connection,
    data_dir: &Path,
    project_id: &str,
    post_id: &str,
) {
    if let Err(error) =
        EmbeddingService::production(conn, data_dir).remove_post(project_id, post_id)
    {
        eprintln!("could not remove embedding for post {post_id}: {error}");
    }
}

fn new_index(capacity: usize) -> EngineResult<Index> {
    let index = Index::new(&IndexOptions {
        dimensions: DIMENSIONS,
        metric: MetricKind::Cos,
        quantization: ScalarKind::F32,
        connectivity: 16,
        expansion_add: 128,
        expansion_search: 64,
        ..IndexOptions::default()
    })
    .map_err(index_error)?;
    index.reserve(capacity.max(1)).map_err(index_error)?;
    Ok(index)
}

fn load_cached(path: &Path, expected: &HashMap<u64, String>) -> EngineResult<CachedIndex> {
    let metadata: IndexMetadata = serde_json::from_slice(&fs::read(meta_path(path))?)?;
    let labels = metadata.labels.into_iter().collect::<HashMap<_, _>>();
    if metadata.dimensions != DIMENSIONS || &labels != expected || !path.exists() {
        return Err(EngineError::Validation(
            "embedding index metadata is stale".into(),
        ));
    }
    let index = new_index(expected.len())?;
    index
        .load(path.to_string_lossy().as_ref())
        .map_err(index_error)?;
    if index.size() != expected.len() {
        return Err(EngineError::Validation(
            "embedding index size is stale".into(),
        ));
    }
    Ok(CachedIndex {
        index,
        labels,
        dirty_since: None,
        index_path: path.to_path_buf(),
    })
}

fn persist_cached(cached: &mut CachedIndex) -> EngineResult<()> {
    if cached.dirty_since.is_none() {
        return Ok(());
    }
    if let Some(parent) = cached.index_path.parent() {
        fs::create_dir_all(parent)?;
    }
    cached
        .index
        .save(cached.index_path.to_string_lossy().as_ref())
        .map_err(index_error)?;
    let mut labels = cached
        .labels
        .iter()
        .map(|(label, post_id)| (*label, post_id.clone()))
        .collect::<Vec<_>>();
    labels.sort_by_key(|(label, _)| *label);
    let metadata = serde_json::to_vec(&IndexMetadata {
        dimensions: DIMENSIONS,
        labels,
    })?;
    fs::write(meta_path(&cached.index_path), metadata)?;
    cached.dirty_since = None;
    Ok(())
}

fn meta_path(path: &Path) -> PathBuf {
    PathBuf::from(format!("{}.meta.json", path.display()))
}

fn index_error(error: impl std::fmt::Display) -> EngineError {
    EngineError::Validation(format!("embedding index error: {error}"))
}

pub fn hash_text(text: &str) -> String {
    format!("{:x}", Sha256::digest(text.as_bytes()))
}

pub fn encode_vector(vector: &[f32]) -> Vec<u8> {
    vector
        .iter()
        .flat_map(|value| value.to_le_bytes())
        .collect()
}

pub fn decode_vector(bytes: &[u8]) -> EngineResult<Vec<f32>> {
    if bytes.len() != DIMENSIONS * size_of::<f32>() {
        return Err(EngineError::Validation(format!(
            "invalid embedding vector length: {}",
            bytes.len()
        )));
    }
    Ok(bytes
        .chunks_exact(4)
        .map(|chunk| f32::from_le_bytes(chunk.try_into().expect("four-byte chunk")))
        .collect())
}

fn normalize(mut vector: Vec<f32>) -> Vec<f32> {
    let norm = vector.iter().map(|value| value * value).sum::<f32>().sqrt();
    if norm > 0.0 {
        for value in &mut vector {
            *value /= norm;
        }
    }
    vector
}

fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b)
        .map(|(left, right)| left * right)
        .sum::<f32>()
        .clamp(0.0, 1.0)
}

fn canonical_pair(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vector_blob_is_exactly_1536_bytes_and_round_trips() {
        let vector = (0..DIMENSIONS)
            .map(|index| index as f32 / 10.0)
            .collect::<Vec<_>>();
        let encoded = encode_vector(&vector);
        assert_eq!(encoded.len(), 1536);
        assert_eq!(decode_vector(&encoded).unwrap(), vector);
        assert!(decode_vector(&encoded[..100]).is_err());
    }

    #[test]
    fn content_hash_uses_normative_title_blank_line_body_source() {
        assert_eq!(
            hash_text("Title\n\nBody"),
            "45777c14d90fa79dc6ce71ceb6f81cced62c929c472199547cfd939a54e954c6"
        );
    }

    #[test]
    fn model_selection_is_real_multilingual_e5_small() {
        let info = TextEmbedding::get_model_info(&EmbeddingModel::MultilingualE5Small).unwrap();
        assert_eq!(MODEL_ID, "Xenova/multilingual-e5-small");
        assert_eq!(info.model_code, MODEL_REPOSITORY);
        assert_eq!(info.dim, DIMENSIONS);
        #[cfg(target_os = "macos")]
        {
            use ort::ep::ExecutionProvider;
            assert!(ort::ep::CoreML::default().is_available().unwrap());
        }
    }

    #[test]
    fn usearch_index_uses_the_normative_hnsw_configuration() {
        let index = new_index(1).unwrap();
        assert_eq!(index.dimensions(), DIMENSIONS);
        assert_eq!(index.metric_kind(), MetricKind::Cos);
        assert_eq!(index.connectivity(), 16);
        assert_eq!(index.expansion_add(), 128);
        assert_eq!(index.expansion_search(), 64);
    }

    #[test]
    #[ignore = "downloads the real multilingual model; run for release verification"]
    fn real_model_is_multilingual_and_reloads_from_its_local_cache() {
        let cache = tempfile::tempdir().unwrap();
        let backend = NeuralBackend {
            model: Mutex::new(None),
            cache_dir: cache.path().into(),
        };
        let texts = vec![
            "query: A rocket launches into space".to_string(),
            "query: Eine Rakete startet in den Weltraum".to_string(),
            "query: Baking bread in the kitchen".to_string(),
        ];
        let first = backend.embed(&texts).unwrap();
        assert_eq!(
            first.iter().map(Vec::len).collect::<Vec<_>>(),
            vec![DIMENSIONS; 3]
        );
        assert!(cosine(&first[0], &first[1]) > cosine(&first[0], &first[2]));

        let cached_backend = NeuralBackend {
            model: Mutex::new(None),
            cache_dir: cache.path().into(),
        };
        let cached = cached_backend.embed(&texts[..1]).unwrap();
        assert_eq!(cached[0].len(), DIMENSIONS);
    }

    use crate::db::Database;
    use crate::model::{PostStatus, Project};
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FixtureBackend(AtomicUsize);

    impl FixtureBackend {
        fn new() -> Self {
            Self(AtomicUsize::new(0))
        }
        fn embedded(&self) -> usize {
            self.0.load(Ordering::SeqCst)
        }
    }

    impl EmbeddingBackend for FixtureBackend {
        fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, String> {
            self.0.fetch_add(texts.len(), Ordering::SeqCst);
            Ok(texts
                .iter()
                .map(|text| {
                    let text = text.to_lowercase();
                    let mut vector = vec![0.0; DIMENSIONS];
                    if text.contains("rocket") || text.contains("rakete") || text.contains("space")
                    {
                        vector[0] = 1.0;
                        vector[1] = 0.02;
                    } else if text.contains("bread") || text.contains("brot") {
                        vector[1] = 1.0;
                    } else {
                        vector[2] = 1.0;
                    }
                    vector
                })
                .collect())
        }
    }

    fn setup_service(
        enabled: bool,
    ) -> (
        Database,
        tempfile::TempDir,
        tempfile::TempDir,
        String,
        Arc<FixtureBackend>,
    ) {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        let data = tempfile::tempdir().unwrap();
        let cache = tempfile::tempdir().unwrap();
        crate::engine::meta::startup_sync(data.path()).unwrap();
        let mut metadata = crate::engine::meta::read_project_json(data.path()).unwrap();
        metadata.semantic_similarity_enabled = enabled;
        crate::engine::meta::write_project_json(data.path(), &metadata).unwrap();
        let project_id = uuid::Uuid::new_v4().to_string();
        crate::db::queries::project::insert_project(
            db.conn(),
            &Project {
                id: project_id.clone(),
                name: "Semantic".into(),
                slug: format!("semantic-{project_id}"),
                description: None,
                data_path: Some(data.path().to_string_lossy().into()),
                is_active: true,
                created_at: 1,
                updated_at: 1,
            },
        )
        .unwrap();
        (db, data, cache, project_id, Arc::new(FixtureBackend::new()))
    }

    fn insert_post(
        db: &Database,
        project_id: &str,
        id: &str,
        title: &str,
        body: &str,
        tags: &[&str],
    ) -> Post {
        let post = Post {
            id: id.into(),
            project_id: project_id.into(),
            title: title.into(),
            slug: id.into(),
            excerpt: None,
            content: Some(body.into()),
            status: PostStatus::Draft,
            author: None,
            language: Some("en".into()),
            do_not_translate: false,
            template_slug: None,
            file_path: String::new(),
            checksum: None,
            tags: tags.iter().map(|tag| (*tag).into()).collect(),
            categories: vec![],
            published_title: None,
            published_content: None,
            published_tags: None,
            published_categories: None,
            published_excerpt: None,
            created_at: 1,
            updated_at: 1,
            published_at: None,
        };
        qp::insert_post(db.conn(), &post).unwrap();
        post
    }

    #[test]
    fn lifecycle_is_gated_hash_cached_and_recovers_disposable_index_from_db() {
        let (db, data, cache, project_id, backend) = setup_service(false);
        let post = insert_post(
            &db,
            &project_id,
            "space",
            "Space",
            "rocket launch",
            &["space"],
        );
        let service = EmbeddingService::with_backend(
            db.conn(),
            data.path(),
            cache.path().into(),
            backend.clone(),
        );
        assert!(!service.sync_post(&post).unwrap());
        assert_eq!(backend.embedded(), 0);

        let mut metadata = crate::engine::meta::read_project_json(data.path()).unwrap();
        metadata.semantic_similarity_enabled = true;
        crate::engine::meta::write_project_json(data.path(), &metadata).unwrap();
        assert_eq!(service.index_unindexed(&project_id).unwrap(), vec!["space"]);
        assert_eq!(backend.embedded(), 1);
        assert_eq!(
            qe::list_keys(db.conn(), &project_id).unwrap()[0]
                .vector
                .len(),
            1536
        );
        service.index_unindexed(&project_id).unwrap();
        assert_eq!(backend.embedded(), 1, "unchanged hash must skip inference");

        let path = service.index_path(&project_id);
        assert!(!path.exists(), "index save must be debounced");
        service.flush_project(&project_id).unwrap();
        assert!(path.exists());
        assert!(meta_path(&path).exists());

        registry().lock().unwrap().remove(&path);
        fs::write(&path, b"corrupt index").unwrap();
        let results = service.semantic_search(&project_id, "rocket", 5).unwrap();
        assert_eq!(results[0].post_id, "space");
        service.flush_project(&project_id).unwrap();
        assert!(fs::metadata(&path).unwrap().len() > b"corrupt index".len() as u64);

        let mut changed = qp::get_post_by_id(db.conn(), "space").unwrap();
        changed.content = Some("Bread baking".into());
        qp::update_post(db.conn(), &changed).unwrap();
        assert!(service.sync_post(&changed).unwrap());
        assert_eq!(backend.embedded(), 3); // query + changed post
        insert_post(&db, &project_id, "other", "Other", "Other body", &[]);
        service.dismiss_duplicate_pair("space", "other").unwrap();
        service.remove_post(&project_id, "space").unwrap();
        assert_eq!(qe::list_keys(db.conn(), &project_id).unwrap().len(), 0);
        assert!(
            qe::list_dismissed_pairs(db.conn(), &project_id)
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn background_indexing_reports_progress_and_honors_cancellation_before_inference() {
        let (db, data, cache, project_id, backend) = setup_service(true);
        insert_post(&db, &project_id, "space", "Space", "rocket launch", &[]);
        let service = EmbeddingService::with_backend(
            db.conn(),
            data.path(),
            cache.path().into(),
            backend.clone(),
        );

        let cancelled = service.index_unindexed_with_progress(&project_id, |_, _| false);
        assert!(cancelled.is_err());
        assert_eq!(backend.embedded(), 0);
        assert!(qe::list_keys(db.conn(), &project_id).unwrap().is_empty());

        let mut progress = Vec::new();
        let indexed = service
            .index_unindexed_with_progress(&project_id, |current, total| {
                progress.push((current, total));
                true
            })
            .unwrap();
        assert_eq!(indexed, vec!["space"]);
        assert_eq!(progress, vec![(0, 1), (1, 1)]);
    }

    #[test]
    fn semantic_queries_tags_duplicates_and_dismissals_match_spec() {
        let (db, data, cache, project_id, backend) = setup_service(true);
        insert_post(
            &db,
            &project_id,
            "alpha",
            "Space",
            "rocket launch mission",
            &["space"],
        );
        insert_post(
            &db,
            &project_id,
            "beta",
            "Raumfahrt",
            "Rakete im All",
            &["space", "science"],
        );
        insert_post(
            &db,
            &project_id,
            "exact-a",
            "Exact",
            "rocket duplicate",
            &[],
        );
        insert_post(
            &db,
            &project_id,
            "exact-b",
            "Exact",
            "rocket duplicate",
            &[],
        );
        insert_post(
            &db,
            &project_id,
            "bread",
            "Bread",
            "bread baking oven",
            &["food"],
        );
        let service =
            EmbeddingService::with_backend(db.conn(), data.path(), cache.path().into(), backend);
        service.index_unindexed(&project_id).unwrap();

        let similar = service.find_similar("alpha", 4).unwrap();
        assert_eq!(similar.last().unwrap().post_id, "bread");
        assert!(similar[0].similarity > similar.last().unwrap().similarity);
        let search = service.semantic_search(&project_id, "Rakete", 3).unwrap();
        assert!(search.iter().all(|result| result.post_id != "bread"));
        let scores = service
            .compute_similarities("alpha", &["beta".into(), "bread".into()])
            .unwrap();
        assert!(scores["beta"] > scores["bread"]);
        assert_eq!(service.suggest_tags("alpha").unwrap()[0], "science");

        let duplicates = service.find_duplicates(&project_id, 0).unwrap();
        let exact = duplicates
            .pairs
            .iter()
            .find(|pair| {
                HashSet::from([pair.post_id_a.as_str(), pair.post_id_b.as_str()])
                    == HashSet::from(["exact-a", "exact-b"])
            })
            .unwrap();
        assert!(exact.exact_match);
        service
            .dismiss_duplicate_pair("exact-b", "exact-a")
            .unwrap();
        assert_eq!(
            qe::list_dismissed_pairs(db.conn(), &project_id).unwrap()[0].post_id_a,
            "exact-a"
        );
        let filtered = service.find_duplicates(&project_id, 0).unwrap();
        assert!(
            !filtered
                .pairs
                .iter()
                .any(|pair| pair.post_id_a == "exact-a" && pair.post_id_b == "exact-b")
        );
    }

    #[test]
    fn tag_suggestions_read_the_existing_index_without_running_inference() {
        let (db, data, cache, project_id, backend) = setup_service(true);
        insert_post(
            &db,
            &project_id,
            "source",
            "Space",
            "rocket launch",
            &["space"],
        );
        insert_post(
            &db,
            &project_id,
            "neighbor",
            "Science",
            "rocket mission",
            &["science"],
        );
        let service = EmbeddingService::with_backend(
            db.conn(),
            data.path(),
            cache.path().into(),
            backend.clone(),
        );
        service.index_unindexed(&project_id).unwrap();
        let inference_count = backend.embedded();
        let mut changed = qp::get_post_by_id(db.conn(), "source").unwrap();
        changed.content = Some("content changed after indexing".into());
        qp::update_post(db.conn(), &changed).unwrap();

        assert_eq!(service.suggest_tags("source").unwrap(), vec!["science"]);
        assert_eq!(backend.embedded(), inference_count);
    }

    #[test]
    fn duplicate_search_paginates_and_batch_dismisses_in_chunks() {
        let (db, data, cache, project_id, backend) = setup_service(true);
        for index in 0..60 {
            insert_post(
                &db,
                &project_id,
                &format!("post-{index:02}"),
                "Same",
                "rocket duplicate",
                &[],
            );
        }
        let service =
            EmbeddingService::with_backend(db.conn(), data.path(), cache.path().into(), backend);
        service.index_unindexed(&project_id).unwrap();
        let first = service.find_duplicates(&project_id, 0).unwrap();
        assert_eq!(first.pairs.len(), DUPLICATE_PAGE_SIZE);
        assert!(first.has_more);
        let mut selected = first
            .pairs
            .iter()
            .take(205)
            .map(|pair| (pair.post_id_b.clone(), pair.post_id_a.clone()))
            .collect::<Vec<_>>();
        selected.push((selected[0].1.clone(), selected[0].0.clone()));
        service.dismiss_duplicate_pairs(&selected).unwrap();
        assert_eq!(
            qe::list_dismissed_pairs(db.conn(), &project_id)
                .unwrap()
                .len(),
            205
        );
        let remaining = service.find_duplicates(&project_id, 1).unwrap();
        assert!(
            remaining
                .pairs
                .iter()
                .all(|pair| !selected.contains(&(pair.post_id_b.clone(), pair.post_id_a.clone())))
        );
    }

    #[test]
    fn project_indexes_and_search_results_are_isolated_while_backend_is_shared() {
        let (db, data, cache, first_project, backend) = setup_service(true);
        let second_project = uuid::Uuid::new_v4().to_string();
        crate::db::queries::project::insert_project(
            db.conn(),
            &Project {
                id: second_project.clone(),
                name: "Second".into(),
                slug: format!("second-{second_project}"),
                description: None,
                data_path: Some(data.path().to_string_lossy().into()),
                is_active: false,
                created_at: 2,
                updated_at: 2,
            },
        )
        .unwrap();
        insert_post(
            &db,
            &first_project,
            "first-space",
            "Space",
            "rocket mission",
            &[],
        );
        insert_post(
            &db,
            &second_project,
            "second-bread",
            "Bread",
            "bread baking",
            &[],
        );
        let service =
            EmbeddingService::with_backend(db.conn(), data.path(), cache.path().into(), backend);
        service.index_unindexed(&first_project).unwrap();
        service.index_unindexed(&second_project).unwrap();
        assert_ne!(
            service.index_path(&first_project),
            service.index_path(&second_project)
        );
        assert_eq!(
            service
                .semantic_search(&first_project, "rocket", 5)
                .unwrap()[0]
                .post_id,
            "first-space"
        );
        assert_eq!(
            service
                .semantic_search(&second_project, "bread", 5)
                .unwrap()[0]
                .post_id,
            "second-bread"
        );
    }
}
