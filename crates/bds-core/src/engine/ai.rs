use std::collections::BTreeMap;
use std::net::IpAddr;
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

use crate::db::DbConnection as Connection;
use keyring::Entry;
use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::db::queries::setting;
use crate::engine::{EngineError, EngineResult};
use crate::util::now_unix_ms;

const KEYRING_SERVICE: &str = "RuDS";
const KEYRING_SETTING_PREFIX: &str = "ai.endpoint";
static TEST_API_KEYS: LazyLock<Mutex<BTreeMap<String, String>>> =
    LazyLock::new(|| Mutex::new(BTreeMap::new()));

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AiEndpointKind {
    Online,
    Airplane,
}

impl AiEndpointKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Online => "online",
            Self::Airplane => "airplane",
        }
    }

    fn settings_prefix(self) -> String {
        format!("ai.endpoint.{}", self.as_str())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiEndpointConfig {
    pub kind: AiEndpointKind,
    pub url: String,
    pub model: String,
    pub api_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StoredAiEndpointConfig {
    pub kind: AiEndpointKind,
    pub url: String,
    pub model: String,
    pub api_key_configured: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AiSettings {
    pub offline_mode: bool,
    pub system_prompt: String,
    pub online: AiModeSettings,
    pub airplane: AiModeSettings,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct AiModeSettings {
    pub endpoint: StoredAiEndpointConfig,
    pub title_model: Option<String>,
    pub image_model: Option<String>,
    pub chat_supports_tools: Option<bool>,
    pub image_supports_vision: Option<bool>,
    pub models: Vec<AiModelInfo>,
}

impl AiSettings {
    pub fn active(&self) -> &AiModeSettings {
        if self.offline_mode {
            &self.airplane
        } else {
            &self.online
        }
    }
}

impl Default for StoredAiEndpointConfig {
    fn default() -> Self {
        Self {
            kind: AiEndpointKind::Online,
            url: String::new(),
            model: String::new(),
            api_key_configured: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiModelInfo {
    pub id: String,
    pub name: String,
    pub context_window: Option<u64>,
    pub max_output_tokens: Option<u64>,
    pub supports_tools: bool,
    pub supports_vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneShotOperation {
    AnalyzeTaxonomy,
    MapImportTaxonomy,
    AnalyzePost,
    DetectLanguage,
    TranslatePost { target_language: String },
    AnalyzeImage,
    TranslateMedia { target_language: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OneShotRequest {
    pub operation: OneShotOperation,
    pub content: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaxonomySuggestion {
    pub tags: Vec<String>,
    pub categories: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportTaxonomyMapping {
    pub category_mappings: BTreeMap<String, String>,
    pub tag_mappings: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PostAnalysisResult {
    pub title: String,
    pub excerpt: String,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageDetectionResult {
    pub language_code: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TranslationResult {
    pub title: String,
    pub excerpt: String,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImageAnalysisResult {
    pub title: String,
    pub alt: String,
    pub caption: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MediaTranslationResult {
    pub title: String,
    pub alt: String,
    pub caption: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub cache_read_tokens: Option<u64>,
    pub cache_write_tokens: Option<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneShotResponse {
    Taxonomy(TaxonomySuggestion),
    ImportTaxonomyMapping(ImportTaxonomyMapping),
    PostAnalysis(PostAnalysisResult),
    LanguageDetection(LanguageDetectionResult),
    Translation(TranslationResult),
    ImageAnalysis(ImageAnalysisResult),
    MediaTranslation(MediaTranslationResult),
}

pub fn load_ai_settings(conn: &Connection, offline_mode: bool) -> EngineResult<AiSettings> {
    let legacy_chat_model = get_optional_setting(conn, "ai.default_model")?;
    let legacy_title_model = get_optional_setting(conn, "ai.title_model")?;
    let legacy_image_model = get_optional_setting(conn, "ai.image_model")?;
    let system_prompt = get_optional_setting(conn, "ai.system_prompt")?.unwrap_or_default();

    Ok(AiSettings {
        offline_mode,
        system_prompt,
        online: load_mode_settings(
            conn,
            AiEndpointKind::Online,
            legacy_chat_model.as_deref(),
            legacy_title_model.as_deref(),
            legacy_image_model.as_deref(),
        )?,
        airplane: load_mode_settings(
            conn,
            AiEndpointKind::Airplane,
            legacy_chat_model.as_deref(),
            legacy_title_model.as_deref(),
            legacy_image_model.as_deref(),
        )?,
    })
}

pub fn save_endpoint(conn: &Connection, endpoint: &AiEndpointConfig) -> EngineResult<()> {
    validate_endpoint_config(endpoint)?;
    let checked_at = now_unix_ms();
    set_setting(
        conn,
        &endpoint_setting_key(endpoint.kind, "url"),
        endpoint.url.trim(),
        checked_at,
    )?;
    set_setting(
        conn,
        &endpoint_setting_key(endpoint.kind, "model"),
        endpoint.model.trim(),
        checked_at,
    )?;
    set_setting(conn, "ai.default_model", "", checked_at)?;
    if let Some(api_key) = &endpoint.api_key {
        save_endpoint_api_key_at(conn, endpoint.kind, api_key, checked_at)?;
    }
    Ok(())
}

pub fn save_online_api_key(conn: &Connection, api_key: &str) -> EngineResult<()> {
    save_endpoint_api_key(conn, AiEndpointKind::Online, api_key)
}

pub fn save_endpoint_api_key(
    conn: &Connection,
    kind: AiEndpointKind,
    api_key: &str,
) -> EngineResult<()> {
    save_endpoint_api_key_at(conn, kind, api_key, now_unix_ms())
}

fn save_endpoint_api_key_at(
    conn: &Connection,
    kind: AiEndpointKind,
    api_key: &str,
    updated_at: i64,
) -> EngineResult<()> {
    let configured = !api_key.trim().is_empty();
    if configured {
        store_endpoint_api_key(kind, api_key.trim())?;
    } else {
        delete_endpoint_api_key(kind)?;
    }
    set_setting(
        conn,
        &endpoint_setting_key(kind, "api_key_configured"),
        if configured { "true" } else { "false" },
        updated_at,
    )
}

pub fn save_model_preferences(
    conn: &Connection,
    kind: AiEndpointKind,
    title_model: Option<&str>,
    image_model: Option<&str>,
    chat_supports_tools: Option<bool>,
    image_supports_vision: Option<bool>,
) -> EngineResult<()> {
    let updated_at = now_unix_ms();
    set_optional_setting(
        conn,
        &endpoint_setting_key(kind, "title_model"),
        title_model,
        updated_at,
    )?;
    set_optional_setting(
        conn,
        &endpoint_setting_key(kind, "image_model"),
        image_model,
        updated_at,
    )?;
    set_optional_bool_setting(
        conn,
        &endpoint_setting_key(kind, "chat_supports_tools"),
        chat_supports_tools,
        updated_at,
    )?;
    set_optional_bool_setting(
        conn,
        &endpoint_setting_key(kind, "image_supports_vision"),
        image_supports_vision,
        updated_at,
    )?;
    set_setting(conn, "ai.title_model", "", updated_at)?;
    set_setting(conn, "ai.image_model", "", updated_at)?;
    Ok(())
}

pub fn save_endpoint_models(
    conn: &Connection,
    kind: AiEndpointKind,
    models: &[AiModelInfo],
) -> EngineResult<()> {
    use crate::db::schema::ai_endpoint_models::dsl;
    use diesel::prelude::*;

    let updated_at = now_unix_ms();
    conn.with(|connection| {
        connection.transaction(|connection| {
            diesel::delete(dsl::ai_endpoint_models.filter(dsl::kind.eq(kind.as_str())))
                .execute(connection)?;
            for model in models {
                diesel::insert_into(dsl::ai_endpoint_models)
                    .values((
                        dsl::kind.eq(kind.as_str()),
                        dsl::model_id.eq(&model.id),
                        dsl::label.eq(&model.name),
                        dsl::context_window.eq(model.context_window.map(|value| value as i32)),
                        dsl::max_output_tokens
                            .eq(model.max_output_tokens.map(|value| value as i32)),
                        dsl::supports_tools.eq(i32::from(model.supports_tools)),
                        dsl::supports_vision.eq(i32::from(model.supports_vision)),
                        dsl::updated_at.eq(updated_at),
                    ))
                    .execute(connection)?;
            }
            diesel::QueryResult::Ok(())
        })
    })?;
    Ok(())
}

pub fn load_endpoint_models(
    conn: &Connection,
    kind: AiEndpointKind,
) -> EngineResult<Vec<AiModelInfo>> {
    use crate::db::schema::ai_endpoint_models::dsl;
    use diesel::prelude::*;

    let rows = conn.with(|connection| {
        dsl::ai_endpoint_models
            .filter(dsl::kind.eq(kind.as_str()))
            .order(dsl::label.asc())
            .select((
                dsl::model_id,
                dsl::label,
                dsl::context_window,
                dsl::max_output_tokens,
                dsl::supports_tools,
                dsl::supports_vision,
            ))
            .load::<(String, String, Option<i32>, Option<i32>, i32, i32)>(connection)
    })?;
    Ok(rows
        .into_iter()
        .map(
            |(id, label, context_window, max_output_tokens, supports_tools, supports_vision)| {
                AiModelInfo {
                    id,
                    name: label,
                    context_window: context_window.map(|value| value.max(0) as u64),
                    max_output_tokens: max_output_tokens.map(|value| value.max(0) as u64),
                    supports_tools: supports_tools != 0,
                    supports_vision: supports_vision != 0,
                }
            },
        )
        .collect())
}

pub fn save_system_prompt(conn: &Connection, system_prompt: &str) -> EngineResult<()> {
    set_setting(conn, "ai.system_prompt", system_prompt, now_unix_ms())
}

pub fn active_endpoint(conn: &Connection, offline_mode: bool) -> EngineResult<AiEndpointConfig> {
    let kind = if offline_mode {
        AiEndpointKind::Airplane
    } else {
        AiEndpointKind::Online
    };
    let stored = load_endpoint(conn, kind)?;
    if stored.url.trim().is_empty() {
        return Err(EngineError::Validation(format!(
            "AI unavailable - configure {} endpoint in Settings",
            kind.as_str()
        )));
    }
    if kind == AiEndpointKind::Online && !stored.api_key_configured {
        return Err(EngineError::Validation(
            "AI unavailable - configure online endpoint in Settings".to_string(),
        ));
    }
    let api_key = if stored.api_key_configured {
        let password = read_endpoint_api_key(kind)?.ok_or_else(|| {
            EngineError::Validation(format!(
                "AI unavailable - configure {} endpoint in Settings",
                kind.as_str()
            ))
        })?;
        if password.trim().is_empty() {
            return Err(EngineError::Validation(format!(
                "AI unavailable - configure {} endpoint in Settings",
                kind.as_str()
            )));
        }
        Some(password)
    } else {
        None
    };

    Ok(AiEndpointConfig {
        kind,
        url: stored.url,
        model: stored.model,
        api_key,
    })
}

pub fn load_endpoint_api_key(kind: AiEndpointKind) -> EngineResult<Option<String>> {
    read_endpoint_api_key(kind)
}

pub fn refresh_model_catalog(endpoint: &AiEndpointConfig) -> EngineResult<Vec<AiModelInfo>> {
    validate_endpoint_access(endpoint)?;
    let client = build_http_client_for_endpoint(&endpoint.url)?;
    let request = client.get(models_url(&endpoint.url));
    let response = with_auth(request, endpoint).send()?.error_for_status()?;
    let body: Value = response.json()?;
    let models = body.get("data").and_then(Value::as_array).ok_or_else(|| {
        EngineError::Parse("model catalog response missing data array".to_string())
    })?;

    let mut result = Vec::new();
    for model in models {
        let id = model
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| EngineError::Parse("model entry missing id".to_string()))?
            .to_string();
        let name = model
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or(&id)
            .to_string();
        let context_window = model
            .get("context_window")
            .or_else(|| model.get("contextWindow"))
            .and_then(Value::as_u64);
        let max_output_tokens = model
            .get("max_output_tokens")
            .or_else(|| model.get("maxOutputTokens"))
            .and_then(Value::as_u64);
        let supports_vision = model
            .get("modalities")
            .and_then(Value::as_array)
            .map(|modalities| {
                modalities
                    .iter()
                    .any(|value| value.as_str() == Some("vision"))
            })
            .unwrap_or(false);
        let supports_tools = model
            .get("supports_tools")
            .or_else(|| model.get("supportsTools"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        result.push(AiModelInfo {
            id,
            name,
            context_window,
            max_output_tokens,
            supports_tools,
            supports_vision,
        });
    }

    Ok(result)
}

pub fn test_chat(endpoint: &AiEndpointConfig, model: &str) -> EngineResult<()> {
    validate_endpoint_access(endpoint)?;
    if model.trim().is_empty() {
        return Err(EngineError::Validation("model is required".to_string()));
    }
    let payload = json!({
        "model": model.trim(),
        "messages": [{"role": "user", "content": "Reply with OK."}],
        "max_tokens": 8,
        "stream": false,
    });
    with_auth(
        build_http_client_for_endpoint(&endpoint.url)?
            .post(chat_completions_url(&endpoint.url))
            .json(&payload),
        endpoint,
    )
    .send()?
    .error_for_status()?;
    Ok(())
}

pub fn run_one_shot(
    conn: &Connection,
    offline_mode: bool,
    request: &OneShotRequest,
) -> EngineResult<(OneShotResponse, TokenUsage)> {
    let settings = load_ai_settings(conn, offline_mode)?;
    let endpoint = active_endpoint(conn, offline_mode)?;
    let model = select_model(&settings, &endpoint, &request.operation)?;
    let user_content = build_one_shot_user_content(request)?;
    let schema = response_schema(&request.operation);
    let payload = json!({
        "model": model,
        "messages": [
            {
                "role": "system",
                "content": build_system_prompt(&settings.system_prompt, &request.operation),
            },
            {
                "role": "user",
                "content": user_content,
            }
        ],
        "response_format": {
            "type": "json_schema",
            "json_schema": {
                "name": schema.0,
                "schema": schema.1,
                "strict": true
            }
        }
    });
    let client = build_http_client_for_endpoint(&endpoint.url)?;
    let response = with_auth(
        client
            .post(chat_completions_url(&endpoint.url))
            .json(&payload),
        &endpoint,
    )
    .send()?
    .error_for_status()?;
    let body: Value = response.json()?;
    let content = body
        .get("choices")
        .and_then(Value::as_array)
        .and_then(|choices| choices.first())
        .and_then(|choice| choice.get("message"))
        .and_then(|message| message.get("content"))
        .and_then(Value::as_str)
        .ok_or_else(|| {
            EngineError::Parse("chat completions response missing message content".to_string())
        })?;
    let response = parse_one_shot_response(request, content)?;
    Ok((response, parse_token_usage(&body)))
}

fn parse_token_usage(body: &Value) -> TokenUsage {
    let usage = body.get("usage").unwrap_or(&Value::Null);
    TokenUsage {
        input_tokens: usage.get("prompt_tokens").and_then(Value::as_u64),
        output_tokens: usage.get("completion_tokens").and_then(Value::as_u64),
        cache_read_tokens: usage
            .get("prompt_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64),
        cache_write_tokens: usage
            .get("completion_tokens_details")
            .and_then(|details| details.get("cached_tokens"))
            .and_then(Value::as_u64),
    }
}

fn build_http_client_for_endpoint(endpoint_url: &str) -> EngineResult<Client> {
    let builder = Client::builder().timeout(Duration::from_secs(5));
    let builder = if should_bypass_proxy(endpoint_url) {
        builder.no_proxy()
    } else {
        builder
    };
    Ok(builder.build()?)
}

fn should_bypass_proxy(endpoint_url: &str) -> bool {
    let Ok(url) = reqwest::Url::parse(endpoint_url) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    host.eq_ignore_ascii_case("localhost")
        || host
            .parse::<IpAddr>()
            .map(|address| address.is_loopback())
            .unwrap_or(false)
}

fn load_endpoint(conn: &Connection, kind: AiEndpointKind) -> EngineResult<StoredAiEndpointConfig> {
    let url = get_optional_setting(conn, &endpoint_setting_key(kind, "url"))?.unwrap_or_default();
    let model =
        get_optional_setting(conn, &endpoint_setting_key(kind, "model"))?.unwrap_or_default();
    let api_key_configured =
        get_optional_setting(conn, &endpoint_setting_key(kind, "api_key_configured"))?
            .map(|value| value == "true")
            .unwrap_or(false);
    Ok(StoredAiEndpointConfig {
        kind,
        url,
        model,
        api_key_configured,
    })
}

fn load_mode_settings(
    conn: &Connection,
    kind: AiEndpointKind,
    legacy_chat_model: Option<&str>,
    legacy_title_model: Option<&str>,
    legacy_image_model: Option<&str>,
) -> EngineResult<AiModeSettings> {
    let mut endpoint = load_endpoint(conn, kind)?;
    if endpoint.model.trim().is_empty() {
        endpoint.model = legacy_chat_model.unwrap_or_default().to_string();
    }
    Ok(AiModeSettings {
        endpoint,
        title_model: get_optional_setting(conn, &endpoint_setting_key(kind, "title_model"))?
            .or_else(|| legacy_title_model.map(str::to_string)),
        image_model: get_optional_setting(conn, &endpoint_setting_key(kind, "image_model"))?
            .or_else(|| legacy_image_model.map(str::to_string)),
        chat_supports_tools: get_optional_bool_setting(
            conn,
            &endpoint_setting_key(kind, "chat_supports_tools"),
        )?,
        image_supports_vision: get_optional_bool_setting(
            conn,
            &endpoint_setting_key(kind, "image_supports_vision"),
        )?,
        models: load_endpoint_models(conn, kind)?,
    })
}

fn validate_endpoint_config(endpoint: &AiEndpointConfig) -> EngineResult<()> {
    validate_endpoint_access(endpoint)?;
    if endpoint.model.trim().is_empty() {
        return Err(EngineError::Validation(
            "endpoint model is required".to_string(),
        ));
    }
    Ok(())
}

fn validate_endpoint_access(endpoint: &AiEndpointConfig) -> EngineResult<()> {
    if endpoint.url.trim().is_empty() {
        return Err(EngineError::Validation(
            "endpoint url is required".to_string(),
        ));
    }
    if endpoint.kind == AiEndpointKind::Online
        && endpoint
            .api_key
            .as_ref()
            .map(|value| value.trim().is_empty())
            .unwrap_or(true)
    {
        return Err(EngineError::Validation(
            "online endpoint api key is required".to_string(),
        ));
    }
    Ok(())
}

fn select_model(
    settings: &AiSettings,
    endpoint: &AiEndpointConfig,
    operation: &OneShotOperation,
) -> EngineResult<String> {
    let active = settings.active();
    let selected = match operation {
        OneShotOperation::AnalyzeImage => {
            if active.image_supports_vision == Some(false) {
                return Err(EngineError::Validation(
                    "AI unavailable - selected image model is not configured for vision"
                        .to_string(),
                ));
            }
            active.image_model.as_ref()
        }
        OneShotOperation::AnalyzeTaxonomy
        | OneShotOperation::MapImportTaxonomy
        | OneShotOperation::AnalyzePost
        | OneShotOperation::DetectLanguage
        | OneShotOperation::TranslatePost { .. }
        | OneShotOperation::TranslateMedia { .. } => active.title_model.as_ref(),
    }
    .filter(|model| !model.trim().is_empty())
    .cloned()
    .unwrap_or_else(|| endpoint.model.clone());
    if selected.trim().is_empty() {
        return Err(EngineError::Validation(
            "AI unavailable - configure model in Settings".to_string(),
        ));
    }
    Ok(selected)
}

fn build_system_prompt(base_prompt: &str, operation: &OneShotOperation) -> String {
    let operation_prompt = match operation {
        OneShotOperation::AnalyzeTaxonomy => {
            "Return only JSON with tags and categories for the post."
        }
        OneShotOperation::MapImportTaxonomy => {
            "Map each imported category and tag to an existing equivalent when one exists. Return JSON objects keyed by each imported term; omit terms without a sound match."
        }
        OneShotOperation::AnalyzePost => {
            "Return only JSON with title, excerpt, and slug suggestions for the post."
        }
        OneShotOperation::DetectLanguage => "Return only JSON with the detected language_code.",
        OneShotOperation::TranslatePost { target_language } => {
            return format!(
                "{} Translate the post into {} and return only JSON with title, excerpt, and content.",
                base_prompt.trim(),
                target_language
            );
        }
        OneShotOperation::AnalyzeImage => {
            "Return only JSON with title, alt, and caption suggestions for the image."
        }
        OneShotOperation::TranslateMedia { target_language } => {
            return format!(
                "{} Translate the media metadata into {} and return only JSON with title, alt, and caption.",
                base_prompt.trim(),
                target_language
            );
        }
    };
    if base_prompt.trim().is_empty() {
        operation_prompt.to_string()
    } else {
        format!("{} {}", base_prompt.trim(), operation_prompt)
    }
}

fn build_one_shot_user_content(request: &OneShotRequest) -> EngineResult<Value> {
    match &request.operation {
        OneShotOperation::AnalyzeTaxonomy => Ok(format!(
            "Suggest tags and categories for this post: {}",
            serde_json::to_string(&request.content)?
        )
        .into()),
        OneShotOperation::MapImportTaxonomy => Ok(format!(
            "Map these imported taxonomy terms to existing project terms without inventing terms: {}",
            serde_json::to_string(&request.content)?
        )
        .into()),
        OneShotOperation::AnalyzePost => Ok(format!(
            "Analyze this post and suggest title, excerpt, and slug: {}",
            serde_json::to_string(&request.content)?
        )
        .into()),
        OneShotOperation::DetectLanguage => Ok(format!(
            "Detect the language of this text: {}",
            serde_json::to_string(&request.content)?
        )
        .into()),
        OneShotOperation::TranslatePost { target_language } => Ok(format!(
            "Translate this post to {}: {}",
            target_language,
            serde_json::to_string(&request.content)?
        )
        .into()),
        OneShotOperation::AnalyzeImage => build_image_analysis_user_content(&request.content),
        OneShotOperation::TranslateMedia { target_language } => Ok(format!(
            "Translate this media metadata to {}: {}",
            target_language,
            serde_json::to_string(&request.content)?
        )
        .into()),
    }
}

fn build_image_analysis_user_content(content: &Value) -> EngineResult<Value> {
    let image_data_url = content
        .get("image_data_url")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| EngineError::Validation("image analysis requires image data".to_string()))?;

    let mut metadata = content.clone();
    if let Some(object) = metadata.as_object_mut() {
        object.remove("image_data_url");
    }

    Ok(json!([
        {
            "type": "text",
            "text": format!(
                "Analyze this image and return title, alt, and caption suggestions. Metadata: {}",
                serde_json::to_string(&metadata)?
            )
        },
        {
            "type": "image_url",
            "image_url": {
                "url": image_data_url
            }
        }
    ]))
}

fn response_schema(operation: &OneShotOperation) -> (&'static str, Value) {
    match operation {
        OneShotOperation::AnalyzeTaxonomy => (
            "taxonomy_suggestion",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "tags": { "type": "array", "items": { "type": "string" } },
                    "categories": { "type": "array", "items": { "type": "string" } }
                },
                "required": ["tags", "categories"]
            }),
        ),
        OneShotOperation::MapImportTaxonomy => (
            "import_taxonomy_mapping",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "category_mappings": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    },
                    "tag_mappings": {
                        "type": "object",
                        "additionalProperties": { "type": "string" }
                    }
                },
                "required": ["category_mappings", "tag_mappings"]
            }),
        ),
        OneShotOperation::AnalyzePost => (
            "post_analysis",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "title": { "type": "string" },
                    "excerpt": { "type": "string" },
                    "slug": { "type": "string" }
                },
                "required": ["title", "excerpt", "slug"]
            }),
        ),
        OneShotOperation::DetectLanguage => (
            "language_detection",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "language_code": { "type": "string" }
                },
                "required": ["language_code"]
            }),
        ),
        OneShotOperation::TranslatePost { .. } => (
            "post_translation",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "title": { "type": "string" },
                    "excerpt": { "type": "string" },
                    "content": { "type": "string" }
                },
                "required": ["title", "excerpt", "content"]
            }),
        ),
        OneShotOperation::AnalyzeImage => (
            "image_analysis",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "title": { "type": "string" },
                    "alt": { "type": "string" },
                    "caption": { "type": "string" }
                },
                "required": ["title", "alt", "caption"]
            }),
        ),
        OneShotOperation::TranslateMedia { .. } => (
            "media_translation",
            json!({
                "type": "object",
                "additionalProperties": false,
                "properties": {
                    "title": { "type": "string" },
                    "alt": { "type": "string" },
                    "caption": { "type": "string" }
                },
                "required": ["title", "alt", "caption"]
            }),
        ),
    }
}

