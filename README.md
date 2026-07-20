# RuDS

RuDS is a native Rust blogging desktop application and the successor to bDS2. It manages local projects from authoring through preview, static-site generation, integrity checks, and publishing while preserving the existing bDS filesystem and SQLite formats.

The project is under active development. Core blogging workflows are broadly available; remaining core work and optional extensions are tracked separately.

## Available Features

- Native Iced desktop workspace with localized menus, tabs, automatically paged post/media sidebars, dialogs, tasks, embedded Wry previews, and a live Pico CSS theme editor.
- Post and translation authoring with draft/published lifecycle, metadata, tags, categories, links, media, and batch gallery-image import.
- Media import, thumbnails, metadata translations, filters, validation, and post assignment.
- WordPress WXR migration with saved analyses, HTML-to-Markdown and shortcode conversion, conflict/taxonomy review, recoverable 500-item execution batches, media-parent linking, progress reporting, and optional AI-assisted taxonomy mapping.
- Template and Lua script management with explicit syntax-check feedback, using a custom Ropey/Syntect/Cosmic Text editor and the documented, bDS2-signature-compatible project-scoped [`bds` host API](docs/scripting/API_REFERENCE.md) across utilities, rendered macros, and Blogmark transforms.
- SQLite and filesystem persistence with frontmatter, sidecars, rebuild, metadata diff/repair, and FTS5 search.
- Optional on-device multilingual semantic search and tag suggestions backed by a persistent USearch index, with duplicate-post review and dismissal in the desktop workspace.
- Read-only in-app browsers in the Help menu for the bundled global `DOCUMENTATION.md`, the generated Lua API reference with public types and runnable examples, the [CLI/server/TUI documentation](CLI.md), and the [MCP server documentation](MCP.md), with safe GFM rendering and confirmed external links.
- A localized OPML menu editor manages pages, submenus, and category archives with protected Home ordering, keyboard-accessible tree controls, drag-and-drop, and bDS2-compatible persistence.
- Project-scoped typed domain events synchronize desktop views with shared-engine and future CLI mutations; persisted CLI notifications are consumed once, and the selected UI language is shared through settings.
- Headless `bds-cli` automation for rebuild/repair/render, publishing and Git sync, post/media/gallery creation, effective shared settings with secret-presence redaction, projects, utility Lua tasks, JSON I/O, airplane-mode AI routing, and guarded launcher installation from Settings → Data or `bds-cli install`.
- Local MCP automation over stdio or a localhost-only stateless HTTP endpoint, with project resources, read/search/count tools, inert write proposals, explicit desktop approval, and opt-in Claude Code/Copilot configuration.
- A full Ratatui terminal workspace, available locally through `bds-cli tui`/`BDS_MODE=tui` and remotely through authenticated SSH shell sessions, with shared post/template/script editing and publishing, project/search/command overlays, settings, tags, Git, reports, task progress, live multi-client updates, locale changes, and airplane-mode AI gating.
- `bds-cli server` hosting the shared application engines over a loopback-by-default, public-key-only SSH service, with restrictive private key material, live authorization updates, terminal-session transport, CLI-change synchronization, ordered domain/task events, and native desktop remote-project selection.
- Markdown/Liquid rendering with native macros, multilingual routes, feeds, sitemap, Pagefind, and incremental site generation through cancellable section task groups.
- Local preview in the app or system browser.
- Optional one-shot AI translation, description, analysis, taxonomy, and language-detection operations using independent online and local OpenAI-compatible profiles. Each profile has secure credentials, persistently discovered chat/title/image model selections, explicit tool/vision overrides, chat testing, and status-bar airplane-mode routing.
- Persistent conversational AI with safe Markdown, streamed and cancellable responses, model/session/token tracking, bounded project-aware blog tools, and localized conversation management in the Chat workspace. Allowlisted render tools add persistent native cards, charts, forms, lists, metrics, mind maps, tables, and tabs without executing assistant-provided HTML or JavaScript.
- SSH-agent-based SCP or rsync publishing.
- Integrated Git workflow with repository initialization, Git LFS image tracking, status and diffs, branch/file history, commits, remotes, cancellable fetch/pull/push, and post-pull filesystem reconciliation; network actions respect airplane mode.
- Site, media, and translation validation plus `ruds://new-post` Blogmark capture and Lua transforms; bDS2 keeps its separate `bds2://` bookmarklet protocol.

RuDS uses no JavaScript application runtime and loads no CSS or JavaScript from CDNs. The preview is served by the Rust application and displayed by the operating-system webview.

The packaged Apple Silicon application requires macOS 26 or newer.

Packaged executables share native `bds-core` and `bds-server` dynamic libraries. ONNX Runtime is statically contained in `bds-core`; the package does not ship or download a separate ONNX library. The **Install CLI** action writes a small forwarding launcher to `~/.local/bin`, so the command continues to execute the packaged CLI beside the same runtime libraries instead of copying them.

Local macOS packages are ad-hoc signed without hardened runtime. A Developer ID and notarization remain optional release-channel steps for downloads that should pass Gatekeeper without a user override.

## Repository Map

- `crates/bds-core` — data, engines, rendering, AI, publishing, and Lua
- `crates/bds-editor` — reusable syntax-highlighting editor
- `crates/bds-ui` — desktop application and platform integration
- `crates/bds-cli` — headless automation CLI over the shared engines
- `crates/bds-mcp` — packaged stdio MCP transport over the shared MCP engine
- `crates/bds-server` — reusable headless host, SSH transport, remote protocol, and desktop client library
- `specs` — authoritative Allium behavior specifications
- `fixtures` — compatibility projects and generated-site fixtures
- `locales` — UI and native-menu translations

## References

- [Specification index](SPECIFICATION_INDEX.md)
- [User guide](DOCUMENTATION.md)
- [CLI, headless server, and terminal UI](CLI.md)
- [MCP server](MCP.md)
- [UI style guide](docs/UI_STYLE_GUIDE.md)
- `../bDS2` — reference implementation when an Allium contract is ambiguous

Contributor workflow and project invariants are documented in [AGENTS.md](AGENTS.md).

## Development gates

Install the dependency audit tools once:

```sh
cargo install cargo-machete cargo-outdated
```

Before committing, verify dependency usage and freshness, then build and test the complete workspace:

```sh
cargo machete --with-metadata
cargo outdated --workspace --root-deps-only --exit-code 1
cargo build --workspace
cargo test --workspace
```

The full test suite starts loopback-only mock and preview servers, so localhost binding must be permitted.

## Headless server

Run `bds-cli server` for a dedicated headless process, or set `BDS_MODE=server` when launching `bds-ui`. The SSH listener defaults to `127.0.0.1:2222`; use `--bind`/`BDS_SSH_BIND` and `--port`/`BDS_SSH_PORT` to opt into another address. Startup prints the private `authorized_keys` path. The desktop creates its own private `id_ed25519.pub`; add that public key to the server file, then use **File → Connect to Server…** and select a remote project. Host keys are recorded on first connection and verified thereafter. Run `bds-cli tui` or set `BDS_MODE=tui` for the local terminal workspace; an authenticated SSH shell opens the same workspace against server-side data and locale. Press `:` for its command list and `:?` for help.
