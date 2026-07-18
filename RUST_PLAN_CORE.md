# bDS Rust Rewrite — Core Plan

## Goal

Ship a native desktop Rust application (macOS primary, cross-platform ready) that fully replaces the baseline app (bDS2, Elixir, at `../bDS2/`) for the primary blogging workflow:

1. open an existing project
2. create and edit content
3. preview content locally
4. generate the site
5. publish the site

Core is not a toy MVP. Core must already be a production-capable replacement for the current app's content pipeline.

## Core Planning Rules

1. Critical path first. Do not start advanced parity work while the authoring and publishing path is incomplete.
2. Compatibility before optimization. Matching data and output behavior matters more than early performance work.
3. Engine and UI land together at milestone boundaries. No long-lived backend-only branches of product functionality.
4. No new persistence formats during core.
5. Every milestone ends with a runnable app, not just passing unit tests.

## Explicit Compatibility Contract

### Must stay identical

- SQLite schema semantics and data readability
- post markdown files and frontmatter layout
- translation file naming and frontmatter layout
- media sidecars and thumbnail conventions
- media translation records
- post-link tracking records
- template file formats and lookup rules
- menu document format (OPML at `meta/menu.opml`) — both read and write
- generated routes, feeds, sitemaps, and local preview behavior
- metadata JSON file layout: `meta/project.json`, `meta/categories.json`, `meta/category-meta.json`, `meta/publishing.json`
- metadata diff and rebuild-from-filesystem behavior
- full-text search behavior (SQLite FTS5 virtual tables for posts and media, with Snowball stemmers for 24 languages via custom tokenizer)
- slug generation behavior (verify `deunicode` output matches current `transliteration` output for the project's content corpus)

### May change intentionally

- app implementation language and architecture
- editor technology
- scripting runtime: Python out, Lua in
- internal process model: no Electron main/renderer split
- UI framework: Iced replaces Electron/web UI
- platform integration: muda/rfd replace Electron's built-in menu and dialog APIs

### Compatibility truth

User content and project data are compatible.
User-authored Python scripts are not compatible and are treated as legacy artifacts.

## bDS2 Spec Sync — Contract Updates (2026-07-18)

The specs in `specs/` were synced wholesale from `../bDS2/specs/`. The following contracts changed or were made explicit relative to what this plan and the existing Rust code assumed. Each item names the spec file that is authoritative; the executable gap list lives in RUST_EXECUTION_BACKLOG.md M6.

### Data placement (project.allium)

- `public_dir` = the project folder (the directory containing `meta/project.json`): all user-owned, portable, webserver-bound content — posts, media + thumbnails, templates/, scripts/, meta/, tags.json, menu.opml, and generated `html/` output.
- `private_dir` = the OS per-user app-data dir (`~/Library/Application Support/bds`, `$XDG_CONFIG_HOME/bds`, `%APPDATA%\bds`): SQLite database, per-project embeddings index, model cache, project registry, window/UI state. Never inside the repo or the project folder.
- `data_path` is **discovered** from the on-disk location of `meta/project.json` and never persisted inside project.json — projects must stay movable (invariant `DataPathNotPersistedInProjectJson`).
- Creating a project does not seed `public_dir/templates` with bundled defaults (`ProjectTemplatesDirectoryReservedForUserTemplates`).

### Behaviour deltas vs. old plan assumptions

- **Slug uniqueness** (post.allium): unbounded numeric suffix `{slug}-2, -3, …`. The old `-999` cap and timestamp fallback are gone.
- **Menu** (menu.allium): file-only model — `meta/menu.opml` is the sole store, no DB table. Normalization guarantees home is always the first item (`HomeAlwaysFirst`); `UpdateMenu` strips incoming home entries and prepends one. A `SyncMenuFromFilesystem` flow reloads it from disk.
- **Scripts** (script.allium, schema.allium): file path is `scripts/{slug}.{extension}` with configurable `script_extension` (default `"lua"`); entrypoint default is `"render"` for macros, `"main"` otherwise. Execution guarantees: sandboxed runtime (no filesystem mutation, process control, or package loading) with host capabilities exposed only through an explicit `bds.*` API. Config limits: macro timeout 10s, transform toast limits (5/script, 20 total, 300 chars), blogmark title ≤200 / URL ≤2048.
- **Schema** (schema.allium): `ai_usage` gains `cache_read_tokens` / `cache_write_tokens` (nullable); the legacy `npm` / `provider_npm` columns are the generic `package_ref` / provider package reference. Statuses are named enums (PostStatus, PostTranslationStatus, TemplateStatus, ScriptStatus).
- **Media** (media.allium): import accepts optional caller-supplied metadata (title, alt, caption, author, language, tags); a `ReplaceMediaFile` flow swaps the underlying file for an existing media record; batch import processes linked images. Media search takes `filters`, not just a query (search.allium).
- **Search** (search.allium): FTS5 tables use per-field stemmed columns (title/excerpt/content/tags/categories); translations are stemmed with their own language stemmer and appended per-field. Languages without a Snowball algorithm pass through unstemmed. (Already implemented in `bds-core/db/fts.rs`.)
- **Generation** (generation.allium): only published content generates (`GenerationPublishedOnly`); language variant selection is specified; generation/validation run as a task group — one task per section (Site Core, Single Posts, Category/Tag/Date Archives) plus a final Build Search Index task — streaming one progress message per written page.
- **Rendering** (rendering.allium — NEW, core scope): the render-assigns contract shared by preview and generation. Rendering language is the content language, never the UI locale. Custom Liquid filters are `i18n`, `markdown`, and `slugify` (slugify was not in the old plan's filter list).
- **Metadata diff** (metadata_diff.allium): translation files carry only translation-specific metadata — shared status/timestamps come from the canonical post and must not be reported missing; repair supports a direction per item; `embedding` is a diffable entity type.
- **Translations** (translation.allium): translation files carry full metadata (`TranslationFilesCarryFullMetadata`); editing a published translation reopens it to draft (`ReopenPublishedTranslation`); an auto-translation pipeline (`ScheduleAutoTranslation`, `AutoTranslatePost`, `AutoTranslateMediaCascade`, `FillMissingTranslations`) fills missing languages, skips `doNotTranslate`, and is gated by the configured AI endpoint and airplane mode.
- **AI** (ai.allium): endpoint add/remove rules, advisory model catalog with conditional refresh, provider detection, vision capability gating, chat context truncation, airplane-mode model swap. One-shot operations remain core; chat remains extension.
- **Settings UI** (editor_settings.allium): settings gains a technology section (semantic similarity toggle; scripting runtime is fixed, no switch exposed), a data section (rebuild buttons for posts/media/scripts/templates/links/thumbnails/embedding + Open Data Folder), agent integrations (Claude Code and GitHub Copilot supported; others render disabled with a "not supported yet" note), image import concurrency (1–8, default 4), and default editor mode (markdown | preview). Settings opens in a separate tab.
- **Tasks** (task.allium): `pending → cancelled` is a legal transition (cancel queued tasks, not only running ones).
- **Frontmatter** (frontmatter.allium): timestamps are Unix **milliseconds**; keys are camelCase; `doNotTranslate` only written when true, `templateSlug`/`publishedAt` only when present. (Already implemented in `bds-core/util/frontmatter.rs`.)

### New subsystems specified by bDS2 (extension scope)

- `cli.allium` — workspace CLI (`rebuild | repair | render | upload | push | pull | post | media | gallery | config | project | tui | lua`) sharing the same database and settings as the GUI, no listeners/window.
- `events.allium` — domain event bus: every entity mutation broadcasts `(entity, entity_id, action)` on the same topic/payload shape as the CLI sync watcher, so one subscription covers in-app and external changes.
- `server.allium` — headless server mode with SSH transport (boot modes desktop | server | tui via env var; SSH key material in the private app-data dir).
- `tui.allium` — terminal UI as a second renderer over the same shared UI core and post-editor workflow as the GUI.

These map to extension Buckets G (CLI + events) and K/L (server, TUI) in RUST_PLAN_EXTENSION.md.

## Architecture Decisions

- **Single process Rust app**: no Electron, no IPC boundary.
- **Iced (Elm architecture) for UI**: Message-driven update cycle. All user interactions produce `Message` variants; state mutations happen in a centralized `update()` function; views are pure functions that return element trees.
- **muda for native menus**: cross-platform native menu bar from day one. NSMenu on macOS, Win32 menus on Windows, GTK/dbus on Linux. Key equivalents and menu validation handled through muda's API.
- **rfd for file dialogs**: cross-platform native open/save/folder dialogs. NSOpenPanel/NSSavePanel on macOS, equivalents elsewhere.
- **objc2 for macOS lifecycle shim** (cfg-gated): thin bridge for `application:openFile:`, `application:openURLs:`, and other `NSApplicationDelegate` hooks that muda/rfd do not cover. ~50 lines of platform code.
- **Custom editor widget (bds-editor crate)**: syntax-highlighting markdown/Liquid/Lua editor built on ropey (rope buffer), syntect (syntax highlighting), and cosmic-text (font shaping and text layout). This is the highest-risk custom component and gets a proof-of-concept in Wave 0.
- **Diesel + embedded migrations**: typed SQLite queries and generated schema, with migrations managed by `diesel_migrations`. Backend-only SQL is confined to connection setup and FTS5 operations.
- **tokio as the async runtime**: preview server (axum), SSH publishing, file watching, and rfd async dialogs all require an async executor. tokio is the standard choice and is used workspace-wide. Synchronous engine code in bds-core does not use tokio directly — it remains callable from both async (bds-ui) and sync (bds-cli) contexts.
- **Markdown editor with live preview**: bds-editor provides syntax-highlighted Markdown editing, paired with the rendered preview required by the baseline workflow.
- **Lua for user-authored scripting**: `mlua` with Lua 5.4. Only user-authored macros, transforms, and utility scripts run through Lua. Built-in macros are native Rust — see the macro architecture section below.
- **One-shot AI operations are core scope**: six operations use a simple OpenAI-compatible HTTP client (`reqwest` + `serde_json`): (1) translate post, (2) translate media metadata, (3) image description / alt text, (4) post analysis (title + excerpt + slug suggestion), (5) taxonomy analysis (tag + category suggestions), (6) language detection. Two configurable endpoints: one for online use, one for airplane/offline mode (local model). These are fire-and-forget request/response calls — no streaming, no tool use, no chat history. The AI chat UI, streaming responses, and tool execution remain in Extension Bucket C.

## Iced Architecture Patterns

The Iced Elm architecture imposes a specific application structure:

### Message routing

All user interactions, menu events, file dialog results, platform lifecycle events, and async task completions are expressed as variants of a root `Message` enum. The application's `update()` method matches on these variants and mutates state accordingly. This replaces the "command dispatcher" pattern seen in imperative UI frameworks.

### View composition

Views are pure functions: `fn view(&self) -> Element<Message>`. They read application state and return a widget tree. Views never mutate state directly. This makes UI code inherently testable — you can assert on the element tree without rendering.

### Command and subscription model

Side effects (file I/O, network, timers, platform events) are expressed as `Command` or `Task` values returned from `update()`. Menu events from muda arrive via a `Subscription` that polls `MenuEvent::receiver()`. Platform lifecycle events from the objc2 shim arrive via a similar channel-based subscription.

### State model

The UI state must track at minimum:

- active project
- open tabs and selected tab
- selected entities
- dirty editors (with undo/redo state in bds-editor)
- task progress
- `ui_locale`
- project render settings including `content_language`
- menu state (enabled/disabled/checked items synced to app state)

## Workspace Structure

```text
bds-rust/
├── Cargo.toml
├── crates/
│   ├── bds-core/
│   │   ├── src/
│   │   │   ├── db/
│   │   │   ├── engine/
│   │   │   ├── model/
│   │   │   ├── render/
│   │   │   ├── scripting/
│   │   │   ├── i18n/
│   │   │   └── util/
│   ├── bds-editor/
│   │   ├── src/
│   │   │   ├── buffer.rs       # ropey rope wrapper, edit operations
│   │   │   ├── highlight.rs    # syntect integration, incremental rehighlight
│   │   │   ├── layout.rs       # cosmic-text buffer/shaping/layout
│   │   │   ├── cursor.rs       # cursor model, selection, multi-cursor
│   │   │   ├── history.rs      # undo/redo operation log
│   │   │   ├── input.rs        # key event → buffer mutation mapping
│   │   │   ├── widget.rs       # Iced custom widget implementation
│   │   │   └── lib.rs
│   ├── bds-ui/
│   │   ├── src/
│   │   │   ├── app.rs          # Iced Application impl, root Message enum, update()
│   │   │   ├── platform/
│   │   │   │   ├── mod.rs
│   │   │   │   ├── macos.rs    # objc2 lifecycle shim (cfg-gated)
│   │   │   │   └── menu.rs     # muda menu construction and event routing
│   │   │   ├── views/
│   │   │   ├── components/
│   │   │   └── i18n/
│   └── bds-cli/
├── migrations/
├── fixtures/
│   ├── compatibility-projects/
│   └── golden-generated-sites/
└── docs/
    └── scripting/
```

## Core Feature Scope

### Included in core

- project open/create/select
- posts and translations
- media import and metadata editing
- tags and categories
- template editing and validation
- settings and publishing preferences
- live preview and full static generation
- publish via SSH and rsync
- metadata diff and rebuild-from-filesystem
- native menus and shortcuts via muda (cross-platform)
- native file dialogs via rfd (cross-platform)
- macOS lifecycle hooks via objc2 shim (open-file, URL-open)
- syntax-highlighting editor for markdown, Liquid templates, and Lua scripts via bds-editor
- built-in macros implemented natively in Rust (gallery, youtube, vimeo, photo_archive, tag_cloud)
- Lua runtime for user-authored macros, transforms, and utility scripts
- generated Lua scripting API docs
- FTS5 virtual tables for in-app post and media search
- one-shot AI operations: translate post, translate media, image alt text, post analysis, taxonomy analysis, language detection (two configurable OpenAI-compatible endpoints: online + airplane mode, via `reqwest`)

### Deferred to extensions

- AI chat UI, streaming responses, and tool execution (Bucket C)
- Git UI and history tools
- WordPress import wizard
- embeddings and duplicate detection
- translation validation reports
- menu editor UI
- documentation browser UI
- MCP and remote automation surfaces
- Blogmark transform service (external content capture pipeline)
- A2UI server-driven UI surfaces

## Cross-Cutting Requirements

### Native platform shell

Core includes:

- native menu bar via muda (App, File, Edit, View, Window, Help menus)
- standard key equivalents and accelerators via muda
- menu enable/disable validation synced to application state
- menu actions routed as `Message` variants into the Iced update cycle
- file-open and folder-open dialogs via rfd
- macOS: open-file from Finder and URL scheme handling via objc2 lifecycle shim
- proper window lifecycle and recent-project tracking

### Split localization

Two independent localization domains are required:

- `ui_locale`: detected from the OS and used for menus, dialogs, toasts, and workspace chrome
- `content_language`: read from project settings and used for rendering, preview, feeds, sitemaps, and generation

No design in core may collapse those into a single "current language" field.

### Editor widget requirements

The bds-editor crate is a custom Iced widget that provides syntax-highlighting text editing. It must support:

- **Buffer**: ropey `Rope` for O(log n) edits and line indexing
- **Syntax highlighting**: syntect `HighlightLines` with incremental rehighlighting on edits. Grammars needed: Markdown, Liquid (HTML), Lua, YAML (frontmatter), JSON
- **Text layout**: cosmic-text `Buffer` for font shaping, glyph positioning, and line layout
- **Cursor**: position tracking, selection (shift+movement), click-to-place, click-and-drag selection
- **Input handling**: standard key bindings (arrows, home/end, page up/down, word movement with option/ctrl), text insertion, delete/backspace, tab/indent
- **Undo/redo**: operation log tracking edit groups, triggered by standard shortcuts
- **Line numbers**: gutter with line numbers, synchronized scroll
- **Scroll**: vertical and horizontal scrolling, viewport-aware rendering (only lay out visible lines)
- **Soft wrap**: configurable per editor instance
- **IME support**: proper handling of composition events from winit for CJK and other input methods — test early

The widget emits Iced `Message` variants for content changes, cursor movement, and save requests. The parent view owns the buffer state and passes it to the widget.

### Metadata parity matrix

Before any engine work begins, create a compatibility inventory for every persisted field across:

- database columns
- markdown frontmatter
- translation frontmatter
- media sidecars
- project metadata files
- template files
- generated output metadata

For each field record:

- source of truth
- persisted locations
- read path
- write path
- rebuild behavior
- metadata diff behavior
- publish behavior

This matrix is a release artifact, not a temporary note.

### Liquid template feature subset

The current default templates use a small subset of the Liquid specification. Before implementing the Liquid render pipeline, inventory the exact features used. The current templates (12 files: 3 main templates, 5 macro templates, 4 partials in `src/main/engine/templates/`) use only:

- **Tags:** `if`/`elsif`/`else`, `for`, `assign`, `render` (partials), whitespace stripping (`{%- -%}`)
- **Filters (built-in):** `default`, `escape`, `url_encode`, `append`
- **Filters (custom, must be re-implemented in Rust):** `i18n` (translation lookup by key and language), `markdown` (markdown-to-HTML with macro expansion, URL rewriting, and media path canonicalization)
- **Operators and access:** `==`, `>`, `or`, `and`, bare truthiness, `blank` (nil/empty). Property access via dot notation (`object.property`), `.size` on arrays (this is property access, **not** a pipe filter), bracket notation for map lookups (`map[key]`).
- **Not used:** `unless`, `case/when`, `capture`, `layout`, `include`, `comment`, `raw`, `date`, `truncate`, `split`, `join`, `where`, `group_by`, `map`, `sort`, `reverse`, `cycle`, `tablerow`, `increment`/`decrement`, `size` (as a pipe filter), and most other standard filters

This means the Rust Liquid implementation only needs roughly 35% of the full specification. Use `liquid-rust` or a minimal custom engine scoped to just the features above. User templates may use additional features, but parity with the current default templates is the release gate.

### Built-in macro architecture

The current app has two distinct macro systems that must not be conflated:

1. **Built-in macros** (`gallery`, `youtube`, `vimeo`, `photo_archive`, `tag_cloud`) — implemented in the host language of the baseline app, rendered server-side during generation, each with a corresponding `.liquid` macro template. These are **not** user scripts and never were.
2. **User-authored script macros** — Lua in bDS2 and in the Rust app (the old TypeScript app used Python/Pyodide; those scripts are legacy artifacts). See script.allium for the sandbox and `bds.*` host API contract.

In the Rust app:

- Built-in macros become **native Rust functions** in `bds-core/render`. They are part of Wave 4 (rendering parity), not Wave 6 (Lua scripting). Rendering cannot produce correct output without them.
- User-authored macros run through the **Lua runtime** (`mlua`). This is Wave 6 scope. During Wave 4, unknown macro placeholders should produce a visible "unsupported macro" marker rather than silently dropping content.

Macro invocation syntax in content: `[[macro_name param1="value1" param2="value2"]]`

### Slug generation compatibility

The Rust app uses `deunicode` for Unicode-to-ASCII conversion in slugs; it must match the established bDS transliteration behaviour (especially German umlauts). The slug compatibility test suite in M0 fixtures covers this. Uniqueness per post.allium: try the base slug, then `{slug}-2`, `{slug}-3`, … with an **unbounded** numeric suffix (no 999 cap, no timestamp fallback).

### Two search systems

The app has two independent search systems — do not conflate them:

1. **FTS5 (in-app search)**: SQLite FTS5 virtual tables (`posts_fts`, `media_fts`) power the desktop app's search UI. Text is Snowball-stemmed in application code before indexing and querying. FTS5 virtual-table and `MATCH` operations form the explicitly isolated raw-SQL backend boundary; ordinary filtering uses Diesel's typed query builder. This is Wave 1 scope.

2. **Pagefind (generated site search)**: a client-side search index bundled with the generated static site. Pagefind indexes the generated HTML files and produces JavaScript/WASM artifacts that power search on the published website. This is Wave 4 scope, added to the generation pipeline via the `pagefind` crate's Rust library API.

### Client-side search index (Pagefind)

The baseline app generates a Pagefind search index as part of site generation, and the Rust app must produce the same artifact. Pagefind publishes a Rust library (`pagefind` crate) with a programmatic API (`pagefind::api::PagefindIndex`). The generation pipeline feeds rendered HTML to `PagefindIndex::add_html_file()` and writes the resulting index files via `PagefindIndex::get_files()`. No external binary, no npm — this is a pure Rust library dependency, fully compatible with the no-JavaScript constraint.

Determine during the compatibility inventory (M0) whether Pagefind or another client-side search solution is used. If Pagefind: add it to the generation pipeline in Wave 4 via the `pagefind` crate. If a different solution: document it and plan accordingly.

### Image processing

Media import requires thumbnail generation and potentially format conversion (e.g., to WEBP). The Rust choice is the `image` crate for decoding/encoding and basic transforms (resize, crop). If WEBP encoding performance or advanced processing (EXIF handling, ICC profiles) proves insufficient with `image` alone, `libvips` Rust bindings (`libvips-rs`) are the fallback. Start with `image` — it covers the common cases and has no system dependencies.

## Planned Crate Registry

All crate choices for core scope, organized by subsystem. This prevents ad-hoc technology decisions during implementation.

### Foundation (Wave 0 onward)

| Crate | Purpose | Notes |
|---|---|---|
| `diesel` (sqlite) | Typed SQLite database access | Query builder and generated schema |
| `diesel_migrations` | Embedded migration management | Runs generated Diesel migrations at startup |
| `libsqlite3-sys` (bundled) | SQLite native library | Compiles SQLite from source with FTS5 |
| `uuid` (v4) | Entity identifiers | |
| `serde` + `serde_json` | Serialization/deserialization | Used everywhere |
| `serde_yaml` | YAML frontmatter parsing/writing | Posts, translations, media sidecars |
| `chrono` | Date/time handling | |
| `sha2` | Content hashing, checksums | |
| `deunicode` | Unicode-to-ASCII for slug generation | Verify against `transliteration` output |
| `thiserror` | Typed error definitions in library crates | bds-core, bds-editor |
| `anyhow` | Ergonomic error handling in application crates | bds-ui, bds-cli |
| `tokio` | Async runtime | Preview server, publish, file watching, async dialogs |
| `rust-stemmers` | Snowball stemming for FTS5 | Custom FTS5 tokenizer for 24-language stemmed search |

### UI Framework (Wave 0 onward)

| Crate | Purpose | Notes |
|---|---|---|
| `iced` (wgpu, advanced, image) | Application framework | Elm architecture, GPU-accelerated via wgpu/Metal |
| `muda` | Cross-platform native menu bar | NSMenu / Win32 / GTK |
| `rfd` | Cross-platform native file dialogs | NSOpenPanel / Win32 / GTK |

### Editor Widget (Wave 0 onward, bds-editor crate)

| Crate | Purpose | Notes |
|---|---|---|
| `ropey` | Rope buffer for text storage | O(log n) edits, line indexing |
| `syntect` | Syntax highlighting | Sublime Text grammars: Markdown, Liquid, Lua, YAML, JSON |
| `cosmic-text` | Font shaping, text layout | Same engine as cosmic-DE |

### Platform Lifecycle (Wave 0 onward, macOS cfg-gated)

| Crate | Purpose | Notes |
|---|---|---|
| `objc2` | Objective-C runtime bindings | |
| `objc2-foundation` | Foundation framework types | |
| `objc2-app-kit` | AppKit framework (NSApplicationDelegate hooks) | ~50 lines of shim code |

### Data Layer (Wave 1)

| Crate | Purpose | Notes |
|---|---|---|
| `serde_yaml` | Frontmatter read/write | Already listed in foundation |
| `notify` | Filesystem watching | Detect external file changes affecting open editors |
| `image` | Thumbnail generation, format conversion | Start here; libvips-rs as fallback if needed |
| `walkdir` | Recursive directory traversal | Rebuild-from-filesystem, media import |

### Rendering and Generation (Wave 4)

| Crate | Purpose | Notes |
|---|---|---|
| `pulldown-cmark` | Markdown → HTML | Fast, CommonMark-compliant |
| `liquid` | Liquid template rendering | Scoped to the ~35% feature subset actually used |
| `quick-xml` | RSS/Atom feeds, sitemaps | Also handles OPML menu documents |
| `rayon` | Parallel site generation | Parallelize page rendering across CPU cores |
| `axum` | Preview HTTP server | Lightweight, tokio-based |

### Publishing (Wave 5)

| Crate | Purpose | Notes |
|---|---|---|
| `ssh2` | SSH/SCP file upload | For publish-via-SSH targets. Authentication via SSH agent (`SSH_AUTH_SOCK`) only — no password auth, no interactive prompts. |
| (shell out) | rsync invocation | rsync ships with macOS/Linux; invoke as child process |

### Client-Side Search (Wave 4)

| Crate | Purpose | Notes |
|---|---|---|
| `pagefind` | Search index generation | Rust library API: `PagefindIndex::add_html_file()` + `get_files()`. No CLI binary needed. |

### AI — One-Shot Operations (Wave 4–5)

| Crate | Purpose | Notes |
|---|---|---|
| `reqwest` | HTTP client for AI endpoints | OpenAI-compatible Chat Completions API. Two endpoints: online + airplane mode (local). Used for translation, alt text, taxonomy, post analysis, language detection. |

### Scripting (Wave 6)

| Crate | Purpose | Notes |
|---|---|---|
| `mlua` (lua54) | Lua 5.4 embedding | User-authored macros, transforms, utility scripts. Use `vendored` feature to compile Lua from source. |

### Extension-Only Crates (not used in core)

| Crate | Purpose | Bucket |
|---|---|---|
| `git2` | Git operations | A (Git + Validation) |
| `ort` | ONNX inference for embeddings | D (Embeddings) |
| `usearch` | HNSW vector index | D (Embeddings) |

## Critical Path

The hard sequence is:

1. compatibility inventory and fixtures
2. exact-read and exact-write data layer
3. native platform shell and command system (muda menus, rfd dialogs, Iced message routing)
4. editor widget MVP (ropey + syntect + cosmic-text proof of concept)
5. editors for posts, media, templates, scripts, and settings
6. preview and generation parity
7. one-shot AI operations (translate post, translate media, image alt text, post analysis, taxonomy analysis, language detection)
8. publish and integrity tooling
9. Lua built-ins plus generated script API docs

Anything outside that path is noise until the previous step is stable.

## Core Milestones

### Milestone M0: Compatibility Baseline

- fixture projects checked in
- golden harness working
- schema readable
- native empty app launches with Iced window and muda menu bar
- bds-editor proof-of-concept renders a markdown file with syntax highlighting

### Milestone M1: Data Fidelity

- posts, translations, media, tags, and settings round-trip
- rebuild works
- metadata diff works
- no format drift on fixture writes

### Milestone M2: Native Workspace

- projects open in a real app window
- native menus and shortcuts work via muda
- sidebar, tabs, and message routing work
- file dialogs work via rfd

### Milestone M3: Authoring

- post, translation, media, template, and script editing works using bds-editor
- errors surface in the UI
- all required authoring entities are reachable from the workspace

### Milestone M4: Rendering Parity

- preview is trustworthy
- generation matches golden output
- route, feed, and sitemap parity is acceptable
- one-shot AI operations work (all 6 operations) with two configurable OpenAI-compatible endpoints (online + airplane mode)

### Milestone M5: Operate And Ship

- publish works end to end
- integrity checks surface actionable issues
- Lua built-ins and scripting docs are complete enough for users

Core release happens only after M5.

## Wave 0: Foundation And Compatibility Inventory

**Goal:** Bootstrapped workspace, exact compatibility target defined, golden fixtures checked in, and editor widget risk retired.

### Deliverables

- Cargo workspace for `bds-core`, `bds-editor`, `bds-ui`, `bds-cli`
- SQLite connection layer, migrations loader, and base models
- compatibility inventory document covering all persisted fields (including `mediaTranslations`, `postLinks`, and FTS5 virtual tables)
- Liquid feature inventory documenting exactly which tags, filters, and patterns the default templates use
- slug compatibility test suite comparing `deunicode` output against `transliteration` output for fixture content
- fixture projects copied from the current app
- golden-file harness for file writes and generation output comparisons
- empty native app window: Iced window with muda-driven menu bar (App/File/Edit/View/Window/Help)
- **bds-editor proof-of-concept**: custom Iced widget rendering a markdown file with syntax highlighting via ropey + syntect + cosmic-text. Must demonstrate: text display with highlighting, cursor placement, text insertion, basic scrolling. This retires the highest-risk component early.
- Iced architecture patterns document covering: message design conventions, subscription model for menu events and platform hooks, custom widget patterns for bds-editor

### Dependencies

```toml
# Core
diesel = { version = "2.3", features = ["sqlite", "returning_clauses_for_sqlite_3_35"] }
diesel_migrations = "2.3"
libsqlite3-sys = { version = "0.37", features = ["bundled"] }
uuid = { version = "1", features = ["v4"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
serde_yaml = "0.9"
chrono = "0.4"
sha2 = "0.10"
deunicode = "1"
thiserror = "2"
anyhow = "1"
tokio = { version = "1", features = ["full"] }
walkdir = "2"

# UI framework
iced = { version = "0.13", features = ["wgpu", "advanced", "image"] }

# Editor widget
cosmic-text = "0.12"
ropey = "1"
syntect = "5"

# Platform integration (cross-platform)
muda = "0.15"
rfd = "0.15"

# Platform lifecycle (macOS only)
[target.'cfg(target_os = "macos")'.dependencies]
objc2 = "0.6"
objc2-foundation = "0.3"
objc2-app-kit = "0.3"
```

Not all of these are needed in Wave 0 — this is the full foundation set. Wave-specific additions (image, pulldown-cmark, liquid, axum, rayon, quick-xml, pagefind, reqwest, ssh2, mlua, notify) are added when their wave begins. See the Planned Crate Registry for the complete list.

### Tests

- open real bDS databases and verify read access across all tables (including `mediaTranslations`, `postLinks`, FTS5 tables, and AI/catalog tables that must not cause errors even though they are not used in core)
- verify empty Rust app opens as a native window with working menu bar
- verify bds-editor PoC renders highlighted text and accepts keyboard input
- verify fixture loader and golden harness run in CI
- slug compatibility tests pass for all fixture content

### Done when

- workspace builds on macOS (and ideally on Linux for CI verification)
- fixture projects load
- compatibility corpus exists
- Liquid feature inventory is complete
- bds-editor PoC works
- Iced patterns doc is available
- native app shell with menus exists
- milestone M0 acceptance review passes

## Wave 1: Core Data Layer And Filesystem Contract

**Goal:** Exact-read and exact-write support for the current project data model.

### Key crates introduced

- `serde_yaml` for frontmatter parsing/writing
- `image` for thumbnail generation and format conversion during media import
- `walkdir` for recursive directory traversal during rebuild-from-filesystem
- `notify` for filesystem change detection (may be deferred to Wave 5 if not needed until publish)

### Engines

- `ProjectEngine`
- `PostEngine`
- `MediaEngine`
- `PostMediaEngine`
- `TagEngine`
- `MetaEngine`
- `TaskManager` — max 3 concurrent tasks, FIFO queue, 250ms progress throttle, cancellation support
- `MetadataDiffEngine`
- `RebuildEngine` or equivalent rebuild services

### File and DB responsibilities

- draft vs published post lifecycle
- translation files with canonical linkage
- media sidecars and thumbnails (thumbnail generation via `image` crate)
- tag/category sync
- checksum and content-hash tracking
- rebuild database from filesystem
- metadata diff against filesystem

### Required behavior

- published posts continue storing body content in files, not in DB
- every field persisted by the current app remains persisted in the Rust app in the same places
- metadata writes, metadata diff, rebuild, and publish all use the same field mapping

### Tests

- round-trip tests for posts, translations, media, templates, and project metadata
- rebuild tests using fixture projects
- metadata diff tests proving no false negatives for known field changes
- file-by-file golden comparisons against baseline output

### Done when

- Rust can read and write existing project data without format drift
- rebuild restores the DB from files accurately
- metadata diff catches every field covered by the matrix
- milestone M1 acceptance review passes

## Wave 2: Native Platform Workspace Shell

**Goal:** Native-feeling desktop application shell with real menus and command plumbing.

### UI scope

- workspace window (Iced application)
- activity bar
- sidebar
- tab bar
- status bar
- task/output panel
- project selector
- message routing layer (root `Message` enum with command dispatch in `update()`)

### Platform scope

- App, File, Edit, View, Window, Help menus via muda
- accelerators and key equivalents via muda
- menu enable/disable synced to app state (e.g., Save disabled when no dirty editor)
- menu events received via `Subscription` polling `MenuEvent::receiver()`
- file/folder dialogs via rfd
- macOS: open-file from Finder and URL-open handled by objc2 lifecycle shim, forwarded as `Message` variants
- recent project tracking (app-managed list; no platform API dependency)

### State model

The Iced application state must track at minimum:

- active project
- open tabs
- selected entities
- dirty editors
- task progress
- `ui_locale`
- project render settings including `content_language`

### Tests

- tab lifecycle
- message dispatch from both menu events and keyboard entry points
- app-shell integration tests for opening a fixture project
- rfd dialog invocation tests (mocked where needed)

### Done when

- the app can be navigated by menu or keyboard alone
- native menu labels are localized from the UI locale
- shell behavior feels like a native desktop app
- milestone M2 acceptance review passes

## Wave 3: Content Editing UI

**Goal:** Full editing UI for the content types required by the publishing workflow.

### Views

- dashboard
- post editor (using bds-editor with markdown + YAML frontmatter highlighting)
- translation editor (using bds-editor)
- media browser and media editor
- tags view
- settings view
- templates view and template editor (using bds-editor with Liquid/HTML highlighting)
- scripts view and script editor (using bds-editor with Lua highlighting)
- lightweight linked-media and post-links views where needed for parity

### Required capabilities

- create, edit, duplicate, publish, unpublish, discard, and delete posts
- edit title, slug, excerpt, tags, categories, language, author, template assignment
- import media (via rfd file dialog) and edit title, alt, caption, author, tags, language
- create and edit templates with syntax validation
- create and edit Lua scripts with syntax validation
- expose errors and conflicts through dialogs and task panel output
- undo/redo in all editor instances via bds-editor history

### Editor widget maturation

By Wave 3, bds-editor must support the full feature set documented in the editor widget requirements section:

- all cursor movement patterns (arrows, word, line, page, home/end)
- selection (shift+movement, click-and-drag, double-click word select)
- copy/cut/paste via system clipboard
- undo/redo with edit grouping
- line numbers gutter
- incremental syntax rehighlighting on edits
- IME input for non-Latin scripts
- configurable soft wrap

### Tests

- integration tests from sidebar selection to persisted save
- template edit and validation tests
- script save and validation tests
- editor widget tests for cursor movement, selection, undo/redo, and IME composition

### Done when

- every content type needed by preview/generation/publish has an editor
- template management is reachable and usable from the UI
- scripts are manageable from the UI even if advanced tooling is deferred
- bds-editor handles real-world editing scenarios reliably
- milestone M3 acceptance review passes

## Wave 4: Rendering, Preview, And Generation

**Goal:** Reproduce the current site's rendering pipeline and local preview behavior.

### Engines

- `TemplateEngine`
- `PageRenderer`
- `BlogGenerationEngine`
- `PreviewServer` (axum, localhost-only HTTP server)
- `SearchIndexEngine` if required for parity of generated sites

### Key crates introduced

- `pulldown-cmark` for Markdown → HTML
- `liquid` for Liquid template rendering (scoped to the feature subset)
- `quick-xml` for RSS/Atom feeds, sitemaps, and OPML menu document reading
- `rayon` for parallel page rendering during generation
- `axum` for the preview HTTP server (runs on tokio)
- `pagefind` for client-side search index generation (Rust library API, not CLI binary)
- `reqwest` for one-shot AI operations (all 6 operations: translate post/media, image alt text, post analysis, taxonomy analysis, language detection) against two configurable OpenAI-compatible endpoints (online + airplane mode)

### Rendering pipeline

1. load published posts, translations, menus, templates, and project settings
2. resolve templates using the same lookup rules as the current app
3. render markdown to HTML via `pulldown-cmark`
4. expand built-in macros (gallery, youtube, vimeo, photo_archive, tag_cloud) natively in Rust — these do not go through the Lua runtime
5. for user-authored Lua macros: delegate to the Lua runtime if available, otherwise emit an "unsupported macro" placeholder
6. apply Liquid templates with custom `i18n` and `markdown` filters
7. rewrite URLs and media references
8. generate archives, feeds (via `quick-xml`), sitemaps, and search artifacts required for parity
9. feed rendered HTML pages to Pagefind via `pagefind::api::PagefindIndex::add_html_file()`, then write the search index files via `get_files()` — if client-side search index is required for parity
10. write only changed outputs when safe to do so (track output file content hashes in a `generatedFileHashes` table, matching the current app's skip-unchanged-writes behavior)
11. parallelize page rendering via `rayon` where safe (each page render is independent)

### Preview rules

- preview server binds to `127.0.0.1:4123` (localhost only, fixed port)
- preview and generated HTML use local assets only
- preview must support drafts and language-prefixed routes
- each post editor can switch between Markdown and an internal Wry preview panel backed by the draft preview route
- Open in Browser starts or reuses the same preview server and opens its URL in the external system browser; it does not replace the internal panel
- rendered language is controlled by project settings, not by UI locale

### One-shot AI operations

Wave 4 introduces the one-shot AI client in `bds-core`. This is a minimal `reqwest`-based HTTP client that sends single request/response calls to an OpenAI-compatible Chat Completions endpoint. No streaming, no tool use, no chat history.

**Operations (all 6 are core scope):**

- **Translate post**: translate title, excerpt, and content to a target language. Used from the translation editor.
- **Translate media metadata**: translate title, alt, and caption to a target language. Used from the media editor.
- **Image description / alt text**: generate alt text and caption for a media item. Used from the media editor.
- **Post analysis**: suggest title, excerpt, and slug from post content. Used from the post editor.
- **Taxonomy analysis**: suggest tags and categories for a post. Used from the post editor.
- **Language detection**: detect the language of a text fragment. Used internally by other operations.

**Configuration:**

- **Online endpoint**: URL, API key, model name — for cloud AI providers (OpenAI, Anthropic-via-proxy, etc.)
- **Airplane mode endpoint**: URL, model name — for local models (Ollama, LM Studio, etc.) — no API key needed
- When airplane mode is active, only the airplane mode endpoint is used. Cloud endpoint calls are blocked.
- Default: no endpoints configured — AI features are opt-in.
- API keys stored securely via OS keychain (macOS Keychain, Windows DPAPI, Linux libsecret), never in project files.

**Constraints:**

- AI operations are entirely optional. The app is fully functional without any AI endpoint configured.
- When no endpoint is configured, AI-related UI elements are disabled or hidden.
- When airplane mode is active, the online endpoint is disabled. If no airplane mode endpoint is configured, AI is unavailable with a user-visible toast.
- Error responses produce user-visible feedback (toast or inline error), never silent failures.
- Request/response payloads use the OpenAI Chat Completions wire format.

### Tests

- fixture-based generation comparisons against current app output
- preview route tests for posts, drafts, assets, media, and language routes
- UI tests for switching a post between Markdown and internal preview, plus the external Open in Browser command
- template compatibility tests using current `.liquid` templates

### Done when

- the same project can be generated by both apps with matching output
- internal and external preview are accurate enough to be trusted as authoring tools
- milestone M4 acceptance review passes

## Wave 5: Publishing And Operational Integrity

**Goal:** Publishing and integrity tooling needed to operate the app in production.

### Key crates introduced

- `ssh2` for SSH/SCP file upload
- rsync invoked as child process (ships with macOS/Linux)

### Engines and services

- `PublishEngine`
- `SiteValidationDiffService` or equivalent core validation service
- `NotificationWatcher` if needed for filesystem coherence in core workflows

### Required capabilities

- upload generated site via SCP or rsync
- upload media and thumbnails correctly
- SSH authentication via SSH agent (`SSH_AUTH_SOCK`) only — no password auth, no interactive prompts
- three upload targets (html, thumbnails, media) run as parallel tasks
- exclude `.meta` sidecar files from media uploads
- surface publish progress and failures in the UI
- detect and surface external file changes that affect open editors or preview accuracy

### Tests

- publish tests against mocked remote targets
- validation tests on generated sites
- watcher tests for externally modified content files

### Done when

- publishing works end to end from the Rust app
- operator-visible integrity issues are surfaced before or during publish
- milestone M5 operational review is unblocked pending Wave 6 completion

## Wave 6: Lua Runtime And Scripting Docs

**Goal:** Deliver user-authored scripting via Lua and complete scripting documentation.

Built-in macros (gallery, youtube, vimeo, photo_archive, tag_cloud) are already implemented as native Rust in Wave 4. Wave 6 covers only the user-extensible scripting layer.

### Engines

- `ScriptEngine`
- `LuaRuntime`
- `LuaApi`

### Required scope

- user-authored Lua macros invoked at render time via `[[macro_name]]` syntax
- user-authored transforms (post processing pipelines)
- user-authored utility scripts
- Lua API bridge exposing post, media, tag, and project data to scripts

### Documentation requirements

- generated Lua API documentation from source annotations or schema definitions
- canonical data structure reference for script-visible types
- examples for macro, transform, and utility scripts
- docs bundled with the app and exported as markdown files in `docs/scripting/`

### Tests

- Lua execution tests
- API bridge tests
- macro compatibility tests for built-in macros
- docs sync tests proving generated docs match exposed API

### Done when

- Lua scripting covers the built-ins needed by current templates and render flows
- scripting docs are complete enough for third-party script authors
- milestone M5 acceptance review passes

## Core Dependency Graph

```text
Wave 0 Foundation + Editor PoC
  ↓
Wave 1 Data + Filesystem Contract
  ↓
Wave 2 Native Platform Shell (Iced + muda + rfd)
  ↓
Wave 3 Content Editing UI (bds-editor maturation)
  ↓
Wave 4 Rendering + Preview + Generation
  ↓
Wave 5 Publishing + Operational Integrity
  ↓
Wave 6 Lua Runtime + Scripting Docs
```

Wave 4 depends on built-in macros (native Rust, not Lua). Wave 6 can start earlier for Lua runtime bootstrapping, but core release is not complete until user-authored scripting and docs are done.

## Core Release Checklist

Core ships only when all of the following are true:

1. An existing bDS project opens without manual migration.
2. Posts, translations, media, templates, and settings can be edited in-app.
3. Preview and generation match the current app closely enough to pass golden tests.
4. Publishing works against supported remote targets.
5. Metadata diff and rebuild are accurate.
6. The app is a native desktop app with native menus and shortcuts.
7. Lua replaces Python and has proper generated API documentation.

## Cross-Platform Notes

The core stack (Iced + muda + rfd) is cross-platform. The only platform-specific code is the macOS lifecycle shim in `bds-ui/src/platform/macos.rs`. To ship on Linux or Windows:

- Linux: no lifecycle shim needed (file open arrives as CLI args; URL handling via XDG). Iced, muda, and rfd work natively.
- Windows: similar lifecycle shim for file associations and URL protocol handling via Windows APIs. Iced, muda, and rfd work natively.

Cross-platform packaging is not core scope, but the architecture does not accumulate macOS-only technical debt that would block it.

## Supporting Docs

- [RUST_EXECUTION_BACKLOG.md](RUST_EXECUTION_BACKLOG.md)
- [RUST_COMPATIBILITY_MATRIX_TEMPLATE.md](RUST_COMPATIBILITY_MATRIX_TEMPLATE.md)
