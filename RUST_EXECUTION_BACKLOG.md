# bDS Rust Rewrite — Execution Backlog

## Purpose

This document converts the core and extension plans into execution order. It is organized by milestone first and by crate second.

Rules:

1. Do not pull work forward from a later milestone unless it directly unblocks the current milestone.
2. Keep tasks vertically sliced where possible: engine + UI + tests.
3. Compatibility tasks outrank polish tasks.

## Milestone M0: Compatibility Baseline

### `bds-core`

- create workspace and crate boundaries (bds-core, bds-editor, bds-ui, bds-cli)
- add SQLite connection management via `rusqlite` (bundled, vtab features)
- add migration loader via `refinery`
- define initial shared model modules with `serde` derives
- add checksum (`sha2`) and slug (`deunicode`) utilities
- establish error handling conventions: `thiserror` for bds-core, `anyhow` for bds-ui/bds-cli
- add `tokio` runtime as workspace dependency (used by bds-ui and bds-cli, not directly by bds-core engine code)

### `bds-editor`

- create crate with ropey, syntect, cosmic-text dependencies
- implement basic rope buffer wrapper with edit operations
- implement syntect integration for markdown highlighting
- implement cosmic-text layout for rendering highlighted text
- implement basic cursor model (position, move, place by click)
- implement basic text insertion and deletion
- implement Iced custom widget that composes buffer + highlight + layout + cursor
- implement basic vertical scrolling with viewport-aware rendering
- verify IME composition events work through winit (early risk check)

### `bds-ui`

- create app entry point with Iced `Application` impl
- wire Iced window creation
- wire muda menu bar with skeleton App/File/Edit/View/Window/Help menus
- wire macOS lifecycle shim via objc2 (application:openFile:, application:openURLs:) behind cfg gate
- wire muda `MenuEvent` receiver as Iced `Subscription`

### `fixtures` and `docs`

- collect representative fixture projects from current app
- capture golden generated output for those fixtures
- create initial compatibility inventory from the matrix template (must include `mediaTranslations`, `postLinks`, FTS5 tables)
- create Liquid feature inventory from default templates (12 files in `src/main/engine/templates/`)
- determine whether current app uses Pagefind or another client-side search index — if Pagefind, plan for `pagefind` crate library integration (not CLI binary)
- create slug compatibility test suite comparing `deunicode` vs `transliteration` output on fixture content
- create Iced architecture patterns document (message design, subscription model, custom widget patterns)

### Validation

- DB readability tests (all tables including `mediaTranslations`, `postLinks`, FTS5, and AI/catalog tables)
- app launch smoke test (Iced window + muda menus)
- bds-editor PoC test: renders highlighted markdown, accepts keyboard input, cursor moves
- fixture harness test
- slug compatibility tests

## Milestone M1: Data Fidelity

### `bds-core/db`

- verify schema compatibility against existing projects
- add FTS5 virtual tables (`posts_fts`, `media_fts`) for in-app full-text search
- verify `mediaTranslations` table read/write
- verify `postLinks` table read/write

### `bds-core/engine`

- implement `ProjectEngine`
- implement `PostEngine`
- implement `MediaEngine`
- implement `PostMediaEngine`
- implement `TagEngine`
- implement `MetaEngine`
- implement metadata diff flow (posts, media, scripts, templates)
- implement rebuild-from-filesystem flow

### `bds-core/util`

- frontmatter parser/writer via `serde_yaml`
- translation file parser/writer
- sidecar parser/writer
- content hashing support via `sha2`
- thumbnail generation via `image` crate (resize, WEBP encoding)
- recursive directory traversal via `walkdir` (for rebuild-from-filesystem)

### Validation

- round-trip persistence tests
- rebuild tests
- metadata diff tests
- golden-file comparisons for written files

## Milestone M2: Native Workspace

### `bds-ui/platform`

- muda menu bar: full menu tree with accelerators
- menu enable/disable validation synced to app state
- menu event routing as `Message` variants
- rfd integration for file/folder open and save dialogs
- macOS lifecycle shim: file-open and URL-open events forwarded as `Message` variants

### `bds-ui/app`

- root `Message` enum and `update()` dispatcher
- app state model
- task surface model

### `bds-ui/views`

- workspace layout
- sidebar
- activity bar
- tab bar
- status bar
- project selector

### Validation

- menu event → `Message` routing integration tests
- keyboard shortcut integration tests
- rfd dialog invocation tests
- fixture project open flow test

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
- toast/error surfaces
- reusable list rows and panels

### Validation

- entity create/edit/save flows
- template validation flow
- script validation flow
- editor widget tests: cursor movement, selection, undo/redo, clipboard, IME

## Milestone M4: Rendering Parity

### `bds-core/render`

- markdown render pipeline via `pulldown-cmark`
- Liquid render pipeline via `liquid` crate (scoped to the feature subset documented in the Liquid inventory: `if`/`elsif`/`else`, `for`, `assign`, `render`, whitespace stripping, plus filters: `default`, `escape`, `url_encode`, `append`, `size`)
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

- preview controls
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

- headless commands
- MCP server surface
- CLI-to-app notification mechanism (`db_notifications` / `NotificationWatcher`)

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

Do not start extension buckets until Milestone M5 is complete and the Rust app is already a credible replacement for the current authoring and publishing workflow.