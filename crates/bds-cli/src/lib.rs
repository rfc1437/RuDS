use std::fmt::Write as _;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use anyhow::{Context, Result, anyhow, bail};
use bds_core::db::{Database, DbConnection};
use bds_core::engine::{self, cli_sync, domain_events};
use bds_core::model::{DomainEntity, NotificationAction, Project, ScriptKind};
use bds_core::scripting::{CoreHost, ExecutionControl, ExecutionKind, execute_many_with_host};
use clap::{Args, Parser, Subcommand, ValueEnum};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Parser)]
#[command(
    name = "bds-cli",
    version,
    about = "RuDS workspace automation using the desktop application's shared engines"
)]
pub struct Cli {
    /// Print a stable JSON result envelope.
    #[arg(long, global = true)]
    pub json: bool,

    /// Gate network activity and route automatic AI work to the local endpoint.
    #[arg(long, global = true)]
    pub airplane: bool,

    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Rebuild the cache database from the active project's files.
    Rebuild {
        #[arg(long)]
        incremental: bool,
    },
    /// Run one derived-data repair task.
    Repair { part: RepairPart },
    /// Render the generated site.
    Render {
        #[arg(long, conflicts_with = "force")]
        incremental: bool,
        #[arg(long, conflicts_with = "incremental")]
        force: bool,
    },
    /// Upload generated HTML, thumbnails, and media using publishing settings.
    Upload,
    /// Push the active project repository to origin.
    Push,
    /// Fast-forward pull the active project and reconcile its cache database.
    Pull,
    /// Create a post from flags or JSON stdin.
    Post(PostArgs),
    /// Import and optionally AI-enrich one image.
    Media {
        file: PathBuf,
        #[arg(long)]
        language: Option<String>,
    },
    /// Create a post and import/link all supplied images.
    Gallery(GalleryArgs),
    /// Read or update global application settings.
    Config {
        #[command(subcommand)]
        command: ConfigCommand,
    },
    /// List, add, or switch projects in the shared registry.
    Project {
        #[command(subcommand)]
        command: ProjectCommand,
    },
    /// Start the authenticated headless SSH server.
    Server(ServerArgs),
    /// Start interactive TUI mode (normally intercepted by the packaged launcher).
    Tui,
    /// Run an enabled utility Lua script from the active project.
    Lua {
        script: String,
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Install this packaged CLI in ~/.local/bin.
    Install,
}

#[derive(Debug, Args, Default)]
pub struct ServerArgs {
    /// SSH listen address. Defaults to loopback; external access must be explicit.
    #[arg(long)]
    pub bind: Option<IpAddr>,
    /// SSH listen port.
    #[arg(long)]
    pub port: Option<u16>,
    /// Application database path.
    #[arg(long)]
    pub database: Option<PathBuf>,
    /// Private application data directory containing SSH key material.
    #[arg(long)]
    pub data_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum RepairPart {
    PostLinks,
    MediaLinks,
    Thumbnails,
    Embeddings,
    Search,
}

#[derive(Debug, Args, Default)]
pub struct PostArgs {
    /// Read the post object as JSON from standard input.
    #[arg(long)]
    pub stdin: bool,
    /// Skip automatic translation after creation.
    #[arg(long)]
    pub no_translate: bool,
    /// Post title (required unless --stdin is used).
    #[arg(long)]
    pub title: Option<String>,
    /// Markdown post body.
    #[arg(long)]
    pub content: Option<String>,
    /// Short post excerpt.
    #[arg(long)]
    pub excerpt: Option<String>,
    /// Author name.
    #[arg(long)]
    pub author: Option<String>,
    /// BCP 47 source language; detected when omitted.
    #[arg(long)]
    pub language: Option<String>,
    /// Post template slug.
    #[arg(long)]
    pub template: Option<String>,
    /// Comma-separated tags.
    #[arg(long, value_delimiter = ',')]
    pub tags: Vec<String>,
    /// Comma-separated categories.
    #[arg(long, value_delimiter = ',')]
    pub categories: Vec<String>,
}

#[derive(Debug, Args, Default)]
pub struct GalleryArgs {
    #[command(flatten)]
    pub post: PostArgs,
    /// Image files to import and link to the new post.
    #[arg(value_name = "IMAGE")]
    pub images: Vec<PathBuf>,
}

#[derive(Debug, Subcommand)]
pub enum ConfigCommand {
    Get { key: String },
    Set { key: String, value: String },
    List,
}

#[derive(Debug, Subcommand)]
pub enum ProjectCommand {
    List,
    Add {
        path: PathBuf,
        #[arg(long)]
        name: Option<String>,
    },
    Switch {
        project: String,
    },
}

#[derive(Debug, Clone)]
pub struct RunContext {
    pub database_path: PathBuf,
    pub stdin: String,
    pub home_dir: PathBuf,
    pub executable_path: PathBuf,
}

impl RunContext {
    pub fn system() -> Self {
        Self {
            database_path: bds_core::util::application_database_path(),
            stdin: String::new(),
            home_dir: dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")),
            executable_path: std::env::current_exe().unwrap_or_else(|_| PathBuf::from("bds-cli")),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct CommandOutput {
    pub command: &'static str,
    pub message: String,
    pub data: Value,
    pub progress: Vec<String>,
    pub notices: Vec<String>,
    #[serde(skip)]
    json: bool,
}

impl std::fmt::Display for CommandOutput {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.json {
            let envelope = json!({
                "ok": true,
                "command": self.command,
                "message": self.message,
                "data": self.data,
                "progress": self.progress,
                "notices": self.notices,
            });
            return write!(
                formatter,
                "{}",
                serde_json::to_string(&envelope).map_err(|_| std::fmt::Error)?
            );
        }
        for line in &self.progress {
            writeln!(formatter, "{line}")?;
        }
        for notice in &self.notices {
            writeln!(formatter, "Notice: {notice}")?;
        }
        formatter.write_str(&self.message)
    }
}

#[derive(Debug, Clone, Deserialize)]
struct PostInput {
    title: String,
    #[serde(default)]
    content: String,
    excerpt: Option<String>,
    author: Option<String>,
    language: Option<String>,
    template: Option<String>,
    #[serde(default, deserialize_with = "string_list")]
    tags: Vec<String>,
    #[serde(default, deserialize_with = "string_list")]
    categories: Vec<String>,
    #[serde(default)]
    images: Vec<PathBuf>,
}

fn string_list<'de, D>(deserializer: D) -> std::result::Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Value::deserialize(deserializer)?;
    Ok(match value {
        Value::Array(values) => values
            .into_iter()
            .filter_map(|value| value.as_str().map(str::to_string))
            .collect(),
        Value::String(value) => split_list(&value),
        _ => Vec::new(),
    })
}

pub fn run(cli: Cli, context: RunContext) -> Result<CommandOutput> {
    let command_name = command_name(&cli.command);
    let mut output = execute(cli.command, &context, cli.airplane)?;
    output.command = command_name;
    output.json = cli.json;
    Ok(output)
}

fn execute(command: Command, context: &RunContext, airplane: bool) -> Result<CommandOutput> {
    if matches!(command, Command::Install) {
        return install_launcher(context);
    }
    if matches!(command, Command::Tui | Command::Server(_)) {
        bail!("server and TUI modes are started directly by the native bds-cli entry point");
    }

    let db = open_database(&context.database_path)?;
    match command {
        Command::Rebuild { incremental } => rebuild(&db, incremental),
        Command::Repair { part } => repair(&db, part),
        Command::Render { incremental, force } => render(&db, incremental, force),
        Command::Upload => upload(&db, airplane),
        Command::Push => git_push(&db, airplane),
        Command::Pull => git_pull(&db, airplane),
        Command::Post(args) => create_post(&db, args, &context.stdin, airplane),
        Command::Media { file, language } => import_media(&db, &file, language, airplane),
        Command::Gallery(args) => create_gallery(&db, args, &context.stdin, airplane),
        Command::Config { command } => config(&db, command),
        Command::Project { command } => project(&db, command),
        Command::Lua { script, args } => run_lua(&db, &script, &args, airplane),
        Command::Server(_) | Command::Tui | Command::Install => unreachable!(),
    }
}

fn open_database(path: &Path) -> Result<Database> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "could not create application data directory {}",
                parent.display()
            )
        })?;
    }
    let db = Database::open(path).with_context(|| format!("could not open {}", path.display()))?;
    db.migrate()
        .map_err(|error| anyhow!("could not migrate the shared database: {error}"))?;
    engine::search::prepare_search_index(db.conn())
        .context("could not prepare the shared search index")?;
    Ok(db)
}

