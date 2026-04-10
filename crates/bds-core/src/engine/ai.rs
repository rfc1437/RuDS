use std::time::Duration;

use keyring::Entry;
use reqwest::blocking::Client;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::db::queries::setting;
use crate::engine::{EngineError, EngineResult};
use crate::util::now_unix_ms;

const KEYRING_SERVICE: &str = "RuDS";
const KEYRING_SETTING_PREFIX: &str = "ai.endpoint";

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
    pub default_model: Option<String>,
    pub title_model: Option<String>,
    pub image_model: Option<String>,
    pub system_prompt: String,
    pub online_endpoint: StoredAiEndpointConfig,
    pub airplane_endpoint: StoredAiEndpointConfig,
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
    pub supports_vision: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneShotOperation {
    AnalyzeTaxonomy,
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OneShotResponse {
    Taxonomy(TaxonomySuggestion),
    PostAnalysis(PostAnalysisResult),
    LanguageDetection(LanguageDetectionResult),
    Translation(TranslationResult),
    ImageAnalysis(ImageAnalysisResult),
    MediaTranslation(MediaTranslationResult),
}

pub fn load_ai_settings(conn: &Connection, offline_mode: bool) -> EngineResult<AiSettings> {
    let online_endpoint = load_endpoint(conn, AiEndpointKind::Online)?;
    let airplane_endpoint = load_endpoint(conn, AiEndpointKind::Airplane)?;
    let default_model = get_optional_setting(conn, "ai.default_model")?;
    let title_model = get_optional_setting(conn, "ai.title_model")?;
    let image_model = get_optional_setting(conn, "ai.image_model")?;
    let system_prompt = get_optional_setting(conn, "ai.system_prompt")?.unwrap_or_default();

    Ok(AiSettings {
        offline_mode,
        default_model,
        title_model,
        image_model,
        system_prompt,
        online_endpoint,
        airplane_endpoint,
    })
}

pub fn save_endpoint(conn: &Connection, endpoint: &AiEndpointConfig) -> EngineResult<()> {
    validate_endpoint_config(endpoint)?;
    let checked_at = now_unix_ms();
    set_setting(conn, &endpoint_setting_key(endpoint.kind, "url"), endpoint.url.trim(), checked_at)?;
    set_setting(conn, &endpoint_setting_key(endpoint.kind, "model"), endpoint.model.trim(), checked_at)?;
    if endpoint.kind == AiEndpointKind::Online {
        let entry = endpoint_keyring_entry(endpoint.kind)?;
        if let Some(api_key) = &endpoint.api_key {
            if api_key.trim().is_empty() {
                entry.delete_credential().ok();
                set_setting(conn, &endpoint_setting_key(endpoint.kind, "api_key_configured"), "false", checked_at)?;
            } else {
                entry.set_password(api_key.trim()).map_err(keyring_error)?;
                set_setting(conn, &endpoint_setting_key(endpoint.kind, "api_key_configured"), "true", checked_at)?;
            }
        }
    }
    Ok(())
}

pub fn save_model_preferences(
    conn: &Connection,
    default_model: Option<&str>,
    title_model: Option<&str>,
    image_model: Option<&str>,
    system_prompt: &str,
) -> EngineResult<()> {
    let updated_at = now_unix_ms();
    set_optional_setting(conn, "ai.default_model", default_model, updated_at)?;
    set_optional_setting(conn, "ai.title_model", title_model, updated_at)?;
    set_optional_setting(conn, "ai.image_model", image_model, updated_at)?;
    set_setting(conn, "ai.system_prompt", system_prompt, updated_at)?;
    Ok(())
}

pub fn active_endpoint(conn: &Connection, offline_mode: bool) -> EngineResult<AiEndpointConfig> {
    let kind = if offline_mode { AiEndpointKind::Airplane } else { AiEndpointKind::Online };
    let stored = load_endpoint(conn, kind)?;
    if stored.url.trim().is_empty() {
        return Err(EngineError::Validation(format!(
            "AI unavailable - configure {} endpoint in Settings",
            kind.as_str()
        )));
    }
    let api_key = if kind == AiEndpointKind::Online {
        let entry = endpoint_keyring_entry(kind)?;
        let password = entry.get_password().map_err(keyring_error)?;
        if password.trim().is_empty() {
            return Err(EngineError::Validation(
                "AI unavailable - configure online endpoint in Settings".to_string(),
            ));
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
    let entry = endpoint_keyring_entry(kind)?;
    match entry.get_password() {
        Ok(password) if password.trim().is_empty() => Ok(None),
        Ok(password) => Ok(Some(password)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(error) => Err(keyring_error(error)),
    }
}

pub fn refresh_model_catalog(endpoint: &AiEndpointConfig) -> EngineResult<Vec<AiModelInfo>> {
    validate_endpoint_config(endpoint)?;
    let client = build_http_client()?;
    let request = client.get(models_url(&endpoint.url));
    let response = with_auth(request, endpoint)
        .send()?
        .error_for_status()?;
    let body: Value = response.json()?;
    let models = body
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| EngineError::Parse("model catalog response missing data array".to_string()))?;

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
            .map(|modalities| modalities.iter().any(|value| value.as_str() == Some("vision")))
            .unwrap_or(false);
        result.push(AiModelInfo {
            id,
            name,
            context_window,
            max_output_tokens,
            supports_vision,
        });
    }

    Ok(result)
}

pub fn test_endpoint(endpoint: &AiEndpointConfig) -> EngineResult<()> {
    let _ = refresh_model_catalog(endpoint)?;
    Ok(())
}

pub fn run_one_shot(
    conn: &Connection,
    offline_mode: bool,
    request: &OneShotRequest,
) -> EngineResult<OneShotResponse> {
    let settings = load_ai_settings(conn, offline_mode)?;
    let endpoint = active_endpoint(conn, offline_mode)?;
    let model = select_model(&settings, &endpoint, &request.operation)?;
    let prompt = build_one_shot_prompt(request)?;
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
                "content": prompt,
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
    let client = build_http_client()?;
    let response = with_auth(client.post(chat_completions_url(&endpoint.url)).json(&payload), &endpoint)
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
        .ok_or_else(|| EngineError::Parse("chat completions response missing message content".to_string()))?;
    parse_one_shot_response(request, content)
}

fn build_http_client() -> EngineResult<Client> {
    Ok(Client::builder().timeout(Duration::from_secs(5)).build()?)
}

fn load_endpoint(conn: &Connection, kind: AiEndpointKind) -> EngineResult<StoredAiEndpointConfig> {
    let url = get_optional_setting(conn, &endpoint_setting_key(kind, "url"))?.unwrap_or_default();
    let model = get_optional_setting(conn, &endpoint_setting_key(kind, "model"))?.unwrap_or_default();
    let api_key_configured = get_optional_setting(conn, &endpoint_setting_key(kind, "api_key_configured"))?
        .map(|value| value == "true")
        .unwrap_or(false);
    Ok(StoredAiEndpointConfig {
        kind,
        url,
        model,
        api_key_configured,
    })
}

fn validate_endpoint_config(endpoint: &AiEndpointConfig) -> EngineResult<()> {
    if endpoint.url.trim().is_empty() {
        return Err(EngineError::Validation("endpoint url is required".to_string()));
    }
    if endpoint.model.trim().is_empty() {
        return Err(EngineError::Validation("endpoint model is required".to_string()));
    }
    if endpoint.kind == AiEndpointKind::Online
        && endpoint.api_key.as_ref().map(|value| value.trim().is_empty()).unwrap_or(true)
    {
        return Err(EngineError::Validation("online endpoint api key is required".to_string()));
    }
    Ok(())
}

fn select_model(settings: &AiSettings, endpoint: &AiEndpointConfig, operation: &OneShotOperation) -> EngineResult<String> {
    let selected = match operation {
        OneShotOperation::AnalyzeImage => settings.image_model.as_ref(),
        OneShotOperation::AnalyzeTaxonomy
        | OneShotOperation::AnalyzePost
        | OneShotOperation::DetectLanguage
        | OneShotOperation::TranslatePost { .. }
        | OneShotOperation::TranslateMedia { .. } => settings.title_model.as_ref().or(settings.default_model.as_ref()),
    }
    .filter(|model| !model.trim().is_empty())
    .cloned()
    .unwrap_or_else(|| endpoint.model.clone());
    if selected.trim().is_empty() {
        return Err(EngineError::Validation("AI unavailable - configure model in Settings".to_string()));
    }
    Ok(selected)
}

fn build_system_prompt(base_prompt: &str, operation: &OneShotOperation) -> String {
    let operation_prompt = match operation {
        OneShotOperation::AnalyzeTaxonomy => "Return only JSON with tags and categories for the post.",
        OneShotOperation::AnalyzePost => "Return only JSON with title, excerpt, and slug suggestions for the post.",
        OneShotOperation::DetectLanguage => "Return only JSON with the detected language_code.",
        OneShotOperation::TranslatePost { target_language } => {
            return format!("{} Translate the post into {} and return only JSON with title, excerpt, and content.", base_prompt.trim(), target_language);
        }
        OneShotOperation::AnalyzeImage => "Return only JSON with title, alt, and caption suggestions for the image.",
        OneShotOperation::TranslateMedia { target_language } => {
            return format!("{} Translate the media metadata into {} and return only JSON with title, alt, and caption.", base_prompt.trim(), target_language);
        }
    };
    if base_prompt.trim().is_empty() {
        operation_prompt.to_string()
    } else {
        format!("{} {}", base_prompt.trim(), operation_prompt)
    }
}

fn build_one_shot_prompt(request: &OneShotRequest) -> EngineResult<String> {
    match &request.operation {
        OneShotOperation::AnalyzeTaxonomy => Ok(format!(
            "Suggest tags and categories for this post: {}",
            serde_json::to_string(&request.content)?
        )),
        OneShotOperation::AnalyzePost => Ok(format!(
            "Analyze this post and suggest title, excerpt, and slug: {}",
            serde_json::to_string(&request.content)?
        )),
        OneShotOperation::DetectLanguage => Ok(format!(
            "Detect the language of this text: {}",
            serde_json::to_string(&request.content)?
        )),
        OneShotOperation::TranslatePost { target_language } => Ok(format!(
            "Translate this post to {}: {}",
            target_language,
            serde_json::to_string(&request.content)?
        )),
        OneShotOperation::AnalyzeImage => Ok(format!(
            "Analyze this image metadata and return title, alt, and caption suggestions: {}",
            serde_json::to_string(&request.content)?
        )),
        OneShotOperation::TranslateMedia { target_language } => Ok(format!(
            "Translate this media metadata to {}: {}",
            target_language,
            serde_json::to_string(&request.content)?
        )),
    }
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

fn parse_one_shot_response(request: &OneShotRequest, content: &str) -> EngineResult<OneShotResponse> {
    Ok(match request.operation {
        OneShotOperation::AnalyzeTaxonomy => OneShotResponse::Taxonomy(serde_json::from_str(content)?),
        OneShotOperation::AnalyzePost => OneShotResponse::PostAnalysis(serde_json::from_str(content)?),
        OneShotOperation::DetectLanguage => OneShotResponse::LanguageDetection(serde_json::from_str(content)?),
        OneShotOperation::TranslatePost { .. } => OneShotResponse::Translation(serde_json::from_str(content)?),
        OneShotOperation::AnalyzeImage => OneShotResponse::ImageAnalysis(serde_json::from_str(content)?),
        OneShotOperation::TranslateMedia { .. } => OneShotResponse::MediaTranslation(serde_json::from_str(content)?),
    })
}

fn endpoint_setting_key(kind: AiEndpointKind, suffix: &str) -> String {
    format!("{}.{}", kind.settings_prefix(), suffix)
}

fn endpoint_keyring_entry(kind: AiEndpointKind) -> EngineResult<Entry> {
    Entry::new(KEYRING_SERVICE, &format!("{}.{}", KEYRING_SETTING_PREFIX, kind.as_str())).map_err(keyring_error)
}

fn keyring_error(error: keyring::Error) -> EngineError {
    EngineError::Validation(error.to_string())
}

fn set_setting(conn: &Connection, key: &str, value: &str, updated_at: i64) -> EngineResult<()> {
    setting::set_setting_value(conn, key, value, updated_at)?;
    Ok(())
}

fn set_optional_setting(conn: &Connection, key: &str, value: Option<&str>, updated_at: i64) -> EngineResult<()> {
    set_setting(conn, key, value.unwrap_or(""), updated_at)
}

fn get_optional_setting(conn: &Connection, key: &str) -> EngineResult<Option<String>> {
    match setting::get_setting_by_key(conn, key) {
        Ok(setting) if setting.value.trim().is_empty() => Ok(None),
        Ok(setting) => Ok(Some(setting.value)),
        Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
        Err(error) => Err(EngineError::Db(error)),
    }
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
        let mut db = Database::open_in_memory().unwrap();
        db.migrate().unwrap();
        db
    }

    fn clear_keyring(kind: AiEndpointKind) {
        let entry = endpoint_keyring_entry(kind).unwrap();
        entry.delete_credential().ok();
    }

    #[test]
    fn loads_empty_defaults() {
        let db = setup();
        let settings = load_ai_settings(db.conn(), false).unwrap();
        assert!(!settings.offline_mode);
        assert!(settings.online_endpoint.url.is_empty());
        assert!(settings.airplane_endpoint.url.is_empty());
        assert!(settings.default_model.is_none());
    }

    #[test]
    #[ignore = "touches system keychain; run explicitly when validating keychain integration"]
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
        let server = spawn_test_server(|request| {
            assert!(request.starts_with("GET /v1/models HTTP/1.1"));
            http_ok(
                r#"{"data":[{"id":"gpt-4.1-mini","name":"GPT 4.1 mini","context_window":128000,"max_output_tokens":8192,"modalities":["text"]},{"id":"gpt-4.1","modalities":["text","vision"]}]}"#,
            )
        });
        let models = refresh_model_catalog(&AiEndpointConfig {
            kind: AiEndpointKind::Airplane,
            url: server,
            model: "gpt-4.1-mini".to_string(),
            api_key: None,
        })
        .unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0].name, "GPT 4.1 mini");
        assert!(models[1].supports_vision);
    }

    #[test]
    #[ignore = "touches system keychain; run explicitly when validating keychain integration"]
    fn run_one_shot_uses_active_endpoint_and_parses_response() {
        clear_keyring(AiEndpointKind::Online);
        let server = spawn_test_server(|request| {
            if request.starts_with("GET /v1/models HTTP/1.1") {
                return http_ok(r#"{"data":[{"id":"gpt-4.1-mini"}]}"#);
            }
            assert!(request.starts_with("POST /v1/chat/completions HTTP/1.1"));
            assert!(request.contains("authorization: Bearer secret-token") || request.contains("Authorization: Bearer secret-token"));
            http_ok(
                r#"{"choices":[{"message":{"content":"{\"title\":\"Better title\",\"excerpt\":\"Short summary\",\"slug\":\"better-title\"}"}}]}"#,
            )
        });

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
        save_model_preferences(db.conn(), None, Some("gpt-4.1-mini"), None, "").unwrap();

        let response = run_one_shot(
            db.conn(),
            false,
            &OneShotRequest {
                operation: OneShotOperation::AnalyzePost,
                content: json!({"title":"Draft title","excerpt":"","content":"Body"}),
            },
        )
        .unwrap();

        assert_eq!(
            response,
            OneShotResponse::PostAnalysis(PostAnalysisResult {
                title: "Better title".to_string(),
                excerpt: "Short summary".to_string(),
                slug: "better-title".to_string(),
            })
        );
    }

    fn spawn_test_server(handler: impl Fn(String) -> String + Send + 'static) -> String {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        thread::spawn(move || {
            for stream in listener.incoming().take(2) {
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