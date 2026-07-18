# bDS Rust Rewrite — Execution Backlog

## Purpose

This document converts the core and extension plans into execution order. It is organized by milestone first and by crate second.

Rules:

1. Do not pull work forward from a later milestone unless it directly unblocks the current milestone.
2. Keep tasks vertically sliced where possible: engine + UI + tests.
3. Compatibility tasks outrank polish tasks.
4. The allium specs in `specs/` (synced from `../bDS2/specs/` on 2026-07-18) are the behaviour contract. When a task references a spec file, implement what the spec says and cover its invariants with unit tests.

## Status (2026-07-18)

- M0–M2: complete (items marked DONE below).
- M3: implemented (all views and the editor exist in `crates/bds-ui/src/views/`); items below are not individually ticked — verify against tests before re-doing anything.
- M4: in progress per git history ("M4 closing" commits). `bds-core/src/render/` exists (generation, macros, markdown, page renderer, routes, site, template lookup). `bds-core/src/ai/` does **not** exist yet — the one-shot AI client is unstarted.
- M5: not started (`bds-core/src/scripting/` is an empty stub; no PublishEngine).
- M6 (below): new — code-vs-spec gaps introduced by the bDS2 spec sync. Fold these in as the milestones they touch come up; none of them should wait for "later" if the code they touch is being written now.

## Milestone M0: Compatibility Baseline

### `bds-core`

- ~~create workspace and crate boundaries (bds-core, bds-editor, bds-ui, bds-cli)~~ **DONE**
- ~~add typed SQLite connection and query layer via `diesel`~~ **DONE**
- ~~add embedded migration loader via `diesel_migrations`~~ **DONE**
- ~~define initial shared model modules with `serde` derives~~ **DONE**
- ~~add checksum (`sha2`) and slug (`deunicode`) utilities~~ **DONE**
- ~~establish error handling conventions: `thiserror` for bds-core, `anyhow` for bds-ui/bds-cli~~ **DONE**
- ~~add `tokio` runtime as workspace dependency (used by bds-ui and bds-cli, not directly by bds-core engine code)~~ **DONE**

### `bds-editor`

- ~~create crate with ropey, syntect, cosmic-text dependencies~~ **DONE**
- ~~implement basic rope buffer wrapper with edit operations~~ **DONE**
- ~~implement syntect integration for markdown highlighting~~ **DONE**
- ~~implement cosmic-text layout for rendering highlighted text~~ **DONE** (measured monospace font metrics via cosmic-text FontSystem/Buffer; OnceLock-cached MonoMetrics replaces hardcoded constants)
- ~~implement basic cursor model (position, move, place by click)~~ **DONE** (up/down/left/right/home/end + click placement)
- ~~implement basic text insertion and deletion~~ **DONE** (insert, backspace, delete forward, enter)
- ~~implement Iced custom widget that composes buffer + highlight + layout + cursor~~ **DONE** (CodeEditor widget with gutter, text, cursor rendering + keyboard/mouse events)
- ~~implement basic vertical scrolling with viewport-aware rendering~~ **DONE** (scroll_by, ensure_cursor_visible, mouse wheel, viewport-clipped rendering)
- ~~verify IME composition events work through winit (early risk check)~~ **DONE** (verified: winit/Iced delivers composed chars via Key::Character; pre-edit display deferred to M3)

### `bds-ui`

- ~~create app entry point with Iced `Application` impl~~ **DONE**
- ~~wire Iced window creation~~ **DONE**
- ~~wire muda menu bar with skeleton App/File/Edit/View/Window/Help menus~~ **DONE** (stub)
- ~~wire macOS lifecycle shim via objc2 (application:openFile:, application:openURLs:) behind cfg gate~~ **DONE** (stub)
- ~~wire muda `MenuEvent` receiver as Iced `Subscription`~~ **DONE** (platform/menu.rs menu_subscription + app.rs subscription)

### `fixtures` and `docs`