fn active_project(db: &Database) -> Result<(Project, PathBuf)> {
    let project = engine::project::get_active_project(db.conn())?
        .ok_or_else(|| anyhow!("no active project selected; use: project switch <project>"))?;
    let data_dir = project
        .data_path
        .as_deref()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("active project {} has no portable data path", project.id))?;
    if !data_dir.is_dir() {
        bail!(
            "active project folder does not exist: {}",
            data_dir.display()
        );
    }
    Ok((project, data_dir))
}

fn rebuild(db: &Database, incremental: bool) -> Result<CommandOutput> {
    let (project, data_dir) = active_project(db)?;
    if incremental {
        let report = cli_sync::run_cli_mutation(db.conn(), || {
            let report = engine::rebuild::rebuild_incremental(db.conn(), &data_dir, &project.id)?;
            if report.differences_applied > 0 || report.orphans_imported > 0 {
                emit_bulk(&project.id);
            }
            Ok(report)
        })?;
        return Ok(output(
            "Applied incremental filesystem changes",
            json!({
                "differences_applied": report.differences_applied,
                "orphans_imported": report.orphans_imported,
                "orphans_failed": report.orphans_failed,
            }),
        ));
    }

    let progress = Arc::new(Mutex::new(Vec::new()));
    let progress_sink = Arc::clone(&progress);
    let (report, thumbnails) = cli_sync::run_cli_mutation(db.conn(), || {
        let report = engine::rebuild::rebuild_from_filesystem_with_progress(
            db.conn(),
            &data_dir,
            &project.id,
            Some(Arc::new(move |value, message| {
                let line = format!("[{:>3}%] {message}", (value * 100.0).round() as i32);
                let mut progress = progress_sink
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                if progress.last() != Some(&line) {
                    progress.push(line);
                }
            })),
        )?;
        let thumbnails =
            engine::media::regenerate_missing_thumbnails(db.conn(), &data_dir, &project.id)?;
        emit_bulk(&project.id);
        Ok((report, thumbnails))
    })?;
    let mut result = output(
        "Rebuild complete",
        json!({
            "posts_created": report.posts_created,
            "posts_updated": report.posts_updated,
            "translations_created": report.translations_created,
            "translations_updated": report.translations_updated,
            "media_created": report.media_created,
            "media_updated": report.media_updated,
            "templates_created": report.templates_created,
            "templates_updated": report.templates_updated,
            "scripts_created": report.scripts_created,
            "scripts_updated": report.scripts_updated,
            "thumbnails_generated": thumbnails.thumbnails_generated,
            "thumbnail_media_failed": thumbnails.media_failed,
        }),
    );
    result.progress = progress
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .clone();
    Ok(result)
}

