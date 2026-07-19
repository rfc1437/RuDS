# RuDS Extension Plan

## Goal

Extensions add migration, collaboration, automation, alternate clients, and advanced discovery on top of the core blogging workflow. They must reuse core engines and formats, remain reachable through real UI or automation surfaces, and preserve airplane-mode and localization rules.

Status describes the current source code as of 2026-07-19. Core one-shot AI, publishing, rendering, and integrity work is tracked in [RUST_PLAN_CORE.md](RUST_PLAN_CORE.md).

## Current Extension Status

### Git and richer validation — Complete

Done:

- Site, media, translation, and metadata-diff engines and UI views.
- `GitEngine` repository initialization and inspection, Git LFS image tracking, status, staged/unstaged and per-file/commit diffs, branch and rename-following file history, remote state, commit, fetch, fast-forward-only pull, push, cancellation, timeouts, and platform-specific authentication guidance.
- Localized Git sidebar, log panel, read-only inline/side-by-side diff tabs, live task output, airplane-mode gating, and post-pull reconciliation through the normal filesystem rebuild paths and typed events.

### Synchronization scripting bridge — Open

Open:

- Expose `bds.sync` through the scripting API using the shared Git workflow.

### WordPress import — Complete

Done:

- Streaming, untrusted-input-safe WXR parsing for site metadata, posts, pages, attachments, categories, and tags.
- HTML5-to-Markdown conversion, WordPress shortcode conversion, complete new/update/conflict/duplicate/missing classification, date and macro analysis, and saved project-scoped import definitions.
- Localized native import sidebar/editor with WXR and uploads pickers, cached analysis reopening, conflict resolution, manual and airplane-gated AI taxonomy mapping, item review, live progress/ETA, and execution results.
- Taxonomy/posts/media/pages execution through core persistence engines in recoverable 500-item batches, including filesystem rollback, source metadata/status/timestamps, unique-slug import, overwrite/ignore behavior, and media-parent links.

### Conversational AI and agent tools — Complete

Done:

- Persistent conversation/message repositories with rename, reopen, deletion, model and provider-session selection, and four-way token accounting.
- OpenAI-compatible SSE streaming with split-frame content/tool assembly, provider-error handling, independent cancellation, bounded tool rounds, and context truncation that preserves system messages and tool pairs.
- Project-aware tools over statistics, FTS search, posts, media, templates, scripts, tags/categories, metadata mutation through shared engines, and allowlisted workspace navigation.
- Localized Chat sidebar/editor with conversation and model controls, safe GFM text rendering with blocked external images, streaming/tool state, multiline send/stop controls, and status-bar token totals.
- Fixed native A2UI cards, charts, forms, lists, metrics, mind maps, tables, and tabs with safe nested fallbacks, persistent stable-ID interaction state, debounced form values, and strictly allowlisted application actions.
- Online/airplane endpoint routing uses the shared secure endpoint and model infrastructure; unavailable modes direct the user to the existing localized AI settings.

### Embeddings, semantic search, and duplicates — Done

- Lazy, locally cached `multilingual-e5-small` inference with 384-dimensional mean-pooled, normalized vectors and native Core ML acceleration on macOS or DirectML on Windows.
- Project-isolated USearch HNSW indexes with SQLite BLOB vectors as the recovery source, debounced persistence, lifecycle updates, rebuild/backfill, and CLI repair.
- Semantic sidebar search, link ranking, editor tag suggestions, and the bDS2-compatible `bds.embeddings` Lua API.
- Localized duplicate review with exact-match detection, 500-pair pagination, canonical single/batch dismissal, and post navigation.
- Embedding-aware filesystem rebuild and metadata diff/repair, settings gating, project-switch/shutdown flushes, and native menu commands.

### Translation QA and documentation UX — Done

Done:

- Translation validation engine and report view.
- Bundled global `DOCUMENTATION.md` browser with localized loading, empty, missing, and malformed states plus explicit and file-change refresh.
- Generated Lua API, type, and executable-example browser sourced directly from `docs/scripting/` with embedded packaged fallbacks.
- Safe native GFM rendering for headings, links, code blocks, lists, and tables, including in-document anchors and confirmed external links without HTML, CSS, or JavaScript execution.
- Project switches preserve the singleton global documentation tabs and their loaded content.

### Menu editor and deep links — Done

Done:

