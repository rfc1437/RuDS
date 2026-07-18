# RuDS Core Plan

## Goal

RuDS is the native Rust replacement for bDS2. Core covers the complete everyday workflow: open an existing project, author content, preview and generate the site, publish it, and maintain database/filesystem integrity.

The behavioural contract is `specs/*.allium`. When a spec is ambiguous, `../bDS2` is the reference implementation. [RUST_PLAN_EXTENSION.md](RUST_PLAN_EXTENSION.md) contains optional and advanced surfaces.

Status in this document describes the current source code as of 2026-07-18. It deliberately does not track build runs, test runs, release gates, or implementation history.

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
| `bds-core` | Models, SQLite, filesystem formats, engines, rendering, generation, AI, publishing, and Lua |
| `bds-editor` | Reusable Ropey/Syntect/Cosmic Text editor widget |
| `bds-ui` | Iced application, native menus/dialogs, platform lifecycle, and embedded preview |
| `bds-cli` | Extension-only headless automation surface; currently a stub |

## Current Core Status

### Project, persistence, and integrity — Mostly done

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

Open:

- Watch project files for external changes and reconcile open editors, preview, and database state.
- Make the script file extension project-configurable instead of hard-coding `.lua`.

### Native desktop shell — Done

Available:

- Iced workspace with activity bar, sidebar, tabs, status bar, task/output panel, toasts, and modal flows.
- Native muda menus, localized labels, accelerators, and state-dependent command enablement.
- Native file/folder dialogs and recent-project handling.
- macOS open-file and URL lifecycle plumbing.
- Localized UI separate from project content language.

Open:

- No known large core block. New menu commands must continue to be wired through both localization layers and the native intercept.

### Authoring and editor — Mostly done

Available:

- Dashboard and editors for posts, translations, media, tags, templates, scripts, and settings.
- Post create, edit, duplicate, publish, unpublish, discard, and delete flows.
- Media import, replacement, metadata editing, translations, thumbnails, filters, and post assignment.
- Template and Lua script creation, editing, validation, publication, and deletion.
- Rope-based editing with syntax highlighting, selection, clipboard, undo/redo, word/line/page movement, line numbers, soft wrapping, mouse selection, and committed IME input.
- Automatic translation flows with airplane-mode gating and media translation propagation.

Open:

- Replace the post-links placeholder with the specified functional linked-post view.
- Add the specified batch workflow for images linked from post content.

### Rendering, preview, and generation — Mostly done

Available:

- Shared Markdown and Liquid rendering for preview and generated output.
- Template lookup, routes, language variants, published snapshots, custom assigns, and URL rewriting.
- Native gallery, YouTube, Vimeo, photo archive, and tag cloud macros.
- User-authored Lua macro invocation during rendering.
- Localhost-only Axum preview server, draft routes, embedded Wry preview, and external-browser preview.
- Complete site generation with pages, archives, feeds, sitemap, static assets, changed-file tracking, parallel page rendering, and Pagefind output.
- OPML/menu document loading and normalized Home-first menu output.

Open:

- Represent generation as the specified group of section tasks followed by a final search-index task, rather than one coarse application task.
- Keep closing concrete output differences found against bDS2; approved normalization differences belong in this document when discovered.

### One-shot AI — Mostly done

Available:

- Configurable online and airplane-mode OpenAI-compatible endpoints.
- Secure API-key storage through the operating-system keychain.
- Model catalog discovery and model selection.
- Post translation, media translation, image alt text, post analysis, taxonomy analysis, and language detection.
- Explicit offline gating and user-visible errors.

Open:

- Persist actual input, output, cache-read, and cache-write token usage where the schema provides those fields.

Interactive chat, tools, agents, and MCP belong to the extension plan.

### Publishing — Mostly done

Available:

- SCP and rsync publishing through system commands.
- SSH-agent-only authentication with no password prompts.
- Separate HTML, thumbnail, and media targets.
- `.meta` exclusion, changed-file skipping for SCP, progress reporting, cancellation, and UI commands.

Open:

- Run the three upload targets as parallel tasks instead of processing them sequentially.
- Integrate external-file watching with publish/integrity workflows.

### Lua scripting — Partly done

Available:

- Sandboxed vendored Lua 5.4 runtime with cancellation and execution limits.
- Application log, progress, and toast functions.
- User macro and transform execution, including Blogmark transforms.
- Lua script persistence, rebuild, metadata diff, validation, and editor support.

Open:

- Expose the specified `bds.*` API for post, media, tag, project, and other script-visible data.
- Generate and bundle the Lua API reference, canonical type reference, and macro/transform/utility examples under `docs/scripting/`.
- Add a documentation-sync check tied to the exposed API.

## Remaining Core Blocks

1. Complete the Lua host API and scripting documentation.
2. Add filesystem change watching and reconciliation.
3. Parallelize publishing targets.
4. Finish the linked-post and linked-image authoring workflows.
5. Add generation section-task grouping and AI token accounting.
6. Remove the hard-coded Lua script extension.

Core is feature-complete when these blocks are closed and the implementation continues to satisfy the Allium and bDS2 compatibility contracts.
