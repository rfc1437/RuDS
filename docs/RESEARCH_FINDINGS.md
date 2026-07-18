# bDS Rust Rewrite — Research Findings

Research completed during M0 from the TypeScript codebase at `../bDS2/`.

## Database Schema (16 migrations, 0000–0015)

### Active Tables

| Table | Primary Key | Key Columns |
|---|---|---|
| `projects` | `id TEXT` | name, slug, description, data_path, is_active, created_at, updated_at |
| `settings` | `key TEXT` | value, updated_at |
| `posts` | `id TEXT` | project_id, title, slug, excerpt, content, status, author, language, do_not_translate, template_slug, file_path, checksum, tags, categories, published_title/content/tags/categories/excerpt, created_at, updated_at, published_at |
| `post_translations` | `id TEXT` | project_id, translation_for, language, title, excerpt, content, status, file_path, checksum, created_at, updated_at, published_at |
| `post_links` | `id TEXT` | source_post_id, target_post_id, link_text, created_at |
| `post_media` | `id TEXT` | project_id, post_id, media_id, sort_order, created_at |
| `media` | `id TEXT` | project_id, filename, original_name, mime_type, size, width, height, title, alt, caption, author, language, file_path, sidecar_path, checksum, tags, created_at, updated_at |
| `media_translations` | `id TEXT` | project_id, translation_for, language, title, alt, caption, created_at, updated_at |
| `tags` | `id TEXT` | project_id, name, color, post_template_slug, created_at, updated_at |
| `templates` | `id TEXT` | project_id, slug, title, kind, enabled, version, file_path, status, content, created_at, updated_at |
| `scripts` | `id TEXT` | project_id, slug, title, kind, entrypoint, enabled, version, file_path, status, content, created_at, updated_at |
| `generated_file_hashes` | (project_id, relative_path) UNIQUE | content_hash, updated_at |
| `db_notifications` | `id INTEGER AUTOINCREMENT` | entity, entity_id, action, from_cli, seen_at, created_at |
| `chat_conversations` | `id TEXT` | title, model, copilot_session_id, created_at, updated_at |
| `chat_messages` | `id INTEGER AUTOINCREMENT` | conversation_id, role, content, tool_call_id, tool_calls, created_at |
| `import_definitions` | `id TEXT` | project_id, name, wxr_file_path, uploads_folder_path, last_analysis_result, created_at, updated_at |
| `ai_models` | (provider, model_id) | name, family, many feature flags, pricing, context_window, etc. |
| `ai_catalog_meta` | `key TEXT` | value |
| `ai_model_modalities` | (provider, model_id, direction, modality) | — |
| `ai_providers` | `id TEXT` | name, env, npm, api, doc, updated_at |
| `dismissed_duplicate_pairs` | `id TEXT` | project_id, post_id_a, post_id_b, dismissed_at |
| `embedding_keys` | `label INTEGER` | post_id, project_id, content_hash, vector (blob) |

### Dropped Tables
- `sync_log` (dropped in 0001)
- `model_catalog`, `model_catalog_meta` (dropped in 0009, replaced by ai_models etc.)

### Timestamps
All timestamps are **Unix integers** (seconds since epoch), NOT ISO strings.

### FTS5
**No FTS5 virtual tables in migrations.** FTS is created at runtime.

### Unique Indexes
- `posts_project_slug_idx` (project_id, slug)
- `post_translations_translation_language_idx` (translation_for, language)
- `media_translations_translation_language_idx` (translation_for, language)
- `tags_project_name_idx` (project_id, name)
- `templates_project_slug_idx` (project_id, slug)
- `scripts_project_slug_idx` (project_id, slug)
- `post_media_post_media_idx` (post_id, media_id)
- `generated_file_hashes_project_path_idx` (project_id, relative_path)
- `dismissed_pairs_idx` (project_id, post_id_a, post_id_b)

## File Format Details

### Post Files
- Path: `posts/YYYY/MM/{slug}.md`
- Frontmatter (YAML via gray-matter): id, title, slug, status, createdAt, updatedAt, tags, categories
- Conditional: excerpt, author, language, doNotTranslate, templateSlug, publishedAt
- Body: markdown after frontmatter

### Translation Files
- Path: `posts/YYYY/MM/{slug}.{lang}.md` (same dir as source post)
- Frontmatter: translationFor (UUID), language, title, excerpt (optional)
- Body: translated content

### Media Sidecar Files
- Path: `{media-file}.meta` (canonical), `{media-file}.{lang}.meta` (translation)
- Format: hand-built YAML-like (NOT gray-matter), delimited by `---`
- Canonical fields: id, originalName, mimeType, size, createdAt, updatedAt, tags, width?, height?, title?, alt?, caption?, author?, language?, linkedPostIds?
- Translation fields: translationFor, language, title?, alt?, caption?

### Menu Document
- Path: `meta/menu.opml` (OPML 2.0)
- Library: fast-xml-parser (XMLParser/XMLBuilder)
- Item kinds: page, submenu, category-archive, home
- Home entry always enforced at position [0]

### Metadata JSON Files
| File | Managed By |
|---|---|
| `meta/project.json` | MetaEngine |
| `meta/categories.json` | MetaEngine |
| `meta/category-meta.json` | MetaEngine |
| `meta/publishing.json` | MetaEngine |
| `meta/tags.json` | TagEngine |
| `meta/menu.opml` | MenuEngine |

## Pagefind Integration
- npm dependency: `pagefind@^1.4.0`
- Invoked as CLI binary (`pagefind_extended`) via child process
- Generates per-language indexes: `{html}/pagefind/`, `{html}/{lang}/pagefind/`
- Loaded from templates via `<link>` and `<script>` tags in `partials/head.liquid`
- Post body marked with `data-pagefind-body` attribute
- **Rust plan:** Use `pagefind` crate library API instead of CLI

## Slug Generation
- TypeScript: `transliterate(input).toLowerCase().replace(/[^a-z0-9]+/g, '-').replace(/^-+|-+$/g, '')`
- Rust: `deunicode(input)` + same pipeline
- Need corpus comparison test to verify edge cases

## Publishing
- `meta/publishing.json`: sshHost, sshUser, sshRemotePath, sshMode (scp|rsync)
- SSH agent auth only (SSH_AUTH_SOCK), no passwords
- Three parallel upload targets: html/, thumbnails/, media/ (excluding .meta)

## MetadataDiff Compared Fields
- Posts: tags, categories, title, excerpt, author, language, translationFor, doNotTranslate, status, templateSlug, createdAt, updatedAt, publishedAt
- Translations: translationFor, language, title, excerpt
- Media: title, alt, caption, author, tags, language
- Scripts: title, kind, entrypoint, enabled, version
- Templates: title, kind, enabled, version
- Supports bidirectional sync (DB→File, File→DB) + orphan detection
