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
- Online/airplane endpoint routing uses the shared secure endpoint and model infrastructure; unavailable modes direct the user to the existing localized AI settings.

Persistent A2UI surfaces remain separately tracked below.

### Embeddings, semantic search, and duplicates — Open

Existing source contains schema placeholders only.

Open:

- Embedding generation and persistence.
- Vector index and semantic search.
- Expose `bds.embeddings` after those engines exist.
- Duplicate detection, dismissed-pair handling, metadata integrity, and UI.
- Replace the Find Duplicates placeholder.

### Translation QA and documentation UX — Partly done

Done:

- Translation validation engine and report view.

Open:

- In-app project documentation browser.
- Browsable Lua API documentation and examples.
- Replace Documentation and API Documentation placeholders.

The generated Lua documentation and examples are complete core functionality; this section tracks only their in-app browsing experience.

### Menu editor and deep links — Partly done

Done:

- Menu file parsing/rendering and Home-first normalization.
- macOS URL plumbing and the sole bDS2-compatible Blogmark action at `ruds://new-post`; RuDS neither registers nor accepts bDS2's `bds2://` scheme.

Open:

- OPML/menu editor UI.
- Replace the Menu Editor placeholder.

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

### Headless server — Open

Open:

- Desktop/server/TUI boot-mode selection.
- Headless engine host and SSH transport.
- Private host-key and authorized-key management.
- Desktop connection flow for remote projects.

### Terminal UI — Open

Open:

- Terminal renderer over shared application workflows.
- Sidebar/editor navigation, editing, publishing, and live domain-event updates.
- Remote operation through the headless server.

### A2UI surfaces — Open

Open:

- A2UI component renderer, surface state/data flow, and AI-assistant integration.

## Suggested Order

1. CLI, MCP, and domain events.
2. Conversational AI and agent tools.
3. Embeddings and duplicate detection.
4. Documentation and menu UX.
5. Headless server and TUI.
6. A2UI after conversational AI exists.

The order may change when an extension directly unlocks a concrete user workflow; it must not create a parallel data model or bypass core engines.