fn repair(db: &Database, part: RepairPart) -> Result<CommandOutput> {
    let (project, data_dir) = active_project(db)?;
    match part {
        RepairPart::PostLinks => {
            let links = cli_sync::run_cli_mutation(db.conn(), || {
                let links = engine::post::rebuild_all_links(db.conn(), &data_dir, &project.id)?;
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Post,
                    "*",
                    NotificationAction::Updated,
                );
                Ok(links)
            })?;
            Ok(output("Post links rebuilt", json!({"links": links})))
        }
        RepairPart::MediaLinks => {
            let report = cli_sync::run_cli_mutation(db.conn(), || {
                let report = engine::media::rebuild_media_links(db.conn(), &data_dir, &project.id)?;
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Media,
                    "*",
                    NotificationAction::Updated,
                );
                Ok(report)
            })?;
            Ok(output(
                "Media links rebuilt",
                json!({"links": report.links}),
            ))
        }
        RepairPart::Thumbnails => {
            let report = cli_sync::run_cli_mutation(db.conn(), || {
                let report = engine::media::regenerate_missing_thumbnails(
                    db.conn(),
                    &data_dir,
                    &project.id,
                )?;
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Media,
                    "*",
                    NotificationAction::Updated,
                );
                Ok(report)
            })?;
            Ok(output(
                "Missing thumbnails regenerated",
                json!({
                    "media_processed": report.media_processed,
                    "media_repaired": report.media_repaired,
                    "media_failed": report.media_failed,
                    "thumbnails_generated": report.thumbnails_generated,
                }),
            ))
        }
        RepairPart::Search => {
            let report = cli_sync::run_cli_mutation(db.conn(), || {
                let report = engine::search::reindex_project(db.conn(), &project.id, None)?;
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Post,
                    "*",
                    NotificationAction::Updated,
                );
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Media,
                    "*",
                    NotificationAction::Updated,
                );
                Ok(report)
            })?;
            Ok(output(
                "Search text reindexed",
                json!({"posts_indexed": report.posts_indexed, "media_indexed": report.media_indexed}),
            ))
        }
        RepairPart::Embeddings => {
            let metadata = engine::meta::read_project_json(&data_dir)?;
            if metadata.semantic_similarity_enabled {
                let service = engine::embedding::EmbeddingService::production(db.conn(), &data_dir);
                let indexed = service.reindex_all(&project.id)?;
                service.flush_project(&project.id)?;
                return Ok(output(
                    "Embedding index rebuilt",
                    json!({"rebuilt": indexed.len(), "disabled": false}),
                ));
            }
            Ok(output(
                "Embedding repair skipped because semantic similarity is disabled",
                json!({"rebuilt": 0, "disabled": true}),
            ))
        }
    }
}

fn render(db: &Database, incremental: bool, force: bool) -> Result<CommandOutput> {
    let (project, data_dir) = active_project(db)?;
    let metadata = engine::meta::read_project_json(&data_dir)?;
    let posts = published_sources(db.conn(), &data_dir, &project.id)?;
    let output_dir = data_dir.join("html");
    std::fs::create_dir_all(&output_dir)?;
    if incremental {
        let validation = engine::validate_site::validate_site(db.conn(), &data_dir, &project.id)?;
        let sections = engine::generation::sections_from_validation_report(&validation, &metadata);
        let report = engine::generation::apply_validation_sections(
            db.conn(),
            &output_dir,
            &project.id,
            &metadata,
            &posts,
            &sections,
        )?;
        return Ok(output(
            "Validation differences applied",
            json!({"written": report.written_paths.len(), "skipped": report.skipped_paths.len(), "deleted": report.deleted_paths.len()}),
        ));
    }
    if force {
        engine::generation::clear_generation_cache(db.conn(), &project.id)?;
    }
    let report = engine::generation::generate_starter_site(
        db.conn(),
        &output_dir,
        &project.id,
        &metadata,
        &posts,
        metadata.main_language.as_deref().unwrap_or("en"),
    )?;
    Ok(output(
        "Site rendered",
        json!({"written": report.written_paths.len(), "skipped": report.skipped_paths.len(), "deleted": report.deleted_paths.len(), "force": force}),
    ))
}

fn published_sources(
    conn: &DbConnection,
    data_dir: &Path,
    project_id: &str,
) -> Result<Vec<engine::generation::PublishedPostSource>> {
    let mut sources = Vec::new();
    for post in bds_core::db::queries::post::list_posts_by_project(conn, project_id)? {
        if let Some(source) = engine::generation::load_published_post_source(data_dir, post)? {
            sources.push(source);
        }
    }
    Ok(sources)
}

fn upload(db: &Database, airplane: bool) -> Result<CommandOutput> {
    if airplane {
        bail!("upload is unavailable in airplane mode");
    }
    let (_project, data_dir) = active_project(db)?;
    let preferences = engine::meta::read_publishing_json(&data_dir)?;
    let private_cache_dir = db
        .conn()
        .database_path()?
        .parent()
        .map(PathBuf::from)
        .ok_or_else(|| anyhow!("private application directory unavailable"))?;
    let job = engine::publishing::upload_site(
        &data_dir,
        &private_cache_dir,
        &preferences,
        |_current, _total, _kind| {},
    )?;
    Ok(output(
        "Upload complete",
        json!({"targets": job.completed_targets.len()}),
    ))
}

fn git_push(db: &Database, airplane: bool) -> Result<CommandOutput> {
    if airplane {
        bail!("git push is unavailable in airplane mode");
    }
    let (_project, data_dir) = active_project(db)?;
    let result = engine::git::GitEngine::new(data_dir).push(|| false, |_| {})?;
    Ok(output("Pushed", json!({"output": result.output})))
}

fn git_pull(db: &Database, airplane: bool) -> Result<CommandOutput> {
    if airplane {
        bail!("git pull is unavailable in airplane mode");
    }
    let (_project, data_dir) = active_project(db)?;
    let result = engine::git::GitEngine::new(data_dir).pull(|| false, |_| {})?;
    let mut rebuilt = rebuild(db, true)?;
    rebuilt.message = "Pulled and reconciled the cache database".into();
    rebuilt.data["git_output"] = Value::String(result.output);
    Ok(rebuilt)
}