- ~~collect representative fixture projects from current app~~ **DONE** (fixtures/compatibility-projects/rfc1437-sample/)
- ~~capture golden generated output for those fixtures~~ **DONE** (fixtures/golden-generated-sites/rfc1437-sample/ — subset from live TypeScript-generated site: 3 post pages × 2 languages, structural files, assets, feeds, sitemap, category page)
- ~~create initial compatibility inventory from the matrix template (must include `mediaTranslations`, `postLinks`, FTS5 tables)~~ **DONE** (docs/COMPATIBILITY_INVENTORY.md)
- ~~create Liquid feature inventory from default templates~~ **DONE** (docs/LIQUID_FEATURE_INVENTORY.md)
- ~~determine whether current app uses Pagefind or another client-side search index — if Pagefind, plan for `pagefind` crate library integration (not CLI binary)~~ **DONE** (confirmed: TypeScript app uses pagefind_extended CLI; Rust will use pagefind crate library API)
- ~~create slug compatibility test suite comparing `deunicode` vs `transliteration` output on fixture content~~ **DONE**
- ~~create Iced architecture patterns document (message design, subscription model, custom widget patterns)~~ **DONE** (docs/ICED_ARCHITECTURE_PATTERNS.md)

### Validation

- ~~DB readability tests (all tables including `mediaTranslations`, `postLinks`, FTS5, and AI/catalog tables)~~ **DONE** (59 unit tests + 29 fixture integration tests)
- ~~app launch smoke test (Iced window + muda menus)~~ **DONE** (tests/app_smoke.rs — type-level tests; muda requires main thread so full launch tested via `cargo run`)
- ~~bds-editor PoC test: renders highlighted markdown, accepts keyboard input, cursor moves~~ **DONE** (tests/editor_poc.rs — 11 integration tests covering highlight + input + cursor + scroll)
- ~~fixture harness test~~ **DONE** (tests/fixture_readability.rs)
- ~~slug compatibility tests~~ **DONE**

## Milestone M1: Data Fidelity

### `bds-core/db`

- ~~verify schema compatibility against existing projects~~ **DONE**
- ~~add FTS5 virtual tables (`posts_fts`, `media_fts`) for in-app full-text search~~ **DONE** (multi-column schema: title/excerpt/content/tags/categories with per-language snowball stemming, 24 languages)
- ~~verify `mediaTranslations` table read/write~~ **DONE**
- ~~verify `postLinks` table read/write~~ **DONE**

### `bds-core/engine`

- ~~implement `ProjectEngine`~~ **DONE**
- ~~implement `PostEngine`~~ **DONE**
- ~~implement `MediaEngine`~~ **DONE**
- ~~implement `PostMediaEngine`~~ **DONE**
- ~~implement `TagEngine`~~ **DONE**
- ~~implement `MetaEngine`~~ **DONE**
- ~~implement metadata diff flow (posts, media, scripts, templates)~~ **DONE**
- ~~implement rebuild-from-filesystem flow~~ **DONE**

### `bds-core/util`

- ~~frontmatter parser/writer via `serde_yaml`~~ **DONE**
- ~~translation file parser/writer~~ **DONE**
- ~~sidecar parser/writer~~ **DONE**
- ~~content hashing support via `sha2`~~ **DONE**
- ~~thumbnail generation via `image` crate (resize, WEBP encoding)~~ **DONE**
- ~~recursive directory traversal via `walkdir` (for rebuild-from-filesystem)~~ **DONE**

### Validation

- ~~round-trip persistence tests~~ **DONE** (38 m1_validation integration tests)
- ~~rebuild tests~~ **DONE**
- ~~metadata diff tests~~ **DONE** (full coverage matrix: 13 post fields, 6 media, 4 template, 5 script)
- ~~golden-file comparisons for written files~~ **DONE**

## Milestone M2: Native Workspace

### `bds-ui/platform`

