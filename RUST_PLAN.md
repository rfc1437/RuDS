# bDS Rust Rewrite Plan

bDS is a blogging desktop system. The behavioural baseline is **bDS2**, an Elixir (Phoenix LiveView) implementation living in:

../bDS2/

This project (RuDS) rewrites it in Rust. For any implementation detail, look into ../bDS2/ — the Elixir code there is "the truth" about the current implementation. The old TypeScript app at ../bDS is historical and no longer the reference.

## Spec Baseline

The authoritative behaviour contract is the allium spec set in `specs/`, synced wholesale from `../bDS2/specs/` on 2026-07-18 (46 files, all pass `allium check`). Rules:

- When spec and Rust code disagree, the spec wins; when the spec is ambiguous, the bDS2 Elixir code wins.
- Five specs are new with the bDS2 sync and have **no Rust implementation yet**: `rendering.allium` (core scope — render assigns/filters/macros shared by preview and generation), `cli.allium`, `events.allium`, `server.allium`, `tui.allium` (all extension scope).
- The concrete code-vs-spec gaps are tracked in RUST_EXECUTION_BACKLOG.md under "Milestone M6: bDS2 Spec Parity".
- When re-syncing specs from bDS2, re-run `allium check specs/*.allium` and refresh the M6 gap list.

This plan is split into multiple documents:

- [RUST_PLAN_CORE.md](RUST_PLAN_CORE.md) — the shipping core. This must already be a fully functioning blogging app: create, edit, preview, generate, and publish content using the exact same on-disk and generated formats as the current app.
- [RUST_PLAN_EXTENSION.md](RUST_PLAN_EXTENSION.md) — parity work and advanced tooling that can land after the core app is usable.
- [RUST_EXECUTION_BACKLOG.md](RUST_EXECUTION_BACKLOG.md) — implementation backlog grouped by milestone and crate.
- [RUST_COMPATIBILITY_MATRIX_TEMPLATE.md](RUST_COMPATIBILITY_MATRIX_TEMPLATE.md) — template for the required persistence and compatibility inventory.

## Non-Negotiable Constraints

1. **Content compatibility is exact.** Existing SQLite data, markdown/frontmatter, translation files, media sidecars, templates, menu documents, generated HTML structure, feeds, and sitemaps must remain readable and writable by the Rust app.
2. **No JavaScript application runtime.** No npm, Node.js, Electron, or remotely loaded application assets. The required in-editor and style preview panels use Wry to display the localhost-only Rust preview server in the operating system webview; the separate Open in Browser command opens the same preview in the system browser. Pagefind is integrated as a Rust library dependency (`pagefind` crate), not as an external binary.
3. **Script compatibility is intentionally broken.** Python/Pyodide is removed. Rust bDS is a green-field app and uses Lua for user-authored scripting. Existing Python scripts are not carried forward as compatible runtime artifacts. Built-in macros (gallery, youtube, vimeo, photo_archive, tag_cloud) are re-implemented as native Rust, not routed through Lua.
4. **Core release ships as a native desktop app.** Primary target is macOS, but the UI stack (Iced + muda + rfd) is cross-platform from day one. Native menus, key handling, open-file/deep-link handling, and platform command routing are part of core scope, not follow-up polish.
5. **Template editing is core scope.** Template rendering without template management UI is not sufficient.
6. **Script API docs remain mandatory.** The runtime changes to Lua, but the app still ships and generates proper scripting API documentation.
7. **Split localization is mandatory.** UI locale follows the OS. Rendered and previewed content language follows project settings.

## UI Technology Stack

The application uses the following UI and platform integration stack:

| Layer | Technology | Purpose |
|---|---|---|
| UI framework | **Iced** (Elm architecture) | Application layout, views, widgets, event loop |
| Text editing | **ropey** + **syntect** + **cosmic-text** | Custom syntax-highlighting editor widget for markdown, Liquid templates, and Lua scripts |
| Native menus | **muda** | Cross-platform native menu bar (NSMenu on macOS, Win32 menus on Windows, GTK/dbus on Linux) |
| File dialogs | **rfd** | Cross-platform native file/folder dialogs (NSOpenPanel/NSSavePanel on macOS, equivalents elsewhere) |
| Internal preview | **wry** | Embedded post and style preview panels backed only by the localhost Rust preview server |
| Platform lifecycle | **objc2** (macOS only, cfg-gated) | Thin shim for `application:openFile:`, `application:openURLs:`, and other `NSApplicationDelegate` hooks |