fn parse_one_shot_response(
    request: &OneShotRequest,
    content: &str,
) -> EngineResult<OneShotResponse> {
    Ok(match request.operation {
        OneShotOperation::AnalyzeTaxonomy => {
            OneShotResponse::Taxonomy(serde_json::from_str(content)?)
        }
        OneShotOperation::MapImportTaxonomy => {
            OneShotResponse::ImportTaxonomyMapping(serde_json::from_str(content)?)
        }
        OneShotOperation::AnalyzePost => {
            OneShotResponse::PostAnalysis(serde_json::from_str(content)?)
        }
        OneShotOperation::DetectLanguage => {
            OneShotResponse::LanguageDetection(serde_json::from_str(content)?)
        }
        OneShotOperation::TranslatePost { .. } => {
            OneShotResponse::Translation(serde_json::from_str(content)?)
        }
        OneShotOperation::AnalyzeImage => {
            OneShotResponse::ImageAnalysis(serde_json::from_str(content)?)
        }
        OneShotOperation::TranslateMedia { .. } => {
            OneShotResponse::MediaTranslation(serde_json::from_str(content)?)
        }
    })
}

fn endpoint_setting_key(kind: AiEndpointKind, suffix: &str) -> String {
    format!("{}.{}", kind.settings_prefix(), suffix)
}