- ~~muda menu bar: full menu tree with accelerators~~ **DONE** (platform/menu.rs with muda MenuBar, menu_subscription, accelerators)
- ~~menu enable/disable validation synced to app state~~ **DONE** (sync_menu_state() evaluates has_project/has_tab/offline_mode after every state change)
- ~~menu event routing as `Message` variants~~ **DONE** (MenuEvent → Message mapping in app.rs subscription)
- ~~rfd integration for file/folder open and save dialogs~~ **DONE** (platform/dialog.rs with i18n-aware pick_folder/pick_media_files)
- ~~macOS lifecycle shim: file-open and URL-open events forwarded as `Message` variants~~ **DONE** (objc2 BdsAppDelegate with application:openFile: and application:openURLs: → mpsc channel → Message::FileOpenRequested/UrlOpenRequested)

### `bds-ui/app`

- ~~root `Message` enum and `update()` dispatcher~~ **DONE** (Message enum with menu, lifecycle, sidebar, tab, editor variants + update dispatcher)
- ~~app state model~~ **DONE** (AppState with project, sidebar, editor, output panel state; sidebar post list with 500-post limit)
- ~~task surface model~~ **DONE** (engine/task.rs TaskManager with concurrency limit, FIFO queue, progress reporting, cancel support)

### `bds-ui/views`

- ~~workspace layout~~ **DONE** (three-panel layout: sidebar + editor + optional right panel)
- ~~sidebar~~ **DONE** (post list with search, status filter, category/tag counts)
- ~~activity bar~~ **DONE** (VS Code-style 50px bar with SVG icons for Posts/Pages/Media/Scripts/Templates/Tags/Chat/Import/Git/Settings)
- ~~tab bar~~ **DONE** (editor tab bar with open/close/switch)
- ~~status bar~~ **DONE** (bottom bar with project name, task progress)
- ~~project selector~~ **DONE** (project open dialog and recent projects)
- ~~toast notifications~~ **DONE** (overlay toast stack with auto-dismiss, 4 severity levels, i18n messages; replaces output-only notifications)

### Validation

- ~~menu event → `Message` routing integration tests~~ **DONE** (tests/menu_routing.rs — 4 tests: i18n key prefix, all-locale coverage, action count, no duplicates)
- ~~keyboard shortcut integration tests~~ **DONE** (tests/m2_validation.rs — 15 accelerator-bound actions verified)
- ~~rfd dialog invocation tests~~ **DONE** (tests/m2_validation.rs — dialog i18n keys in all 5 locales)
- ~~fixture project open flow test~~ **DONE** (tests/project_flow.rs — 6 tests: create, switch, delete, directory, meta files)
- ~~toast notification tests~~ **DONE** (tests/m2_validation.rs — toast id monotonicity, levels, expiry, message preservation, i18n keys)
- ~~menu enable/disable rule tests~~ **DONE** (tests/m2_validation.rs — project-gated 11 items, tab-gated 4 items, offline-gated 2 items)

## Milestone M3: Authoring

### `bds-editor` (maturation)

- full cursor movement (arrows, word, line, page, home/end)
- selection (shift+movement, click-and-drag, double-click word select)
- system clipboard integration (copy/cut/paste)
- undo/redo with edit grouping
- line numbers gutter
- incremental syntax rehighlighting on edits
- IME input handling for CJK and other input methods
- configurable soft wrap
- additional syntax grammars: Liquid/HTML, Lua, YAML, JSON

### `bds-core/engine`

- expose editor-safe save APIs
- expose publish/unpublish/discard flows
- expose template validation APIs
- expose script validation APIs

### `bds-ui/views`

- sidebar post filtering: text search box, status filter, tag/category filter dropdowns, language filter, year/month selectors, date range picker — wired to existing `PostSearchFilters` / `search_posts_filtered()` in bds-core. The 500 post limit applies AFTER filtering, not before.
- dashboard
- post editor (bds-editor with markdown + YAML frontmatter highlighting)
- translation editor (bds-editor)
- media browser
- media editor
- tags view
- settings view
- templates view and editor (bds-editor with Liquid/HTML highlighting)
- scripts view and editor (bds-editor with Lua highlighting)

