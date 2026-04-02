# bDS Rust Rewrite Plan

bDS is a blogging desktop system that is currently in the following folder:

../bDS/

this is to be rewritten with Rust and this is the plan. For any implementation detail, look into the ../bDS/ folder into the typescript code there. Assume the typescript code as "the truth" about the current implementation.

This plan is split into multiple documents:

- [RUST_PLAN_CORE.md](RUST_PLAN_CORE.md) — the shipping core. This must already be a fully functioning blogging app: create, edit, preview, generate, and publish content using the exact same on-disk and generated formats as the current app.
- [RUST_PLAN_EXTENSION.md](RUST_PLAN_EXTENSION.md) — parity work and advanced tooling that can land after the core app is usable.
- [RUST_EXECUTION_BACKLOG.md](RUST_EXECUTION_BACKLOG.md) — implementation backlog grouped by milestone and crate.
- [RUST_COMPATIBILITY_MATRIX_TEMPLATE.md](RUST_COMPATIBILITY_MATRIX_TEMPLATE.md) — template for the required persistence and compatibility inventory.

## Non-Negotiable Constraints

1. **Content compatibility is exact.** Existing SQLite data, markdown/frontmatter, translation files, media sidecars, templates, menu documents, generated HTML structure, feeds, and sitemaps must remain readable and writable by the Rust app.
2. **No JavaScript anywhere.** No npm, no webview, no JS runtime, no Electron. This is a supply-chain security constraint. The entire app is pure Rust plus native platform APIs. Pagefind is integrated as a Rust library dependency (`pagefind` crate), not as an external binary.
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
| Platform lifecycle | **objc2** (macOS only, cfg-gated) | Thin shim for `application:openFile:`, `application:openURLs:`, and other `NSApplicationDelegate` hooks |

### Why this stack

- **Iced** is a published crate with versioned releases, proper documentation, and a stable API. It uses wgpu for GPU-accelerated rendering and follows the Elm architecture (Message → update → view).
- **muda** and **rfd** render through real platform APIs (NSMenu, NSOpenPanel, etc.) with zero fidelity loss versus hand-rolled platform code, while providing cross-platform support from day one.
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
- **bds-ui**: application shell, Iced views and components, message routing, platform integration (muda for menus, rfd for dialogs, objc2 shim for macOS lifecycle). Depends on bds-core and bds-editor.
- **bds-cli**: headless automation surface. Depends on bds-core only.

## Distribution Characteristics

- **Single static binary.** No external runtime dependencies (no GTK, no Electron, no Node.js). Lua and SQLite are compiled into the binary.
- **Binary size:** ~15–25 MB (versus 150–200 MB for the current Electron app).
- **Memory usage:** ~50–80% less RAM than the Electron app (no Chromium process).
- **Startup:** Significantly faster (no V8/Chromium initialization).
- **Zero install dependencies:** users download one binary. No Homebrew, no system packages.

## Split Rationale

The old single-file plan mixed three categories of work:

- work required to ship a real replacement for the current app
- work required for eventual feature parity
- work that is valuable but not on the critical path to replacing TypeScript

The new split keeps the shipping path narrow. Core is only the work needed to replace the current app for everyday authoring and publishing. Extensions carry the rest.

## Release Gates

The Rust rewrite is not considered successful until the core release can do all of the following with existing bDS project data:

1. Open a real project created by the TypeScript app.
2. Create and edit posts, translations, media, tags, templates, and settings.
3. Preview drafts and published content locally.
4. Generate a complete site whose output matches the current app modulo approved normalization differences.
5. Publish the generated output to a remote target.
6. Rebuild the database from files and run metadata diff without losing information.
7. Operate as a native desktop application with native menus and command handling.

## Verification Baseline

Both plan documents assume the same verification baseline:

1. Fixture projects exported from the current app are the compatibility corpus.
2. Golden-file tests compare Rust-written files against TypeScript-written files.
3. Golden-output tests compare generated sites between the current app and the Rust app.
4. No feature is complete until both UI behavior and underlying engine behavior are covered.

## Recommended Repository Strategy

Build the Rust rewrite as a separate parallel project, not inside the current TypeScript application tree.

Why:

1. The rewrite has a different language stack, build chain, packaging model, runtime model, and test harness.
2. The compatibility target is the current app's behavior and data, not shared source code.
3. Keeping the rewrite isolated avoids contaminating this repo with long-lived dual-toolchain complexity.
4. The current app remains the stable reference implementation while the Rust app catches up.

Recommended structure:

- keep this repository as the reference implementation and fixture source
- create a sibling repository such as `bds-rust`
- pull compatibility fixtures from this repo into the Rust repo on a controlled cadence
- compare generated output across repos in the Rust repo's CI

Only use the same repository if you explicitly want a temporary umbrella monorepo and are willing to accept:

- slower CI
- mixed Node and Rust release pipelines
- more complex contributor onboarding
- higher risk of plan drift between the legacy app and the rewrite