fn endpoint_keyring_entry(kind: AiEndpointKind) -> EngineResult<Entry> {
    Entry::new(
        KEYRING_SERVICE,
        &format!("{}.{}", KEYRING_SETTING_PREFIX, kind.as_str()),
    )
    .map_err(keyring_error)
}

fn store_endpoint_api_key(kind: AiEndpointKind, api_key: &str) -> EngineResult<()> {
    if cargo_test_process() {
        TEST_API_KEYS
            .lock()
            .map_err(|error| EngineError::Validation(error.to_string()))?
            .insert(test_api_key_name(kind), api_key.to_string());
        return Ok(());
    }
    endpoint_keyring_entry(kind)?
        .set_password(api_key)
        .map_err(keyring_error)
}

fn read_endpoint_api_key(kind: AiEndpointKind) -> EngineResult<Option<String>> {
    if cargo_test_process() {
        return TEST_API_KEYS
            .lock()
            .map_err(|error| EngineError::Validation(error.to_string()))
            .map(|keys| {
                keys.get(&test_api_key_name(kind))
                    .cloned()
                    .filter(|key| !key.trim().is_empty())
            });
    }
    match endpoint_keyring_entry(kind)?.get_password() {
        Ok(password) if password.trim().is_empty() => Ok(None),
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(keyring_error(error)),
    }
}