### `bds-ui/components`

- inputs
- modals
- reusable list rows and panels

### Validation

- entity create/edit/save flows
- template validation flow
- script validation flow
- editor widget tests: cursor movement, selection, undo/redo, clipboard, IME

## Milestone M4: Rendering Parity

### `bds-core/render`

- markdown render pipeline via `pulldown-cmark`
- Liquid render pipeline via `liquid` crate (scoped to the feature subset documented in the Liquid inventory: `if`/`elsif`/`else`, `for`, `assign`, `render`, whitespace stripping, plus filters: `default`, `escape`, `url_encode`, `append`; `.size` is property access on arrays, not a pipe filter)
- custom Liquid filter: `i18n` (translation lookup by key and language)
- custom Liquid filter: `markdown` (markdown-to-HTML with macro expansion, URL rewriting, media path canonicalization)
- built-in macro renderers: `gallery`, `youtube`, `vimeo`, `photo_archive`, `tag_cloud` (native Rust, not Lua)
- template resolution rules
- URL rewriting
- RSS/Atom feed generation via `quick-xml`
- sitemap generation via `quick-xml`
- generated file hash tracking (`generatedFileHashes` table for skip-unchanged-writes)
- Pagefind search index generation via `pagefind` crate library API (`PagefindIndex::add_html_file()` + `get_files()`) — if required for parity (determine in M0 inventory)

### `bds-core/ai`

- one-shot AI client: `reqwest` + `serde_json` against OpenAI-compatible Chat Completions endpoint
- two-endpoint configuration: online endpoint (URL, API key, model) + airplane mode endpoint (URL, model)
- AI translate post operation (title, excerpt, content to target language)
- AI translate media metadata operation (title, alt, caption to target language)
- AI image description / alt text generation operation
- AI post analysis operation (title, excerpt, slug suggestion)
- AI taxonomy analysis operation (tag, category suggestions)
- AI language detection operation
- API key storage via OS keychain (macOS Keychain, Windows DPAPI, Linux libsecret)
- airplane mode gating: block online endpoint, use airplane endpoint or show toast
- error handling: surface failures as user-visible feedback, never silent

### `bds-core/engine`

- `TemplateEngine`
- `PageRenderer` (parallelize rendering via `rayon`)
- `BlogGenerationEngine`
- `PreviewServer` (localhost HTTP via `axum` on `tokio`)
- `SearchIndexEngine` only if required for parity

### `bds-ui/views`

- internal Markdown/Preview switch backed by Wry, plus external Open in Browser command
- generation progress display
- render errors and diagnostics
- AI operation triggers in post editor (analysis, taxonomy), translation editor (translate), and media editor (alt text, translate)
- AI endpoint configuration in settings view (online + airplane mode endpoints)

### Validation

- golden generated-site comparisons
- preview route coverage
- template compatibility suite
- one-shot AI client tests (mocked endpoint: all 6 operations)
- AI endpoint configuration persistence tests
- AI airplane mode gating tests

## Milestone M5: Operate And Ship

### `bds-core/engine`

- `PublishEngine` (SSH/SCP via `ssh2`, rsync via child process)
- validation and integrity services
- filesystem watcher via `notify` (if needed for detecting external changes to open content)

### `bds-core/scripting`

- `ScriptEngine`
- `LuaRuntime` via `mlua` (Lua 5.4, vendored) — user-authored scripts only; built-in macros are native Rust in `bds-core/render`
- `LuaApi` (expose post, media, tag, and project data to Lua scripts via `mlua::UserData` trait)
- user-authored Lua macro execution at render time
- user-authored transform and utility script execution
- scripting docs generator

### `bds-ui/views`

- publish workflow screens
- publish progress and failure surfaces
- script docs access points if needed for core usability

### Validation

- publish end-to-end tests
- Lua API bridge tests
- built-in macro compatibility tests
- docs sync tests

## Milestone M6: bDS2 Spec Parity

