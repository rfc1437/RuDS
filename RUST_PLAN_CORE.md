# RuDS Core Plan

## Goal

RuDS is the native Rust replacement for bDS2. Core covers the complete everyday workflow: open an existing project, author content, preview and generate the site, publish it, and maintain database/filesystem integrity.

The behavioural contract is `specs/*.allium`. When a spec is ambiguous, `../bDS2` is the reference implementation. [RUST_PLAN_EXTENSION.md](RUST_PLAN_EXTENSION.md) contains optional and advanced surfaces.

Status in this document describes the current source code as of 2026-07-20. It deliberately does not track build runs, test runs, release gates, or implementation history.

## Non-Negotiable Constraints

- Preserve existing SQLite, Markdown/frontmatter, translation, media sidecar, template, menu, and generated-site formats.
- Published post bodies live in files, not the database.
- Use a native Rust desktop stack: Iced, muda, rfd, and Wry. No JavaScript application runtime or remote assets.
- Preview and generated output share the same Rust rendering pipeline.
- Python scripting is intentionally replaced by Lua; built-in macros remain native Rust.
- UI locale follows the operating system; rendered content language follows project settings.
- Online AI is optional and blocked in airplane mode; local models may remain available.
- Metadata writes, publishing, metadata diff, and rebuild must share one field mapping.
- User-visible functionality requires a UI entry point, and UI controls require working functionality.
- macOS commands must be wired through native menus and application lifecycle handling.

## Workspace

| Crate | Responsibility |
|---|---|
| `bds-core` | Shared dynamic library for models, SQLite, filesystem formats, engines, rendering, generation, AI, publishing, and Lua |
| `bds-editor` | Reusable Ropey/Syntect/Cosmic Text editor widget |
| `bds-ui` | Iced application, native menus/dialogs, platform lifecycle, and embedded preview |
| `bds-cli` | Extension-only headless automation surface over the shared engines |
| `bds-server` | Reusable extension-only headless host and authenticated SSH transport library |

## Current Core Status

### Project, persistence, and integrity — Done

Available:

- Existing project discovery and opening, with the database stored in private OS application data.
- Diesel-backed SQLite schema and migrations.
- Posts, translations, media, media translations, post-media links, tags, templates, scripts, settings, menus, and publishing settings.
- Draft/published lifecycle with canonical content placement.
- YAML frontmatter, translation files, media sidecars, template/script files, and project metadata.
- FTS5 post and media search with language-aware stemming.
- Database rebuild from filesystem and directional metadata diff/repair.
- Structured site, media, and translation validation.
- Unbounded unique slug allocation.
- Explicit rebuild-from-filesystem paths for manual file changes. bDS2 does
  not live-watch arbitrary project files; its external-change watcher is the
  extension CLI/database-notification contract.
- Typed, project-scoped post/media/tag/template/script/project/settings events at shared mutation boundaries, with deterministic subscribers and a one-shot persisted CLI-to-desktop notification bridge.

### Native desktop shell — Done

Available:

- Iced workspace with activity bar, sidebar, tabs, status bar, task/output panel, toasts, and modal flows.
- Native muda menus, localized labels, accelerators, and state-dependent command enablement.
- Event-driven sidebar/editor refresh, deleted-entity tab closure, and persisted server-selected UI language without mutation feedback loops.
- Native file/folder dialogs and recent-project handling.
- macOS open-file and URL lifecycle plumbing.
- Localized UI separate from project content language.
- Desktop-backed Lua application capabilities for clipboard, folders, preview targeting, title-bar metrics, renderer readiness, and supported menu actions.

Open:

- No known large core block. New menu commands must continue to be wired through both localization layers and the native intercept.

### Authoring and editor — Done

Available:

- Dashboard and editors for posts, translations, media, tags, templates, scripts, and settings.
- Post create, edit, publish, unpublish, discard, and delete flows.
- Media import, replacement, metadata editing, translations, thumbnails, filters, post assignment, and the post-editor batch gallery-image workflow.
- Template and Lua script creation, editing, validation, publication, and deletion.
- Rope-based editing with syntax highlighting, selection, clipboard, undo/redo, word/line/page movement, line numbers, soft wrapping, mouse selection, and committed IME input.
- Automatic translation flows with airplane-mode gating and media translation propagation.
- Functional post-links panel with backlinks, outlinks, and navigation to linked posts.

### Rendering, preview, and generation — Mostly done

Available:

- Shared Markdown and Liquid rendering for preview and generated output.
- Template lookup, routes, language variants, published snapshots, custom assigns, and URL rewriting.
- Native gallery, YouTube, Vimeo, photo archive, and tag cloud macros.
- User-authored Lua macro invocation during rendering.
- Localhost-only Axum preview server, draft routes, embedded Wry preview, and external-browser preview.
- Complete site generation with pages, archives, feeds, sitemap, static assets, changed-file tracking, parallel page rendering, and Pagefind output; full generation and validation apply run as grouped section tasks followed by search indexing.
- Canonical bDS2-compatible OPML/menu document loading, recursive Home-first normalization, and renderer consumption of the saved tree.

Open:

- Keep closing concrete output differences found against bDS2; approved normalization differences belong in this document when discovered.

### One-shot AI — Done

Available:

- Independent online and airplane-mode OpenAI-compatible profiles, selected by the status-bar airplane switch.
- Secure keychain credentials for both profiles, optional for local endpoints.
- Model discovery without a preselected model; per-profile chat/title/image selections, explicit tool/vision overrides, and minimal chat tests.
- Post translation, media translation, image alt text, post analysis, taxonomy analysis, WordPress-import taxonomy mapping, and language detection.
- Explicit offline gating and user-visible errors.
- Parsed input, output, cache-read, and cache-write token usage returned from every one-shot operation; persistent chat accounting is tracked in the extension plan.

Interactive chat, tools, agents, and MCP belong to the extension plan.

### Publishing — Done

Available:

- SCP and rsync publishing through system commands.
- SSH-agent-only authentication with no password prompts.
- Separate HTML, thumbnail, and media targets.
- `.meta` exclusion, changed-file skipping for SCP, progress reporting, and UI commands.
- One managed publish job processes the HTML, thumbnail, and media targets in
  sequence, matching bDS2. Parallel target uploads are not a parity requirement.

### Lua scripting — Done

Available:

- Sandboxed vendored Lua 5.4 runtime with cancellation and execution limits.
- Application log, progress, and toast functions.
- Project-scoped bDS2-compatible core `bds.*` APIs with matching Lua signatures and failure values.
- User macro and transform execution, including project capabilities during rendered macros and Blogmark transforms.
- Managed Blogmark transform cancellation and live, non-duplicated progress reporting.
- Project-scoped post and media reindexing from Lua without disturbing other projects.
- Lua script persistence, rebuild, metadata diff, validation, and editor support.
- Fixed `.lua` script file contract.
- Generated and bundled API/type references plus executable macro, transform, and utility examples under `docs/scripting/`.
- Manifest-driven runtime, documentation, and completion data with drift checks.
- The embedding extension adds the bDS2-compatible `bds.embeddings` namespace; `bds.sync` remains outside the core API.

## Remaining Core Blocks

1. Add generation section-task grouping.
2. Return normalized token accounting from one-shot AI calls.

Core is feature-complete when these blocks are closed and the implementation continues to satisfy the Allium and bDS2 compatibility contracts.