fn delete_endpoint_api_key(kind: AiEndpointKind) -> EngineResult<()> {
    if cargo_test_process() {
        TEST_API_KEYS
            .lock()
            .map_err(|error| EngineError::Validation(error.to_string()))?
            .remove(&test_api_key_name(kind));
        return Ok(());
    }
    match endpoint_keyring_entry(kind)?.delete_credential() {
        Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
        Err(error) => Err(keyring_error(error)),
    }
}

/// Cargo places every unit and integration test executable in a `deps`
/// directory. Keep those processes on a process-local credential store so a
/// test can never open an operating-system password prompt.
fn cargo_test_process() -> bool {
    cfg!(test)
        || std::env::current_exe().is_ok_and(|path| {
            path.parent()
                .and_then(|parent| parent.file_name())
                .is_some_and(|name| name == "deps")
        })
}

fn test_api_key_name(kind: AiEndpointKind) -> String {
    format!("{:?}:{}", std::thread::current().id(), kind.as_str())
}

fn keyring_error(error: keyring::Error) -> EngineError {
    EngineError::Validation(error.to_string())
}

fn set_setting(conn: &Connection, key: &str, value: &str, updated_at: i64) -> EngineResult<()> {
    crate::engine::settings::set_at(conn, key, value, updated_at)
}