Code-vs-spec gaps from the 2026-07-18 spec sync. Each task names its authoritative spec. Schema changes go through generated Diesel migrations, and every new metadata field must be wired into publishing, metadata-diff, and rebuild-from-filesystem together.

### `bds-core/util`

- slug uniqueness: unbounded numeric suffix `{slug}-2, -3, …` — remove the 999 cap and timestamp fallback in `util/slug.rs` (post.allium)

### `bds-core/db`

- migration: add `cache_read_tokens` and `cache_write_tokens` (nullable integers) to `ai_usage` (schema.allium)
- migration: rename `npm` → `package_ref` and align `provider_npm` naming (schema.allium)
- media FTS search: accept structured `filters` alongside the query (search.allium `SearchMediaRequested(query, filters)`)

### `bds-core/engine`

- project open: discover `data_path` from the location of `meta/project.json`; never persist it in project.json; keep a machine-local project registry under the OS app-data dir (project.allium `DiscoverProjectDataPath`, `DataPathNotPersistedInProjectJson`)
- verify public/private data placement matches project.allium (`PublicContentLivesInProjectFolder`, `PrivateArtifactsLiveInOsAppDir`): DB, embeddings index, model cache, registry, and window state in the OS app-data dir; everything user-owned in the project folder
- menu: file-only model (no DB table), `HomeAlwaysFirst` normalization, `SyncMenuFromFilesystem` flow (menu.allium)
- media: optional import metadata (title, alt, caption, author, language, tags), `ReplaceMediaFile` flow, batch-import linked-image processing (media.allium)
- metadata diff: compare only translation-specific fields for translation files; per-item repair direction; `embedding` entity type (metadata_diff.allium)
- translations: `ReopenPublishedTranslation` on edit; `TranslationFilesCarryFullMetadata` (translation.allium)
- auto-translation pipeline: `ScheduleAutoTranslation`, `AutoTranslatePost`, `AutoTranslateMediaCascade`, `FillMissingTranslations` — gated by configured AI endpoint and airplane mode, skips `doNotTranslate`, fills only missing languages (translation.allium; depends on M4 AI client)
- task manager: allow cancelling `pending` (queued) tasks, not only running ones (task.allium)

### `bds-core/render` (fold into M4)

- implement rendering.allium as the shared render-assigns contract for preview and generation; content language only, never UI locale
- expose `slugify` as a custom Liquid filter alongside `i18n` and `markdown` (rendering.allium)
- generation task structure: task group with one task per section (Site Core, Single Posts, Category/Tag/Date Archives) + final Build Search Index task; stream one progress message per written page — "Generated /url (n/total)" or "Rewrote …" for validation apply (generation.allium)
- enforce `GenerationPublishedOnly` and the specified language variant selection (generation.allium)

### `bds-core/ai` (fold into M4)

- honour ai.allium invariants in the one-shot client and settings: endpoint add/remove rules, advisory model catalog with conditional refresh, provider detection, vision capability gate, airplane-mode model swap
- record cache token usage in `ai_usage` (schema.allium)

### `bds-core/scripting` (fold into M5)

- configurable `script_extension` (default "lua"): file path `scripts/{slug}.{extension}`; entrypoint default "render" for macros, "main" otherwise (script.allium, frontmatter.allium)
- sandboxed Lua runtime: no filesystem mutation, process control, or package loading; host capabilities only via explicit `bds.*` API (script.allium `SandboxedExecution`, `ExplicitHostCapabilities`)
- enforce config limits: macro timeout 10s; transform toasts max 5/script, 20 total, 300 chars; blogmark title ≤ 200, URL ≤ 2048 (script.allium)

### `bds-ui`

- settings: technology section (semantic similarity toggle), data section (rebuild buttons: posts/media/scripts/templates/links/thumbnails/embedding + Open Data Folder), agent integrations (Claude Code + GitHub Copilot; others disabled with "not supported yet" note), image import concurrency (1–8, default 4), default editor mode, settings in a separate tab (editor_settings.allium)
- media search filter UI wired to the filtered media search
- auto-translation and repair-direction UI hooks for the engine flows above

