# RuDS

RuDS is a native Rust blogging desktop application and the successor to bDS2. It manages local projects from authoring through preview, static-site generation, integrity checks, and publishing while preserving the existing bDS filesystem and SQLite formats.

The project is under active development. Core blogging workflows are broadly available; remaining core work and optional extensions are tracked separately.

## Available Features

- Native Iced desktop workspace with localized menus, tabs, sidebars, dialogs, tasks, and embedded Wry previews.
- Post and translation authoring with draft/published lifecycle, metadata, tags, categories, links, media, and batch gallery-image import.
- Media import, thumbnails, metadata translations, filters, validation, and post assignment.
- WordPress WXR migration with saved analyses, HTML-to-Markdown and shortcode conversion, conflict/taxonomy review, recoverable 500-item execution batches, media-parent linking, progress reporting, and optional AI-assisted taxonomy mapping.
- Template and Lua script management with explicit syntax-check feedback, using a custom Ropey/Syntect/Cosmic Text editor and the documented, bDS2-signature-compatible project-scoped [`bds` host API](docs/scripting/API_REFERENCE.md) across utilities, rendered macros, and Blogmark transforms.
- SQLite and filesystem persistence with frontmatter, sidecars, rebuild, metadata diff/repair, and FTS5 search.
- Project-scoped typed domain events synchronize desktop views with shared-engine and future CLI mutations; persisted CLI notifications are consumed once, and the selected UI language is shared through settings.
- Headless `bds-cli` automation for rebuild/repair/render, publishing and Git sync, post/media/gallery creation, shared settings/projects, utility Lua tasks, JSON I/O, airplane-mode AI routing, and guarded launcher installation from Settings → Data or `bds-cli install`.
- Markdown/Liquid rendering with native macros, multilingual routes, feeds, sitemap, Pagefind, and incremental site generation through cancellable section task groups.
- Local preview in the app or system browser.
- Optional one-shot AI translation, description, analysis, taxonomy, and language-detection operations using online or local OpenAI-compatible endpoints with airplane-mode gating.
- SSH-agent-based SCP or rsync publishing.
- Integrated Git workflow with repository initialization, Git LFS image tracking, status and diffs, branch/file history, commits, remotes, cancellable fetch/pull/push, and post-pull filesystem reconciliation; network actions respect airplane mode.
- Site, media, and translation validation plus `ruds://new-post` Blogmark capture and Lua transforms; bDS2 keeps its separate `bds2://` bookmarklet protocol.

RuDS uses no JavaScript application runtime and loads no CSS or JavaScript from CDNs. The preview is served by the Rust application and displayed by the operating-system webview.

## Repository Map

- `crates/bds-core` — data, engines, rendering, AI, publishing, and Lua
- `crates/bds-editor` — reusable syntax-highlighting editor
- `crates/bds-ui` — desktop application and platform integration
- `crates/bds-cli` — headless automation CLI over the shared engines
- `specs` — authoritative Allium behavior specifications
- `fixtures` — compatibility projects and generated-site fixtures
- `locales` — UI and native-menu translations

## Plans and References

- [Core plan and current status](RUST_PLAN_CORE.md)
- [Extension plan and current status](RUST_PLAN_EXTENSION.md)
- [Specification index](SPECIFICATION_INDEX.md)
- [UI style guide](docs/UI_STYLE_GUIDE.md)
- `../bDS2` — reference implementation when an Allium contract is ambiguous

Contributor workflow and project invariants are documented in [AGENTS.md](AGENTS.md).