### Why this stack

- **Iced** is a published crate with versioned releases, proper documentation, and a stable API. It uses wgpu for GPU-accelerated rendering and follows the Elm architecture (Message → update → view).
- **muda** and **rfd** render through real platform APIs (NSMenu, NSOpenPanel, etc.) with zero fidelity loss versus hand-rolled platform code, while providing cross-platform support from day one.
- **wry** preserves the baseline app's internal Markdown/Preview workflow using the operating system webview. It is a presentation surface for the loopback preview server, not the application runtime; external-browser preview remains available separately.
- **ropey + syntect + cosmic-text** gives full control over the editor experience: rope-based efficient text storage, Sublime Text syntax grammars (markdown, Liquid, Lua), and proper font shaping and layout via the same engine used by cosmic-DE.
- The only platform-specific code is a small (~50 line) lifecycle shim for macOS app delegate hooks, conditionally compiled via `cfg(target_os = "macos")`. Linux and Windows equivalents are bounded and isolated to the same module.

## Workspace Shape

```text
bds-rust/
├── Cargo.toml
├── crates/
│   ├── bds-core/        # engines, models, rendering, publishing
│   ├── bds-editor/      # custom Iced editor widget (ropey + syntect + cosmic-text)
│   ├── bds-ui/          # Iced application, views, platform integration (muda, rfd)
│   └── bds-cli/         # later, optional automation surface
├── migrations/
├── locales/
├── assets/
└── docs/
    └── scripting/       # generated Lua API docs + guides
```

### Crate responsibilities

- **bds-core**: all engines, models, persistence, rendering, publishing — zero UI dependencies.
- **bds-editor**: reusable Iced custom widget for syntax-highlighting text editing. Depends on ropey, syntect, cosmic-text, and iced. Does not depend on bds-core. Can be extracted as a standalone crate.
- **bds-ui**: application shell, Iced views and components, embedded localhost preview (wry), message routing, platform integration (muda for menus, rfd for dialogs, objc2 shim for macOS lifecycle). Depends on bds-core and bds-editor.
- **bds-cli**: headless automation surface. Depends on bds-core only.

## Distribution Characteristics

- **Single application binary.** No Electron or Node.js runtime. Lua and SQLite are compiled into the binary; internal preview uses the operating system webview supplied by the target platform.
- **Binary size:** ~15–25 MB.
- **Memory usage / startup:** no BEAM VM or bundled browser engine — one native application process plus the operating system webview used while preview is visible.
- **Platform prerequisites:** macOS uses the system WebKit runtime. Windows/Linux packaging must account for Wry's platform webview prerequisites without adding an application-managed JavaScript runtime.

## Split Rationale

The old single-file plan mixed three categories of work:

- work required to ship a real replacement for the current app
- work required for eventual feature parity
- work that is valuable but not on the critical path to replacing the baseline app

The new split keeps the shipping path narrow. Core is only the work needed to replace the current app for everyday authoring and publishing. Extensions carry the rest.

## Release Gates

The Rust rewrite is not considered successful until the core release can do all of the following with existing bDS project data:

1. Open a real project created by the baseline app.
2. Create and edit posts, translations, media, tags, templates, and settings.
3. Preview drafts and published content both in the post editor's internal preview panel and in the external system browser.
4. Generate a complete site whose output matches the current app modulo approved normalization differences.
5. Publish the generated output to a remote target.
6. Rebuild the database from files and run metadata diff without losing information.
7. Operate as a native desktop application with native menus and command handling.

## Verification Baseline

Both plan documents assume the same verification baseline:

1. Fixture projects exported from the current app are the compatibility corpus.
2. Golden-file tests compare Rust-written files against baseline-written files.
3. Golden-output tests compare generated sites between the current app and the Rust app.
4. No feature is complete until both UI behavior and underlying engine behavior are covered.

## Repository Strategy

This repository (RuDS) is the Rust rewrite, kept separate from the baseline. `../bDS2` remains the stable reference implementation and fixture source; compatibility fixtures are pulled from it on a controlled cadence and generated output is compared here.