### Validation

- unit tests for every invariant and rule named above, per AGENTS.md spec-coverage requirement
- re-run `allium check specs/*.allium` after any spec edit

## Extension Backlog

### Bucket A: Git And Validation

#### `bds-core`

- `GitEngine` via `git2` (shell out for LFS operations)
- richer site validation service support

#### `bds-ui`

- Git sidebar
- diff view
- richer metadata diff UI

### Bucket B: Import

#### `bds-core`

- WXR parser
- import analysis
- import execution
- import definitions

#### `bds-ui`

- import wizard
- import progress and result views

### Bucket C: AI Chat And Tool Use

#### `bds-core`

- streaming client extension (SSE parsing on top of core `reqwest` client)
- tool execution framework (tool definitions, call routing, result formatting)
- multi-turn conversation management

#### `bds-ui`

- chat sidebar
- chat panel
- provider settings UI (extends core AI endpoint configuration)

### Bucket D: Embeddings And Duplicates

#### `bds-core`

- embedding engine via `ort` (ONNX Runtime)
- vector index via `usearch` (HNSW)
- duplicate detection logic

#### `bds-ui`

- semantic search UI
- duplicates view

### Bucket E: Translation QA And Docs UX

#### `bds-core`

- translation validation engine

#### `bds-ui`

- translation validation view
- documentation browser

### Bucket F: Menu Editing And Deep Links

#### `bds-core`

- menu editing services
- deep-link flow support beyond core shell hooks

#### `bds-ui`

- menu editor UI

### Bucket G: MCP And Automation

#### `bds-cli`

- CLI command set per cli.allium: `rebuild | repair | render | upload | push | pull | post | media | gallery | config | project | lua` — same database, settings, and cache as the GUI; no listeners, no window; logging redirected to the rotating log file
- MCP server surface per mcp.allium (incl. translation tools, stats/media resources, proposal status)
- CLI-to-app synchronization via the domain event bus (events.allium): entity mutations broadcast `(entity, entity_id, action)` on one topic shape covering in-app and external writes

#### `bds-core`

- domain event bus (events.allium): broadcast after every successful mutation of posts, media, tags, templates, scripts; `SettingsChangedEvent` for global settings

### Bucket K: Headless Server Mode (server.allium)

#### `bds-core` / `bds-cli`

- boot modes desktop | server | tui resolved from an env var at start
- headless server: no window; SSH transport for remote TUI/GUI clients
- SSH key material generated on first boot in the private app-data dir (host key + empty authorized_keys, mode 600) — never in the repo or project folder

### Bucket L: Terminal UI (tui.allium)

#### new crate `bds-tui` (or feature in `bds-cli`)

- TUI as a second renderer over the same shared UI core: sidebar views (posts/media/templates/scripts/tags/settings/git), sidebar/editor focus model, vim-style + arrow navigation skipping section headers
- post editor drives the same workflow as the GUI: canonical-language edits update the post, other languages update translations; publish routes through the same pipeline
- subscribes to the domain event bus (Bucket G) for live updates

### Bucket H: Blogmark And Transforms

#### `bds-core`

- `BlogmarkTransformService`
- transform script chain execution

#### `bds-ui`

- external content capture workflow

### Bucket I: Rich Editor

#### `bds-editor` (evolution)

- inline bold/italic/header rendering via cosmic-text mixed font styles
- inline image preview via Iced image rendering within the custom widget
- macro block preview (render macro output inline)
- clickable links

#### `bds-ui`

- rich editor mode toggle
- image insert dialog from linked media

### Bucket J: A2UI Surfaces

#### `bds-ui`

- A2UI component renderer
- A2UI surface manager
- AI assistant dynamic surface integration

## Exit Rule

Do not start extension buckets until Milestones M5 and M6 are complete and the Rust app is already a credible replacement for the baseline authoring and publishing workflow.