fn create_post(
    db: &Database,
    args: PostArgs,
    stdin: &str,
    airplane: bool,
) -> Result<CommandOutput> {
    let (project, data_dir) = active_project(db)?;
    let no_translate = args.no_translate;
    let mut input = post_input(args, stdin)?;
    let mut notices = Vec::new();
    ensure_language(db.conn(), &mut input, airplane, &mut notices);
    let metadata = engine::meta::read_project_json(&data_dir)?;
    let (post, translation) = cli_sync::run_cli_mutation(db.conn(), || {
        let mut post = engine::post::create_post(
            db.conn(),
            &data_dir,
            &project.id,
            &input.title,
            Some(&input.content),
            input.tags.clone(),
            input.categories.clone(),
            input.author.as_deref(),
            input.language.as_deref(),
            input.template.as_deref(),
        )?;
        if input.excerpt.is_some() {
            post = engine::post::update_post(
                db.conn(),
                &data_dir,
                &post.id,
                None,
                None,
                Some(input.excerpt.as_deref()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
        }
        let translation = if no_translate {
            None
        } else {
            Some(engine::auto_translation::translate_missing_for_post(
                db.conn(),
                &data_dir,
                &post.id,
                metadata.main_language.as_deref().unwrap_or("en"),
                &metadata.blog_languages,
                airplane,
                || false,
            ))
        };
        Ok((post, translation))
    })?;
    translation_notice(translation, &mut notices);
    Ok(CommandOutput {
        command: "post",
        message: format!(
            "Created post {} ({}, {})",
            post.id,
            post.slug,
            post.language.as_deref().unwrap_or("unknown language")
        ),
        data: json!({"id": post.id, "slug": post.slug, "language": post.language}),
        progress: Vec::new(),
        notices,
        json: false,
    })
}

fn import_media(
    db: &Database,
    file: &Path,
    language: Option<String>,
    airplane: bool,
) -> Result<CommandOutput> {
    require_file(file)?;
    let (project, data_dir) = active_project(db)?;
    let metadata = engine::meta::read_project_json(&data_dir)?;
    let language = language
        .or(metadata.main_language.clone())
        .unwrap_or_else(|| "en".into());
    let targets = engine::gallery_import::translation_targets(
        metadata.main_language.as_deref(),
        &metadata.blog_languages,
        &language,
    );
    let ai_available = engine::gallery_import::active_ai_endpoint_configured(db.conn(), airplane);
    let imported = cli_sync::run_cli_mutation(db.conn(), || {
        let original_name = file
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("image");
        let imported = engine::media::import_media(
            db.conn(),
            &data_dir,
            &project.id,
            file,
            original_name,
            None,
            None,
            None,
            None,
            Some(&language),
            Vec::new(),
        )?;
        if ai_available {
            let _ = engine::gallery_import::enrich_imported_image(
                db.conn(),
                &data_dir,
                &imported,
                airplane,
                &targets,
            );
        }
        Ok(imported)
    })?;
    let notices = (!ai_available)
        .then(|| {
            "AI enrichment was not run because the permitted endpoint is not configured".into()
        })
        .into_iter()
        .collect();
    Ok(CommandOutput {
        command: "media",
        message: format!(
            "Imported media {} ({})",
            imported.id, imported.original_name
        ),
        data: json!({"id": imported.id, "file_path": imported.file_path, "language": imported.language}),
        progress: Vec::new(),
        notices,
        json: false,
    })
}

fn create_gallery(
    db: &Database,
    args: GalleryArgs,
    stdin: &str,
    airplane: bool,
) -> Result<CommandOutput> {
    let no_translate = args.post.no_translate;
    let stdin_mode = args.post.stdin;
    let supplied_images = args.images;
    let mut input = post_input(args.post, stdin)?;
    if !stdin_mode {
        input.images = supplied_images;
    }
    if input.images.is_empty() {
        bail!("pass at least one image (or use --stdin with a non-empty images array)");
    }
    for image in &input.images {
        require_file(image)?;
    }
    let (project, data_dir) = active_project(db)?;
    let metadata = engine::meta::read_project_json(&data_dir)?;
    let mut notices = Vec::new();
    ensure_language(db.conn(), &mut input, airplane, &mut notices);
    let post = cli_sync::run_cli_mutation(db.conn(), || {
        let mut post = engine::post::create_post(
            db.conn(),
            &data_dir,
            &project.id,
            &input.title,
            Some(&input.content),
            input.tags.clone(),
            input.categories.clone(),
            input.author.as_deref(),
            input.language.as_deref(),
            input.template.as_deref(),
        )?;
        if input.excerpt.is_some() {
            post = engine::post::update_post(
                db.conn(),
                &data_dir,
                &post.id,
                None,
                None,
                Some(input.excerpt.as_deref()),
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            )?;
        }
        let report = engine::gallery_import::import_gallery_images(
            &db.conn().database_path().map_err(engine::EngineError::Db)?,
            &data_dir,
            &project.id,
            &post.id,
            input.images.clone(),
            input.language.as_deref().unwrap_or("en"),
            airplane,
        );
        for outcome in &report.outcomes {
            if let Ok(imported) = &outcome.result {
                domain_events::entity_changed(
                    &project.id,
                    DomainEntity::Media,
                    &imported.media_id,
                    NotificationAction::Created,
                );
            }
        }
        domain_events::entity_changed(
            &project.id,
            DomainEntity::Post,
            &post.id,
            NotificationAction::Updated,
        );
        if !no_translate {
            let translation = engine::auto_translation::translate_missing_for_post(
                db.conn(),
                &data_dir,
                &post.id,
                metadata.main_language.as_deref().unwrap_or("en"),
                &metadata.blog_languages,
                airplane,
                || false,
            );
            translation_notice(Some(translation), &mut notices);
        }
        let failed = report
            .outcomes
            .iter()
            .filter(|item| item.result.is_err())
            .count();
        if failed > 0 {
            notices.push(format!(
                "{failed} image import(s) failed; successful imports remain linked"
            ));
        }
        Ok(post)
    })?;
    Ok(CommandOutput {
        command: "gallery",
        message: format!(
            "Created gallery post {} ({}) with {} image(s)",
            post.id,
            post.slug,
            input.images.len()
        ),
        data: json!({"id": post.id, "slug": post.slug, "images": input.images.len()}),
        progress: Vec::new(),
        notices,
        json: false,
    })
}

fn config(db: &Database, command: ConfigCommand) -> Result<CommandOutput> {
    match command {
        ConfigCommand::Get { key } => {
            let value = engine::settings::get_effective(db.conn(), &key)?
                .ok_or_else(|| anyhow!("{key} is not set"))?;
            let value = printable_config_value(&key, &value);
            Ok(output(&value, json!({"key": key, "value": value})))
        }
        ConfigCommand::Set { key, value } => {
            cli_sync::run_cli_mutation(db.conn(), || {
                if key == engine::settings::ONLINE_API_KEY {
                    engine::ai::save_online_api_key(db.conn(), &value)
                } else if key == engine::settings::AIRPLANE_API_KEY {
                    engine::ai::save_endpoint_api_key(
                        db.conn(),
                        engine::ai::AiEndpointKind::Airplane,
                        &value,
                    )
                } else {
                    engine::settings::set(db.conn(), &key, &value)
                }
            })?;
            let printable = printable_config_value(&key, &value);
            Ok(output(
                &format!("{key} = {printable}"),
                json!({"key": key, "value": printable}),
            ))
        }
        ConfigCommand::List => {
            let settings = engine::settings::list_effective(db.conn())?;
            let mut message = String::new();
            let mut values = serde_json::Map::new();
            for (key, value) in settings {
                let value = printable_config_value(&key, &value);
                let _ = writeln!(message, "{key}={value}");
                values.insert(key, Value::String(value));
            }
            Ok(output(message.trim_end(), Value::Object(values)))
        }
    }
}

fn printable_config_value(key: &str, value: &str) -> String {
    let key = key.to_ascii_lowercase();
    if [
        "api_key",
        "access_key",
        "private_key",
        "password",
        "secret",
        "token",
        "credential",
    ]
    .iter()
    .any(|name| key.ends_with(name) || key.split(['.', '-']).any(|part| part == *name))
    {
        if value.trim().is_empty() {
            "<not set>".to_string()
        } else {
            "<set>".to_string()
        }
    } else {
        value.to_string()
    }
}

fn project(db: &Database, command: ProjectCommand) -> Result<CommandOutput> {
    match command {
        ProjectCommand::List => {
            let projects = engine::project::list_projects(db.conn())?;
            let message = projects
                .iter()
                .map(|project| {
                    format!(
                        "{} {}  {}  {}",
                        if project.is_active { "*" } else { " " },
                        project.id,
                        project.slug,
                        project.name
                    )
                })
                .collect::<Vec<_>>()
                .join("\n");
            Ok(output(&message, serde_json::to_value(projects)?))
        }
        ProjectCommand::Add { path, name } => {
            if !path.is_dir() {
                bail!("{} is not a directory", path.display());
            }
            let path = path.canonicalize()?;
            let project = cli_sync::run_cli_mutation(db.conn(), || {
                if path.join("meta/project.json").is_file() {
                    engine::project::open_project(db.conn(), &path)
                } else {
                    let fallback = path
                        .file_name()
                        .and_then(|value| value.to_str())
                        .unwrap_or("Blog");
                    engine::project::create_project(
                        db.conn(),
                        name.as_deref().unwrap_or(fallback),
                        path.to_str(),
                    )
                }
            })?;
            Ok(output(
                &format!(
                    "Added project {} ({}); switch with: project switch {}",
                    project.id, project.slug, project.slug
                ),
                serde_json::to_value(project)?,
            ))
        }
        ProjectCommand::Switch { project: reference } => {
            let selected = engine::project::list_projects(db.conn())?
                .into_iter()
                .find(|project| {
                    reference == project.id
                        || reference == project.slug
                        || reference == project.name
                })
                .ok_or_else(|| anyhow!("no project matches {reference:?}"))?;
            cli_sync::run_cli_mutation(db.conn(), || {
                engine::project::set_active_project(db.conn(), &selected.id)
            })?;
            Ok(output(
                &format!("Active project: {}", selected.name),
                serde_json::to_value(selected)?,
            ))
        }
    }
}

fn run_lua(db: &Database, slug: &str, args: &[String], airplane: bool) -> Result<CommandOutput> {
    let (project, data_dir) = active_project(db)?;
    let script = bds_core::db::queries::script::get_script_by_slug(db.conn(), &project.id, slug)
        .with_context(|| format!("no script with slug {slug:?} in the active project"))?;
    if script.kind != ScriptKind::Utility {
        bail!(
            "script {slug:?} is a {} script; only utility scripts can be run",
            script.kind.as_str()
        );
    }
    if !script.enabled {
        bail!("script {slug:?} is disabled");
    }
    let source = if let Some(content) = script.content {
        content
    } else {
        let raw = std::fs::read_to_string(data_dir.join(&script.file_path))?;
        bds_core::util::frontmatter::read_script_file(&raw)
            .map_err(|error| anyhow!(error))?
            .1
    };
    let values = args.iter().cloned().map(Value::String).collect::<Vec<_>>();
    let host = Arc::new(
        CoreHost::new(db.conn().database_path()?, project.id.clone(), data_dir)
            .with_offline_mode(airplane),
    );
    let execution = cli_sync::run_cli_mutation(db.conn(), || {
        execute_many_with_host(
            &source,
            &script.entrypoint,
            &values,
            ExecutionKind::Utility,
            &ExecutionControl::default(),
            host,
        )
        .map_err(|error| engine::EngineError::Validation(format!("script failed: {error}")))
    })?;
    let mut message = execution.output.join("\n");
    if !message.is_empty() {
        message.push('\n');
    }
    write!(message, "Script finished: {}", execution.value)?;
    Ok(output(
        &message,
        json!({"value": execution.value, "output": execution.output, "progress": execution.progress.iter().map(|item| json!({"current": item.current, "total": item.total, "message": item.message})).collect::<Vec<_>>() }),
    ))
}

fn install_launcher(context: &RunContext) -> Result<CommandOutput> {
    let target =
        engine::cli_launcher::install_launcher(&context.executable_path, &context.home_dir)?;
    Ok(output(
        &format!("Installed launcher at {}", target.display()),
        json!({"path": target}),
    ))
}

fn post_input(args: PostArgs, stdin: &str) -> Result<PostInput> {
    if args.stdin {
        if stdin.trim().is_empty() {
            bail!("no JSON data on stdin");
        }
        let input: PostInput = serde_json::from_str(stdin).context("invalid JSON on stdin")?;
        if input.title.trim().is_empty() {
            bail!("JSON post data needs a non-empty title");
        }
        return Ok(input);
    }
    let title = args
        .title
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("--title is required (or pass --stdin with JSON post data)"))?;
    Ok(PostInput {
        title,
        content: args.content.unwrap_or_default(),
        excerpt: args.excerpt,
        author: args.author,
        language: args.language,
        template: args.template,
        tags: cleaned(args.tags),
        categories: cleaned(args.categories),
        images: Vec::new(),
    })
}

fn ensure_language(
    conn: &DbConnection,
    input: &mut PostInput,
    airplane: bool,
    notices: &mut Vec<String>,
) {
    if input
        .language
        .as_deref()
        .is_some_and(|value| !value.trim().is_empty())
    {
        return;
    }
    let request = engine::ai::OneShotRequest {
        operation: engine::ai::OneShotOperation::DetectLanguage,
        content: json!({"title": input.title, "content": input.content}),
    };
    if let Ok((engine::ai::OneShotResponse::LanguageDetection(result), _)) =
        engine::ai::run_one_shot(conn, airplane, &request)
    {
        input.language = Some(result.language_code);
    } else {
        input.language = Some(
            engine::search::detect_language(&format!("{}\n{}", input.title, input.content)).into(),
        );
        notices.push("AI language detection was unavailable; used the offline heuristic".into());
    }
}

fn translation_notice(
    result: Option<engine::EngineResult<engine::auto_translation::FillMissingTranslationsReport>>,
    notices: &mut Vec<String>,
) {
    match result {
        None => {}
        Some(Ok(report)) if report.nothing_to_do => notices.push(
            "automatic translation was not needed (single language or already translated)".into(),
        ),
        Some(Ok(report)) if report.failed_count > 0 => notices.push(format!(
            "automatic translation completed with {} failure(s): {}",
            report.failed_count,
            report.errors.join("; ")
        )),
        Some(Ok(report)) => notices.push(format!(
            "automatic translation created {} post and {} media translation(s)",
            report.translated_posts, report.translated_media
        )),
        Some(Err(error)) => notices.push(format!(
            "automatic translation was not run (offline, unconfigured AI, or endpoint error): {error}"
        )),
    }
}

fn emit_bulk(project_id: &str) {
    for entity in [
        DomainEntity::Post,
        DomainEntity::Media,
        DomainEntity::Script,
        DomainEntity::Template,
    ] {
        domain_events::entity_changed(project_id, entity, "*", NotificationAction::Updated);
    }
}

fn require_file(path: &Path) -> Result<()> {
    if path.is_file() {
        Ok(())
    } else {
        bail!("no such file: {}", path.display())
    }
}

fn cleaned(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .collect()
}

fn split_list(value: &str) -> Vec<String> {
    cleaned(value.split(',').map(str::to_string).collect())
}

fn output(message: &str, data: Value) -> CommandOutput {
    CommandOutput {
        command: "",
        message: message.to_string(),
        data,
        progress: Vec::new(),
        notices: Vec::new(),
        json: false,
    }
}

fn command_name(command: &Command) -> &'static str {
    match command {
        Command::Rebuild { .. } => "rebuild",
        Command::Repair { .. } => "repair",
        Command::Render { .. } => "render",
        Command::Upload => "upload",
        Command::Push => "push",
        Command::Pull => "pull",
        Command::Post(_) => "post",
        Command::Media { .. } => "media",
        Command::Gallery(_) => "gallery",
        Command::Config { .. } => "config",
        Command::Project { .. } => "project",
        Command::Server(_) => "server",
        Command::Tui => "tui",
        Command::Lua { .. } => "lua",
        Command::Install => "install",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bds_core::model::ScriptKind;
    use clap::CommandFactory as _;
    use std::io::{Read as _, Write as _};
    use std::net::TcpListener;
    use std::thread;
    use tempfile::TempDir;

    struct Fixture {
        _root: TempDir,
        database_path: PathBuf,
        project_dir: PathBuf,
        home_dir: PathBuf,
    }

    impl Fixture {
        fn new(active: bool) -> Self {
            let root = tempfile::tempdir().unwrap();
            let database_path = root.path().join("app/bds.db");
            let project_dir = root.path().join("project");
            let home_dir = root.path().join("home");
            std::fs::create_dir_all(&project_dir).unwrap();
            std::fs::create_dir_all(&home_dir).unwrap();
            let db = open_database(&database_path).unwrap();
            if active {
                let project =
                    engine::project::create_project(db.conn(), "CLI Test", project_dir.to_str())
                        .unwrap();
                engine::project::set_active_project(db.conn(), &project.id).unwrap();
            }
            Self {
                _root: root,
                database_path,
                project_dir,
                home_dir,
            }
        }

        fn context(&self, stdin: &str) -> RunContext {
            RunContext {
                database_path: self.database_path.clone(),
                stdin: stdin.to_string(),
                home_dir: self.home_dir.clone(),
                executable_path: self._root.path().join("missing-bds-cli"),
            }
        }

        fn run(&self, args: &[&str], stdin: &str) -> Result<CommandOutput> {
            let cli = Cli::try_parse_from(std::iter::once("bds-cli").chain(args.iter().copied()))?;
            run(cli, self.context(stdin))
        }

        fn image(&self, name: &str) -> PathBuf {
            let path = self._root.path().join(name);
            std::fs::write(
                &path,
                include_bytes!(
                    "../../../fixtures/golden-generated-sites/rfc1437-sample/images/close.png"
                ),
            )
            .unwrap();
            path
        }
    }

    #[test]
    fn help_exposes_every_command_family_and_invalid_argv_fails() {
        let mut help = Vec::new();
        Cli::command().write_long_help(&mut help).unwrap();
        let help = String::from_utf8(help).unwrap();
        for command in [
            "rebuild", "repair", "render", "upload", "push", "pull", "post", "media", "gallery",
            "config", "project", "server", "tui", "lua", "install",
        ] {
            assert!(help.contains(command), "missing {command} from help");
        }
        let server = Cli::try_parse_from([
            "bds-cli",
            "server",
            "--bind",
            "127.0.0.2",
            "--port",
            "2233",
            "--database",
            "/tmp/bds.db",
            "--data-dir",
            "/tmp/bds",
        ])
        .unwrap();
        assert!(matches!(server.command, Command::Server(_)));
        assert!(Cli::try_parse_from(["bds-cli", "unknown"]).is_err());
        assert!(Cli::try_parse_from(["bds-cli", "render", "--incremental", "--force"]).is_err());
        assert!(Cli::try_parse_from(["bds-cli", "repair", "unknown"]).is_err());
    }

    #[test]
    fn config_and_project_families_dispatch_success_and_failure() {
        let fixture = Fixture::new(false);
        assert!(fixture.run(&["config", "get", "missing"], "").is_err());

        let defaults = fixture.run(&["config", "list"], "").unwrap();
        assert_eq!(defaults.data["editor.default_mode"], "markdown");
        assert_eq!(defaults.data["editor.diff_view_style"], "inline");
        assert_eq!(defaults.data["ai.endpoint.online.api_key"], "<not set>");
        let removed_prefix = ["style", "."].concat();
        assert!(
            defaults
                .data
                .as_object()
                .unwrap()
                .keys()
                .all(|key| !key.starts_with(&removed_prefix))
        );
        assert!(
            !defaults
                .message
                .contains("app.search-index-rebuild-required")
        );
        assert_eq!(
            fixture
                .run(&["config", "get", "editor.default_mode"], "")
                .unwrap()
                .message,
            "markdown"
        );

        fixture
            .run(&["config", "set", "editor.mode", "markdown"], "")
            .unwrap();
        let read = fixture
            .run(&["--json", "config", "get", "editor.mode"], "")
            .unwrap();
        assert_eq!(read.data["value"], "markdown");
        assert!(read.to_string().contains("\"ok\":true"));

        let secret = fixture
            .run(&["config", "set", "service.api_key", "secret-token"], "")
            .unwrap();
        assert_eq!(secret.message, "service.api_key = <set>");
        assert!(!secret.to_string().contains("secret-token"));
        let secret = fixture
            .run(&["--json", "config", "get", "service.api_key"], "")
            .unwrap();
        assert_eq!(secret.data["value"], "<set>");
        assert!(!secret.to_string().contains("secret-token"));
        let secrets = fixture.run(&["--json", "config", "list"], "").unwrap();
        assert_eq!(secrets.data["service.api_key"], "<set>");
        assert!(!secrets.to_string().contains("secret-token"));

        let db = open_database(&fixture.database_path).unwrap();
        engine::settings::set(db.conn(), "ai.endpoint.online.api_key_configured", "true").unwrap();
        let configured = fixture.run(&["config", "list"], "").unwrap();
        assert_eq!(configured.data["ai.endpoint.online.api_key"], "<set>");
        assert!(
            !configured
                .message
                .contains("ai.endpoint.online.api_key_configured")
        );

        assert!(fixture.run(&["project", "switch", "missing"], "").is_err());

        let project = fixture._root.path().join("second");
        std::fs::create_dir_all(&project).unwrap();
        fixture
            .run(
                &[
                    "project",
                    "add",
                    project.to_str().unwrap(),
                    "--name",
                    "Second Blog",
                ],
                "",
            )
            .unwrap();
        fixture
            .run(&["project", "switch", "second-blog"], "")
            .unwrap();
        let listed = fixture.run(&["project", "list"], "").unwrap();
        assert!(listed.message.contains("Second Blog"));
    }

    #[test]
    fn post_family_supports_flags_json_fallback_language_and_one_notification() {
        let fixture = Fixture::new(true);
        assert!(fixture.run(&["post"], "").is_err());
        let created = fixture
            .run(
                &[
                    "post",
                    "--title",
                    "Über Rust",
                    "--content",
                    "Das ist ein Beitrag und die Sprache ist Deutsch.",
                    "--excerpt",
                    "Kurz",
                    "--tags",
                    "rust,cli",
                    "--no-translate",
                ],
                "",
            )
            .unwrap();
        assert_eq!(created.data["language"], "de");
        assert!(created.notices[0].contains("offline heuristic"));
        let db = open_database(&fixture.database_path).unwrap();
        let notifications =
            bds_core::db::queries::db_notification::list_unseen_cli_notifications(db.conn())
                .unwrap();
        assert_eq!(notifications.len(), 1);
        assert_eq!(notifications[0].entity_type, DomainEntity::Post);

        let json_post = fixture
            .run(
                &["post", "--stdin", "--no-translate"],
                r#"{"title":"JSON post","content":"body","language":"en","categories":["article"]}"#,
            )
            .unwrap();
        assert_eq!(json_post.data["slug"], "json-post");
        assert!(fixture.run(&["post", "--stdin"], "not json").is_err());
    }

    #[test]
    fn post_language_detection_uses_configured_airplane_endpoint() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());
        thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut request = [0_u8; 8192];
            let _ = stream.read(&mut request).unwrap();
            let body = r#"{"choices":[{"message":{"content":"{\"language_code\":\"fr\"}"}}],"usage":{"prompt_tokens":1,"completion_tokens":1}}"#;
            let response = format!(
                "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            stream.write_all(response.as_bytes()).unwrap();
        });

        let fixture = Fixture::new(true);
        let db = open_database(&fixture.database_path).unwrap();
        engine::ai::save_endpoint(
            db.conn(),
            &engine::ai::AiEndpointConfig {
                kind: engine::ai::AiEndpointKind::Airplane,
                url: endpoint,
                model: "local-model".into(),
                api_key: None,
            },
        )
        .unwrap();
        let created = fixture
            .run(
                &[
                    "--airplane",
                    "post",
                    "--title",
                    "Bonjour",
                    "--content",
                    "Texte sans accents",
                    "--no-translate",
                ],
                "",
            )
            .unwrap();
        assert_eq!(created.data["language"], "fr");
        assert!(created.notices.is_empty());
    }

    #[test]
    fn media_and_gallery_families_dispatch_shared_import_pipeline() {
        let fixture = Fixture::new(true);
        assert!(fixture.run(&["media", "missing.png"], "").is_err());
        let image = fixture.image("one.png");
        let imported = fixture
            .run(
                &[
                    "--airplane",
                    "media",
                    image.to_str().unwrap(),
                    "--language",
                    "en",
                ],
                "",
            )
            .unwrap();
        assert!(imported.message.contains("Imported media"));
        assert!(
            fixture
                .project_dir
                .join("media")
                .read_dir()
                .unwrap()
                .next()
                .is_some()
        );
        let imported_id = imported.data["id"].as_str().unwrap().to_string();
        let db = open_database(&fixture.database_path).unwrap();
        let stored =
            bds_core::db::queries::media::get_media_by_id(db.conn(), &imported_id).unwrap();
        assert!(fixture.project_dir.join(&stored.sidecar_path).is_file());
        fixture.run(&["rebuild"], "").unwrap();
        assert!(
            bds_core::db::queries::media::get_media_by_id(db.conn(), &imported_id).is_ok(),
            "filesystem rebuild must restore CLI-imported media"
        );

        let second = fixture.image("two.png");
        let gallery = fixture
            .run(
                &[
                    "--airplane",
                    "gallery",
                    "--title",
                    "Gallery",
                    "--language",
                    "en",
                    "--no-translate",
                    second.to_str().unwrap(),
                ],
                "",
            )
            .unwrap();
        assert_eq!(gallery.data["images"], 1);
        assert!(
            fixture
                .run(&["gallery", "--title", "Empty", "--no-translate"], "",)
                .is_err()
        );
    }

    #[test]
    fn rebuild_repair_and_render_families_dispatch_success_and_failure() {
        let fixture = Fixture::new(true);
        fixture.run(&["rebuild"], "").unwrap();
        fixture.run(&["rebuild", "--incremental"], "").unwrap();
        for part in ["post-links", "media-links", "thumbnails", "search"] {
            fixture.run(&["repair", part], "").unwrap();
        }
        let skipped = fixture.run(&["repair", "embeddings"], "").unwrap();
        assert_eq!(skipped.data["disabled"], true);
        let mut metadata = engine::meta::read_project_json(&fixture.project_dir).unwrap();
        metadata.semantic_similarity_enabled = true;
        engine::meta::write_project_json(&fixture.project_dir, &metadata).unwrap();
        let repaired = fixture.run(&["repair", "embeddings"], "").unwrap();
        assert_eq!(repaired.data["rebuilt"], 0);
        fixture.run(&["rebuild"], "").unwrap();
        metadata.semantic_similarity_enabled = false;
        engine::meta::write_project_json(&fixture.project_dir, &metadata).unwrap();
        fixture.run(&["render"], "").unwrap();
        fixture.run(&["render", "--force"], "").unwrap();
        fixture.run(&["render", "--incremental"], "").unwrap();
        assert!(fixture.project_dir.join("html/index.html").is_file());

        let no_project = Fixture::new(false);
        assert!(no_project.run(&["rebuild"], "").is_err());
        assert!(no_project.run(&["repair", "search"], "").is_err());
        assert!(no_project.run(&["render"], "").is_err());
    }

    #[test]
    fn lua_family_runs_only_enabled_utility_scripts() {
        let fixture = Fixture::new(true);
        let db = open_database(&fixture.database_path).unwrap();
        let (project, _) = active_project(&db).unwrap();
        engine::script::create_script(
            db.conn(),
            &project.id,
            "Echo",
            ScriptKind::Utility,
            "function main(value) print(value); return value end",
            Some("main"),
        )
        .unwrap();
        engine::script::create_script(
            db.conn(),
            &project.id,
            "Macro",
            ScriptKind::Macro,
            "function render() return 'x' end",
            Some("render"),
        )
        .unwrap();
        let execution = fixture.run(&["lua", "echo", "hello"], "").unwrap();
        assert!(execution.message.contains("hello"));
        assert!(fixture.run(&["lua", "macro"], "").is_err());
        assert!(fixture.run(&["lua", "missing"], "").is_err());
    }

    #[test]
    fn external_and_launcher_families_have_actionable_failure_dispatch() {
        let fixture = Fixture::new(true);
        assert!(fixture.run(&["upload"], "").is_err());
        assert!(fixture.run(&["push"], "").is_err());
        assert!(fixture.run(&["pull"], "").is_err());
        assert!(
            fixture
                .run(&["--airplane", "push"], "")
                .unwrap_err()
                .to_string()
                .contains("airplane")
        );
        assert!(fixture.run(&["tui"], "").is_err());
        assert!(fixture.run(&["install"], "").is_err());
    }

    #[test]
    fn launcher_install_is_idempotent_and_refuses_overwrite() {
        let fixture = Fixture::new(false);
        let executable = fixture._root.path().join("packaged-bds-cli");
        std::fs::write(&executable, b"binary").unwrap();
        let context = RunContext {
            executable_path: executable.clone(),
            ..fixture.context("")
        };
        run(
            Cli::try_parse_from(["bds-cli", "install"]).unwrap(),
            context.clone(),
        )
        .unwrap();
        run(
            Cli::try_parse_from(["bds-cli", "install"]).unwrap(),
            context,
        )
        .unwrap();
        let target = fixture.home_dir.join(".local/bin/bds-cli");
        assert!(!target.is_symlink());
        assert!(
            std::fs::read_to_string(target)
                .unwrap()
                .contains(executable.canonicalize().unwrap().to_str().unwrap())
        );
    }
}
