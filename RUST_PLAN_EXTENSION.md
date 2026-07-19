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

### WordPress import — Open

Open:

- WXR parsing, import analysis, recoverable execution, saved import definitions, and import UI.
- The existing media importer is core functionality and does not implement this workflow.

### Conversational AI and agent tools — Open

Open:

- Conversation persistence and chat UI.
- Streaming OpenAI-compatible responses and tool-call parsing.
- Tools over posts, media, templates, search, and other core engines.
- Agent integrations such as Claude Code and Copilot where required by the specs.
- Replace the current Chat placeholders with the working feature.

Core endpoint settings, offline gating, key storage, model discovery, and six one-shot operations are already implemented.

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

### CLI, MCP, and domain events — Open

Open:

- Implement `bds-cli`; its current binary is only a stub.
- Commands from `cli.allium` and `cli_sync.allium` using the same project, database, engines, and settings as the desktop app.
- Reuse the core gallery batch-import engine already used by the desktop post editor for the CLI `gallery` command.
- MCP tools/resources and proposal-based writes from `mcp.allium`.
- Domain event bus from `events.allium` for desktop, CLI, TUI, and future remote clients.
- Replace the MCP settings placeholder.

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

1. WordPress import.
2. CLI, MCP, and domain events.
3. Conversational AI and agent tools.
4. Embeddings and duplicate detection.
5. Documentation and menu UX.
6. Headless server and TUI.
7. A2UI after conversational AI exists.

The order may change when an extension directly unlocks a concrete user workflow; it must not create a parallel data model or bypass core engines.
