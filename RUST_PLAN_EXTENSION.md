# bDS Rust Rewrite — Extension Plan

## Goal

Deliver the rest of current-app parity and the advanced tooling that is valuable, but not required to ship a production-capable Rust replacement for everyday authoring and publishing.

Extensions begin only after the core plan is already usable end to end.

## Extension Principles

1. No extension may break the core compatibility contract.
2. Extensions must reuse core models, engines, and persistence rules rather than invent parallel formats.
3. UI features must still be tied to underlying functionality; no placeholder shells.
4. AI features remain gated by offline mode and must prefer local models or provide explicit user feedback when unavailable. One-shot AI operations (6 operations: translate post/media, image alt text, post analysis, taxonomy analysis, language detection) are part of core with two configurable OpenAI-compatible endpoints (online + airplane mode), and respect the same offline gating.
5. Extensions use the same Iced + muda + rfd platform stack as core. No additional UI frameworks.

## Extension Buckets

### Bucket A: Git And Validation Tooling

#### Scope

- `GitEngine` via `git2` crate (shell out for LFS operations — no LFS library binding)
- Git sidebar
- diff view
- commit, fetch, pull, push
- richer site validation views
- richer metadata diff UI

#### Why extension

Helpful for operators, but not required to create, preview, generate, and publish content.

#### Done when

- users can inspect repo state and diffs from within the app
- Git actions work reliably enough to replace the current Git tooling

### Bucket B: Import And Migration Tooling

#### Scope

- WXR parser
- import analysis
- import execution
- saved import definitions

#### Why extension

Important onboarding feature, but not required to operate existing bDS projects.

#### Done when

- WordPress import flows are usable and recoverable
- import results match the current app's expectations closely enough for fixture-based tests

### Bucket C: AI Chat And Tool Use

#### Scope

- chat UI (sidebar panel with conversation history)
- streaming responses via `reqwest` (SSE / chunked transfer)
- tool use against local engines (post lookup, media search, template info, etc.)
- multi-turn conversation management
- model and credential settings UI (extends the core AI endpoint configuration)

One-shot AI operations (translation, image alt text, title suggestion) are already in core scope. This bucket adds the interactive, conversational AI layer on top.

The AI client extends the core `reqwest` + `serde_json` client with streaming support and tool-call parsing. Works with any OpenAI-compatible endpoint: OpenAI, Anthropic-via-proxy, local Ollama, etc.

#### Hard constraints

- offline mode gates all automatic AI work
- cloud providers are disabled when offline mode is enabled
- local providers remain usable when allowed
- unavailable operations produce explicit user-visible feedback

#### Done when

- AI chat is useful for content-related queries and actions without weakening the app's offline guarantees
- Streaming and tool use work reliably with at least one OpenAI-compatible provider

### Bucket D: Search, Embeddings, And Duplicate Detection

#### Scope

- ONNX embeddings via `ort` (ONNX Runtime Rust bindings)
- HNSW vector index via `usearch`
- semantic search UI
- duplicate detection UI

#### Why extension

Improves discovery and cleanup, but not required for core publishing flows.

#### Done when

- near-duplicate detection and semantic search are trustworthy on real projects

### Bucket E: Translation QA And Documentation UX

#### Scope

- translation validation engine and report view
- in-app documentation browser
- richer scripting docs browser and examples

#### Why extension

Operationally valuable, but the core release can ship with generated markdown docs and without dedicated browsing surfaces.

#### Done when

- translation integrity issues are discoverable before publish
- docs are comfortably browsable in-app

### Bucket F: Menu Editing And Deep Links

#### Scope

- menu editor UI for OPML/menu documents
- deep-link protocol handling beyond core app-open behaviors

#### Why extension

Core must read menu documents for rendering compatibility, but editing them can follow once the main authoring path is stable.

#### Done when

- users can inspect and edit menus from the Rust app
- deep links cover parity flows from the current app

### Bucket G: MCP And Automation Surfaces

#### Scope

- workspace CLI per `cli.allium`: `rebuild | repair | render | upload | push | pull | post | media | gallery | config | project | lua` — boots the same repo, settings, and database as the desktop app, with no listeners and no window; logging to the rotating log file
- MCP server per `mcp.allium` (translation tools, stats and media resources, proposal workflow)
- domain event bus per `events.allium`: every successful entity mutation (posts, media, tags, templates, scripts) broadcasts `(entity, entity_id, action)`; global settings changes broadcast a settings event. One topic/payload shape covers in-app mutations and external CLI writes, replacing the old `db_notifications` polling design. The bus also feeds the TUI (Bucket L) and any future multi-client sync.