- Menu file parsing/rendering and Home-first normalization.
- Localized OPML tree editor with page/submenu/category drafts, metadata-backed new categories, sibling moves, indent/unindent, protected Home, validated drag/drop, delayed submenu expansion, save/reload, and accessible equivalent controls.
- Canonical bDS2 OPML attributes and recursive normalization feed the same renderer used by preview and generation.
- macOS URL plumbing and the sole bDS2-compatible Blogmark action at `ruds://new-post`; RuDS neither registers nor accepts bDS2's `bds2://` scheme.

### CLI, domain events, and MCP — Done

Done:

- Domain event bus from `events.allium` for desktop, CLI, TUI, server, and future remote clients, including deterministic subscriptions, project scope, and persisted CLI notification consumption/pruning.
- Native `bds-cli` with Clap help/error handling, optional JSON output, shared application paths/database/projects/settings, full and incremental rebuild, derived-data repair, full/targeted/forced generation, publishing, fast-forward Git sync, post/media/gallery creation, offline/local AI routing and translation, project/config operations, sandboxed utility Lua execution, and guarded launcher installation.
- CLI process and dispatch tests use temporary databases/projects; CLI mutations persist deduplicated desktop notifications and imported filesystem metadata survives rebuild.
- Settings → Data exposes the same localized packaged-launcher installer as `bds-cli install`.
- MCP exposes the complete `bds://` resource set and typed read/search/count tools through packaged stdio and stateless localhost-only HTTP transports with Origin/Host validation and CORS.
- Every MCP write is an inert persisted proposal until one desktop approval applies it exactly once through the shared post/media/script/template engines; rejection, expiry, concurrent resolution, result state, and normal domain events are covered.
- Settings → MCP provides localized server enablement/status/endpoint, full proposal review controls, and opt-in guarded Claude Code and GitHub Copilot configuration without secrets.

### Blogmark and transform pipeline — Done

Done:

- Blogmark bookmarklet copy, `ruds://new-post` parsing, content capture, post import, transform selection, and Lua transform execution.
- Project-scoped `bds.*` capabilities, managed task progress, and operator cancellation during transform execution.
- bDS2-compatible delivery behavior without adding unsupported deep-link actions.

### Headless server — Complete

Done:

- Desktop/server/TUI boot selection occurs before desktop initialization; the dedicated `bds-server` binary never depends on or starts the native UI.
- The headless host reuses the application database, project registry, `CoreHost` service operations, task manager, MCP loopback endpoint, ordered domain-event bus, and CLI-notification watcher.
- A loopback-by-default Russh daemon generates and reuses a restrictive RSA host key, validates private directory/file modes, reloads `authorized_keys` for every authentication, and supports explicit external binding only.
- Versioned native remote-project sessions provide protocol negotiation, server locale, project selection, shared engine calls, replay-safe request IDs, ordered domain events, task snapshots, reconnect, concurrent clients, and graceful shutdown.
- The desktop uses a restrictive generated Ed25519 identity and TOFU `known_hosts`, with localized File-menu connect/project selection/open/failure/disconnect states. SSH shell channels host server-side terminal sessions and direct forwarding is restricted to the server-owned loopback endpoint.

### Terminal UI — Complete

Done:

- One Ratatui state/renderer runs locally through the CLI and in authenticated SSH PTYs; resize, disconnect, reconnect, clean exit, and terminal restoration are covered by a real loopback PTY test.
- Numbered sidebar views, per-view filtering, project switching/path completion, command overlays, Markdown preview, syntax-highlighted soft-wrapped Markdown/Liquid/Lua editing, validation, save/publish/unpublish, confirmations, and live domain-event synchronization use the shared engines.
- Typed settings, complete tag workflows, Git status/diff/history/commit/pull/push, metadata/site reports with apply/cancel, managed background progress, locale changes, media information, and airplane-gated AI actions are terminal-accessible.
- State, renderer-buffer, input-decoder, shared local/remote persistence, task/report, event, locale, and real SSH PTY behavior have isolated tests.

### A2UI surfaces — Complete

Done:

- Eight structured render tools and fixed native rendering for ten surface types, including all required chart and form variants.
- Per-conversation form, tab, and dismissal state tied to stable message/tool-index IDs and restored on reopen.
- Safe text/JSON fallbacks, inert assistant markup, persistent expanded surfaces during later streaming, and visibly rejected non-allowlisted actions.

## Suggested Order

1. CLI, MCP, and domain events.
2. Conversational AI and agent tools.
3. Embeddings and duplicate detection.
4. Documentation and menu UX.
5. Headless server and TUI.
6. A2UI after conversational AI exists. (Complete)

The order may change when an extension directly unlocks a concrete user workflow; it must not create a parallel data model or bypass core engines.