fn set_optional_setting(
    conn: &Connection,
    key: &str,
    value: Option<&str>,
    updated_at: i64,
) -> EngineResult<()> {
    set_setting(conn, key, value.unwrap_or(""), updated_at)
}

fn set_optional_bool_setting(
    conn: &Connection,
    key: &str,
    value: Option<bool>,
    updated_at: i64,
) -> EngineResult<()> {
    set_setting(
        conn,
        key,
        value
            .map(|value| if value { "true" } else { "false" })
            .unwrap_or(""),
        updated_at,
    )
}

fn get_optional_setting(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    match setting::get_setting_by_key(conn, key) {
        Ok(setting) if setting.value.trim().is_empty() => Ok(None),
        Ok(setting) => Ok(Some(setting.value)),
        Err(diesel::result::Error::NotFound) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
}

fn get_optional_bool_setting(conn: &Connection, key: &str) -> EngineResult<Option<bool>> {
    Ok(get_optional_setting(conn, key)?.map(|value| value == "true"))
}

fn models_url(base_url: &str) -> String {
    join_openai_path(base_url, "models")
}

fn chat_completions_url(base_url: &str) -> String {
    join_openai_path(base_url, "chat/completions")
}

fn join_openai_path(base_url: &str, suffix: &str) -> String {
    let trimmed = base_url.trim_end_matches('/');
    if trimmed.ends_with("/v1") {
        format!("{trimmed}/{suffix}")
    } else {
        format!("{trimmed}/v1/{suffix}")
    }
}

fn with_auth(
    request: reqwest::blocking::RequestBuilder,
    endpoint: &AiEndpointConfig,
) -> reqwest::blocking::RequestBuilder {
    if let Some(api_key) = &endpoint.api_key {
        request.bearer_auth(api_key)
    } else {
        request
    }
}

#[cfg(test)]
mod tests {
    use std::io::{Read, Write};
    use std::net::TcpListener;
    use std::thread;

    use super::*;
    use crate::db::Database;

    fn setup() -> Database {
        let db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        db
    }

    fn clear_keyring(kind: AiEndpointKind) {
        delete_endpoint_api_key(kind).unwrap();
    }

    #[test]
    fn loads_empty_defaults() {
        let db = setup();
        let settings = load_ai_settings(db.conn(), false).unwrap();
        assert!(!settings.offline_mode);
        assert!(settings.online.endpoint.url.is_empty());
        assert!(settings.airplane.endpoint.url.is_empty());
        assert!(settings.online.title_model.is_none());
    }

    #[test]
    fn saves_online_endpoint_with_keychain_secret() {
        clear_keyring(AiEndpointKind::Online);
        let db = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Online,
                url: "https://example.test/v1".to_string(),
                model: "gpt-4.1-mini".to_string(),
                api_key: Some("secret-token".to_string()),
            },
        )
        .unwrap();

        let active = active_endpoint(db.conn(), false).unwrap();
        assert_eq!(active.url, "https://example.test/v1");
        assert_eq!(active.model, "gpt-4.1-mini");
        assert_eq!(active.api_key.as_deref(), Some("secret-token"));
    }

    #[test]
    fn airplane_endpoint_does_not_require_api_key() {
        let db = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: "http://localhost:11434/v1".to_string(),
                model: "llama3.2".to_string(),
                api_key: None,
            },
        )
        .unwrap();

        let active = active_endpoint(db.conn(), true).unwrap();
        assert_eq!(active.kind, AiEndpointKind::Airplane);
        assert!(active.api_key.is_none());
    }

    #[test]
    fn refresh_model_catalog_parses_openai_models_shape() {
        let server = spawn_test_server(
            |request| {
                assert!(request.starts_with("GET /v1/models HTTP/1.1"));
                assert!(
                    request.contains("authorization: Bearer dummy")
                        || request.contains("Authorization: Bearer dummy")
                );
                http_ok(
                    r#"{"data":[{"id":"gpt-4.1-mini","name":"GPT 4.1 mini","context_window":128000,"max_output_tokens":8192,"supports_tools":true,"modalities":["text"]},{"id":"gpt-4.1","modalities":["text","vision"]}]}"#,
                )
            },
            1,
        );
        let models = refresh_model_catalog(&AiEndpointConfig {
            kind: AiEndpointKind::Online,
            url: server,
            model: String::new(),
            api_key: Some("dummy".to_string()),
        })
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "GPT 4.1 mini");
        assert!(models[0].supports_tools);
        assert!(models[1].supports_vision);
    }

    #[test]
    fn endpoint_models_persist_per_endpoint_and_overwrite_on_refresh() {
        let db = setup();
        let online = vec![AiModelInfo {
            id: "gpt-4.1".to_string(),
            name: "GPT 4.1".to_string(),
            context_window: Some(128_000),
            max_output_tokens: Some(8_192),
            supports_tools: true,
            supports_vision: true,
        }];
        let airplane = vec![AiModelInfo {
            id: "llama3.2".to_string(),
            name: "Llama 3.2".to_string(),
            context_window: None,
            max_output_tokens: None,
            supports_tools: false,
            supports_vision: false,
        }];
        save_endpoint_models(db.conn(), AiEndpointKind::Online, &online).unwrap();
        save_endpoint_models(db.conn(), AiEndpointKind::Airplane, &airplane).unwrap();

        let settings = load_ai_settings(db.conn(), false).unwrap();
        assert_eq!(settings.online.models, online);
        assert_eq!(settings.airplane.models, airplane);

        let refreshed = vec![AiModelInfo {
            id: "gpt-5".to_string(),
            name: "GPT 5".to_string(),
            context_window: Some(256_000),
            max_output_tokens: Some(16_384),
            supports_tools: true,
            supports_vision: false,
        }];
        save_endpoint_models(db.conn(), AiEndpointKind::Online, &refreshed).unwrap();

        let settings = load_ai_settings(db.conn(), false).unwrap();
        assert_eq!(settings.online.models, refreshed);
        assert_eq!(settings.airplane.models, airplane);
    }

    #[test]
    fn model_preferences_are_independent_per_endpoint() {
        let db = setup();
        save_model_preferences(
            db.conn(),
            AiEndpointKind::Online,
            Some("online-title"),
            Some("online-image"),
            Some(true),
            Some(true),
        )
        .unwrap();
        save_model_preferences(
            db.conn(),
            AiEndpointKind::Airplane,
            Some("local-title"),
            Some("local-image"),
            Some(false),
            Some(true),
        )
        .unwrap();

        let settings = load_ai_settings(db.conn(), false).unwrap();
        assert_eq!(settings.online.title_model.as_deref(), Some("online-title"));
        assert_eq!(
            settings.airplane.title_model.as_deref(),
            Some("local-title")
        );
        assert_eq!(settings.online.chat_supports_tools, Some(true));
        assert_eq!(settings.airplane.chat_supports_tools, Some(false));
        assert_eq!(settings.online.image_supports_vision, Some(true));
        assert_eq!(settings.airplane.image_supports_vision, Some(true));
    }

    #[test]
    fn test_chat_sends_the_selected_model() {
        let server = spawn_test_server(
            |request| {
                assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
                assert!(request.contains(r#""model":"qwen-9b""#));
                assert!(
                    request.contains("authorization: Bearer dummy")
                        || request.contains("Authorization: Bearer dummy")
                );
                http_ok(r#"{"choices":[{"message":{"content":"OK"}}]}"#)
            },
            1,
        );

        test_chat(
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: server,
                model: String::new(),
                api_key: Some("dummy".to_string()),
            },
            "qwen-9b",
        )
        .unwrap();
    }

    #[test]
    fn explicit_vision_override_blocks_image_requests() {
        let db = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: "http://localhost:11434/v1".to_string(),
                model: "qwen".to_string(),
                api_key: None,
            },
        )
        .unwrap();
        save_model_preferences(
            db.conn(),
            AiEndpointKind::Airplane,
            None,
            Some("qwen-vl"),
            Some(false),
            Some(false),
        )
        .unwrap();

        let error = run_one_shot(
            db.conn(),
            true,
            &OneShotRequest {
                operation: OneShotOperation::AnalyzeImage,
                content: json!({"image_data_url": "data:image/jpeg;base64,abc123"}),
            },
        )
        .unwrap_err();
        assert!(error.to_string().contains("not configured for vision"));
    }

    #[test]
    fn run_one_shot_uses_active_endpoint_and_parses_response() {
        clear_keyring(AiEndpointKind::Online);
        let server = spawn_test_server(
            |request| {
                if request.starts_with("GET /v1/models HTTP/1.1") {
                    return http_ok(r#"{"data":[{"id":"gpt-4.1-mini"}]}"#);
                }
                assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
                assert!(
                    request.contains("authorization: Bearer secret-token")
                        || request.contains("Authorization: Bearer secret-token")
                );
                http_ok(
                    r#"{"choices":[{"message":{"content":"{\"title\":\"Better title\",\"excerpt\":\"Short summary\",\"slug\":\"better-title\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
                )
            },
            1,
        );

        let db = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Online,
                url: server,
                model: "gpt-4.1-mini".to_string(),
                api_key: Some("secret-token".to_string()),
            },
        )
        .unwrap();
        save_model_preferences(
            db.conn(),
            AiEndpointKind::Online,
            Some("gpt-4.1-mini"),
            None,
            None,
            None,
        )
        .unwrap();

        let (response, usage) = run_one_shot(
            db.conn(),
            false,
            &OneShotRequest {
                operation: OneShotOperation::AnalyzePost,
                content: json!({"title":"Draft title","excerpt":"","content":"Body"}),
            },
        )
        .unwrap();

        assert_eq!(usage.input_tokens, Some(10));
        assert_eq!(usage.output_tokens, Some(5));
        assert_eq!(
            response,
            OneShotResponse::PostAnalysis(PostAnalysisResult {
                title: "Better title".to_string(),
                excerpt: "Short summary".to_string(),
                slug: "better-title".to_string(),
            })
        );
    }

    #[test]
    fn image_analysis_user_content_is_multimodal() {
        let content = build_image_analysis_user_content(&json!({
            "title": "Existing title",
            "alt": "Existing alt",
            "image_data_url": "data:image/jpeg;base64,abc123"
        }))
        .unwrap();

        let parts = content.as_array().unwrap();
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0]["type"], "text");
        assert!(
            parts[0]["text"]
                .as_str()
                .unwrap()
                .contains("Existing title")
        );
        assert_eq!(parts[1]["type"], "image_url");
        assert_eq!(
            parts[1]["image_url"]["url"],
            "data:image/jpeg;base64,abc123"
        );
    }

    #[test]
    fn run_one_shot_supports_taxonomy_analysis_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::AnalyzeTaxonomy,
                content: json!({
                    "title": "Rust preview parity",
                    "excerpt": "Closing M4",
                    "content": "Rendering routes and previews",
                    "tags": ["rendering"],
                    "categories": ["engineering"]
                }),
            },
            r#"{"choices":[{"message":{"content":"{\"tags\":[\"rust\",\"preview\"],\"categories\":[\"engineering\"]}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::Taxonomy(TaxonomySuggestion {
                tags: vec!["rust".to_string(), "preview".to_string()],
                categories: vec!["engineering".to_string()],
            })
        );
    }

    #[test]
    fn run_one_shot_supports_import_taxonomy_mapping_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::MapImportTaxonomy,
                content: json!({
                    "imported_categories": ["Old Engineering"],
                    "imported_tags": ["Old Rust"],
                    "existing_categories": ["Engineering"],
                    "existing_tags": ["Rust"]
                }),
            },
            r#"{"choices":[{"message":{"content":"{\"category_mappings\":{\"Old Engineering\":\"Engineering\"},\"tag_mappings\":{\"Old Rust\":\"Rust\"}}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::ImportTaxonomyMapping(ImportTaxonomyMapping {
                category_mappings: BTreeMap::from([(
                    "Old Engineering".to_string(),
                    "Engineering".to_string(),
                )]),
                tag_mappings: BTreeMap::from([("Old Rust".to_string(), "Rust".to_string(),)]),
            })
        );
    }

    #[test]
    fn run_one_shot_supports_post_analysis_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::AnalyzePost,
                content: json!({"title":"Draft title","excerpt":"","content":"Body"}),
            },
            r#"{"choices":[{"message":{"content":"{\"title\":\"Better title\",\"excerpt\":\"Short summary\",\"slug\":\"better-title\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::PostAnalysis(PostAnalysisResult {
                title: "Better title".to_string(),
                excerpt: "Short summary".to_string(),
                slug: "better-title".to_string(),
            })
        );
    }

    #[test]
    fn run_one_shot_supports_language_detection_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::DetectLanguage,
                content: json!({"text": "Bonjour tout le monde"}),
            },
            r#"{"choices":[{"message":{"content":"{\"language_code\":\"fr\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::LanguageDetection(LanguageDetectionResult {
                language_code: "fr".to_string(),
            })
        );
    }

    #[test]
    fn run_one_shot_supports_post_translation_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::TranslatePost {
                    target_language: "de".to_string(),
                },
                content: json!({
                    "title": "Hello",
                    "excerpt": "Short summary",
                    "content": "Body"
                }),
            },
            r#"{"choices":[{"message":{"content":"{\"title\":\"Hallo\",\"excerpt\":\"Kurzfassung\",\"content\":\"Inhalt\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::Translation(TranslationResult {
                title: "Hallo".to_string(),
                excerpt: "Kurzfassung".to_string(),
                content: "Inhalt".to_string(),
            })
        );
    }

    #[test]
    fn run_one_shot_supports_image_analysis_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::AnalyzeImage,
                content: json!({
                    "title": "Existing title",
                    "alt": "",
                    "caption": "",
                    "filename": "hero.jpg",
                    "mime_type": "image/jpeg",
                    "image_data_url": "data:image/jpeg;base64,abc123"
                }),
            },
            r#"{"choices":[{"message":{"content":"{\"title\":\"Hero image\",\"alt\":\"A scenic mountain\",\"caption\":\"Sunrise over the ridge\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::ImageAnalysis(ImageAnalysisResult {
                title: "Hero image".to_string(),
                alt: "A scenic mountain".to_string(),
                caption: "Sunrise over the ridge".to_string(),
            })
        );
    }

    #[test]
    fn run_one_shot_supports_media_translation_via_airplane_endpoint() {
        let response = run_airplane_one_shot(
            OneShotRequest {
                operation: OneShotOperation::TranslateMedia {
                    target_language: "it".to_string(),
                },
                content: json!({
                    "title": "Mountain",
                    "alt": "Snowy ridge",
                    "caption": "Morning light"
                }),
            },
            r#"{"choices":[{"message":{"content":"{\"title\":\"Montagna\",\"alt\":\"Cresta innevata\",\"caption\":\"Luce del mattino\"}"}}],"usage":{"prompt_tokens":10,"completion_tokens":5}}"#,
        );

        assert_eq!(
            response,
            OneShotResponse::MediaTranslation(MediaTranslationResult {
                title: "Montagna".to_string(),
                alt: "Cresta innevata".to_string(),
                caption: "Luce del mattino".to_string(),
            })
        );
    }

    fn run_airplane_one_shot(request: OneShotRequest, body: &'static str) -> OneShotResponse {
        let server = spawn_test_server(
            move |incoming| {
                assert!(incoming.starts_with("POST /v1/chat/completions HTTP/1.1"));
                http_ok(body)
            },
            1,
        );

        let db = setup();
        save_endpoint(
            db.conn(),
            &AiEndpointConfig {
                kind: AiEndpointKind::Airplane,
                url: server,
                model: "llama3.2".to_string(),
                api_key: None,
            },
        )
        .unwrap();

        let (response, usage) = run_one_shot(db.conn(), true, &request).unwrap();
        assert_eq!(
            usage,
            TokenUsage {
                input_tokens: Some(10),
                output_tokens: Some(5),
                cache_read_tokens: None,
                cache_write_tokens: None,
            }
        );
        response
    }

    #[test]
    fn parse_token_usage_normalizes_complete_usage() {
        let usage = parse_token_usage(&json!({
            "usage": {
                "prompt_tokens": 100,
                "completion_tokens": 42,
                "prompt_tokens_details": { "cached_tokens": 80 },
                "completion_tokens_details": { "cached_tokens": 7 }
            }
        }));
        assert_eq!(
            usage,
            TokenUsage {
                input_tokens: Some(100),
                output_tokens: Some(42),
                cache_read_tokens: Some(80),
                cache_write_tokens: Some(7),
            }
        );
    }

    #[test]
    fn parse_token_usage_leaves_missing_counters_null() {
        let usage = parse_token_usage(&json!({
            "usage": { "prompt_tokens": 100 }
        }));
        assert_eq!(
            usage,
            TokenUsage {
                input_tokens: Some(100),
                output_tokens: None,
                cache_read_tokens: None,
                cache_write_tokens: None,
            }
        );
    }

    #[test]
    fn parse_token_usage_handles_absent_usage_object() {
        assert_eq!(parse_token_usage(&json!({})), TokenUsage::default());
    }

    #[test]
    fn parse_token_usage_ignores_non_numeric_counters() {
        let usage = parse_token_usage(&json!({
            "usage": {
                "prompt_tokens": "many",
                "completion_tokens": -3,
                "prompt_tokens_details": { "cached_tokens": null },
                "completion_tokens_details": "nope"
            }
        }));
        assert_eq!(usage, TokenUsage::default());
    }

    fn spawn_test_server(
        handler: impl Fn(String) -> String + Send + 'static,
        request_count: usize,
    ) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().take(request_count) {
                let mut stream = stream.unwrap();
                let mut buffer = [0_u8; 8192];
                let size = stream.read(&mut buffer).unwrap();
                let request = String::from_utf8_lossy(&buffer[..size]).to_string();
                let response = handler(request);
                stream.write_all(response.as_bytes()).unwrap();
            }
        });
        format!("http://{}", addr)
    }

    fn http_ok(body: &str) -> String {
        format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        )
    }
}