#### Why extension

Useful ecosystem surface, not required to replace the desktop app itself.

#### Done when

- automation consumers can drive the Rust app safely and consistently
- CLI changes are detected and surfaced to the running desktop app via the event bus

### Bucket H: Blogmark And Transform Pipeline

#### Scope

- `BlogmarkTransformService`
- external content capture (bookmarklet) workflow
- transform script execution chain
- integration with Lua transform scripts

#### Why extension

Secondary content-capture workflow. Not required for core authoring with existing projects.

#### Done when

- external content can be captured and transformed into posts via the Blogmark pipeline
- transform scripts execute reliably

### Bucket I: Rich Markdown Editor

#### Scope

- WYSIWYG or hybrid markdown editing (comparable to the baseline app's rich editor)
- macro syntax preview in editor
- image insert dialog from linked media

#### Why extension

Core ships with the bds-editor syntax-highlighting plain-text editor and live preview. The baseline app defaults to a rich editor, so this is a user-facing regression that should be addressed after core stabilizes.

#### Architecture advantage

The bds-editor crate (ropey + syntect + cosmic-text) built during core provides the foundation for the rich editor. Incremental additions:

- inline rendering of bold/italic/headers via cosmic-text mixed font styles
- inline image preview via Iced image rendering within the custom widget
- macro block preview (render macro output inline in the editor)
- clickable links

This is an evolution of the existing editor widget, not a separate technology decision.

#### Done when

- users can edit content with a rich editor comparable to the baseline app's editing experience

### Bucket K: Headless Server Mode

#### Scope

Per `server.allium`:

- boot modes `desktop | server | tui` resolved from an environment variable at application start
- headless server: full engine, no window, SSH transport for remote TUI/GUI clients
- SSH key material (host key + authorized_keys, mode 600) generated on first boot into the private OS app-data dir — never the repo or the project folder
- GUI "connect to server" flow

#### Why extension

Client/server split is new bDS2 capability, not needed to replace the desktop app for local use.

#### Done when

- the app can run headless and accept an SSH-transported client session against real project data

### Bucket L: Terminal UI

#### Scope

Per `tui.allium` — a second renderer over the same shared UI core:

- sidebar views (posts, media, templates, scripts, tags, settings, git) with sidebar/editor focus model and vim-style + arrow navigation that skips section headers
- post editor driving the same workflow as the GUI: canonical-language edits update the post, other languages update translations, publish routes through the same pipeline
- live updates via the domain event bus (Bucket G)

#### Why extension

Second renderer; requires the shared UI core and (for remote use) Bucket K.

#### Done when

- everyday authoring (browse, edit, publish) works in a terminal against the same project data as the GUI

### Bucket J: A2UI Server-Driven Surfaces

#### Scope

- A2UI component renderer (layout, input, display, chart, etc.)
- A2UI surface manager for bidirectional data flow
- integration with AI assistant outputs

#### Why extension

Tightly coupled to the AI feature set (Bucket C). Not required until AI features are active.

#### Done when

- AI-generated dynamic UI surfaces render correctly in the app

## Suggested Extension Ordering

```text
Bucket A Git + Validation
  ↓
Bucket B Import
  ↓
Bucket C AI
  ↓
Bucket D Embeddings + Duplicates
  ↓
Bucket E Translation QA + Docs UX
  ↓
Bucket F Menu Editing + Deep Links
  ↓
Bucket G MCP + Automation
  ↓
Bucket H Blogmark + Transforms
  ↓
Bucket I Rich Editor (builds on bds-editor from core)
  ↓
Bucket K Headless Server Mode
  ↓
Bucket L Terminal UI (after Buckets G + K)
  ↓
Bucket J A2UI Surfaces (after Bucket C)
```

The ordering is pragmatic, not mandatory. Git and validation are the closest to operational parity, so they should land first after core.

## Extension Verification

Every extension still inherits the core verification baseline plus extension-specific tests:

- Git fixtures for repo state and diff rendering
- WXR fixtures for import
- mocked SSE and provider fixtures for AI
- embedding fixtures for semantic search and duplicate detection
- translation fixture projects with intentional integrity failures
- OPML fixtures for menu editing

## Out Of Scope For Now

- cross-platform packaging polish beyond what the core and extension work naturally require
- feature work that introduces new persistence formats before full parity is reached