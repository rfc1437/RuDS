# Feasibility Analysis: bDS Rewrite in Rust (Green-Field, No JavaScript)

## Context

Green-field rewrite of bDS blogging CMS in pure Rust. No migration — only the on-disc content format (Markdown + YAML frontmatter) must be preserved. Everything else is up for debate.

**Constraints:**
- No JavaScript at all (supply chain security concern — no npm/webview/JS runtime)
- Python scripting replaced with Rust-native scripting language (Lua)
- WYSIWYG Markdown editor not needed — syntax-highlighting source editor is sufficient
- Mac is first-class, Windows slightly behind, Linux as compatibility option
- Built by team of AI agents, no timeline pressure
- Green-field: existing codebase is reference only, not migrated

---

## Current Scale

| Layer | LOC | Key Tech |
|-------|-----|----------|
| Main process (engines, IPC, DB) | ~45,800 | Node.js, 43+ engine classes, 257 IPC methods |
| Renderer (UI) | ~30,200 | React 19, Zustand, 80 components, Milkdown, Monaco |
| Tests | ~69,500 | Vitest, 210 test files |
| Dependencies | 54 runtime + 27 dev | AI SDK, Pyodide, sharp, git, MCP, etc. |

---

## Rust GUI Framework Comparison (Mac-First)

The "Mac must be first-class" constraint is critical. It reshuffles the ranking.

### Framework Assessment

| Framework | Maturity | Mac Quality | Editor Story | Widget Set |
|-----------|----------|-------------|-------------|------------|
| **GTK4-rs + libadwaita** | 9/10 | 6/10 — functional but non-native feel (no system menu bar by default, non-native scrolling, GTK file dialogs) | GtkSourceView (excellent) | Rich |
| **Iced** | 6/10 | 8/10 — GPU-rendered via Metal, consistent look, respects platform DPI. Not "native macOS" but consistent everywhere (like Electron was). | Custom needed (`syntect` + `ropey` + `cosmic-text`) | Growing, needs custom work |
| **Floem** | 5/10 | 7/10 — used in Lapce which runs well on Mac | Lapce has one but not extracted | Sparse |
| **Dioxus** | 4/10 | Variable | None | Tiny |
| **Slint** | 7/10 | 7/10 — native rendering support | None | Limited |
| **egui** | 8/10 | 7/10 — works well on Mac via wgpu | Wrong paradigm for editing | Immediate mode |

### Mac-specific concerns

**GTK4 on macOS:**
- Requires GTK4 installed (Homebrew/MacPorts) or bundled (~50MB overhead)
- Doesn't use native macOS menu bar by default (can be configured but finicky)
- Non-native scrolling physics, selection behavior, keyboard shortcuts
- File/print dialogs look GTK, not macOS
- Apps like GIMP/Inkscape run on Mac via GTK — functional but feel "foreign"

**Iced on macOS:**
- Renders via wgpu/Metal — native GPU acceleration
- Consistent cross-platform look (no "foreign toolkit" feel)
- You control all UX conventions (can implement macOS cmd-shortcuts, native-feeling scrolling)
- No external toolkit dependency — statically compiled
- Binary stays small (~10-20MB)
- The trade-off: you build more widgets yourself, but AI agents can handle this

### The editor situation

With WYSIWYG dropped, a syntax-highlighting text editor is achievable in pure Rust:

| Approach | Effort | Quality |
|----------|--------|---------|
| **GtkSourceView** (GTK4 only) | 0 weeks (built-in) | Production-grade, 50+ languages |
| **Custom iced widget** (`ropey` + `syntect` + `cosmic-text`) | 4-6 weeks | Good — proven building blocks, AI agents can build this |
| **Extract cosmic-edit** from COSMIC | 3-4 weeks | Untested outside COSMIC |

For an iced-based app, a custom editor widget built from `ropey` (rope buffer) + `syntect` (highlighting) + `cosmic-text` (text layout/shaping) is realistic. This is how Lapce and COSMIC's editors work internally.

### Recommendation: **Iced**

Given Mac-first + no JS + green-field + AI agents building it:

1. **Iced** gives the best Mac experience without GTK's foreign feel
2. Custom editor is feasible (4-6 weeks for AI agents using proven crates)
3. GPU rendering (wgpu/Metal) = smooth, modern feel on all platforms
4. Elm architecture = predictable state management, good for AI-generated code
5. No external toolkit dependency = simpler distribution
6. System76 backing ensures continued development

---

## Scripting Language Replacement (Python → ?)

| Language | Crate | Maturity | Speed | Ecosystem | Best For |
|----------|-------|----------|-------|-----------|----------|
| **Lua** | `mlua` | 10/10 | Very fast (LuaJIT) | Huge (30+ years, games, tools) | Battle-tested embedding, rich stdlib |
| **Rhai** | `rhai` | 8/10 | Moderate | Small but growing | Rust-native, safe sandbox, no FFI |
| **Starlark** | `starlark-rust` | 7/10 | Fast | Niche (Bazel/Buck) | Python-like syntax, deterministic |

**Recommendation: Lua via `mlua`.**
- Most mature embedding story. Used by Neovim, Redis, nginx, game engines.
- `mlua` supports Lua 5.4 and LuaJIT. Sandboxing built in.
- Exposing engine methods to Lua is trivial (`mlua::UserData` trait).
- Distribution: Lua runtime is ~300KB, compiled into the binary. No external install needed.
- Existing Python macros would need rewriting, but Lua syntax is simple for end users.

---

## Two Viable Paths

### Path A: Iced (Recommended — Mac-first, fully native, zero JS)

| Aspect | Assessment |
|--------|-----------|
| **JS purity** | 100%. No webview, no JS runtime, no npm. |
| **Rendering** | wgpu (Metal on Mac, Vulkan/DX12 elsewhere). Smooth, consistent everywhere. |
| **Markdown editor** | Custom widget: `ropey` + `syntect` + `cosmic-text`. Live preview pane via `pulldown-cmark` rendering. |
| **Code editor** | Same custom widget with Lua/Liquid/CSS language definitions via `syntect`. |
| **Widgets needed** | Tree view, resizable panes, tabs, modals, toasts, forms — all buildable in iced, some exist in `iced_aw`. |
| **Mac experience** | Native GPU, respects DPI, no foreign toolkit feel. cmd-key shortcuts, native-feeling scroll. |
| **Distribution** | Single static binary. No external deps. ~15-25MB. |
| **Risk** | Medium. Custom editor is the biggest piece. Iced ecosystem is growing but you build more yourself. |

### Path B: GTK4-rs + GtkSourceView (Fastest, Linux-best)

| Aspect | Assessment |
|--------|-----------|
| **JS purity** | 100%. |
| **Rendering** | Native GTK (Cairo/Vulkan). |
| **Editors** | GtkSourceView — zero work, production-grade. |
| **Widgets** | All built-in (TreeView, Paned, Notebook, etc.). |
| **Mac experience** | 6/10. Functional but feels non-native. Requires GTK4 installed via Homebrew. |
| **Distribution** | Requires GTK4 runtime on user machine (Mac/Windows). |
| **Risk** | Low for functionality, medium-high for Mac UX polish. |

---

## Backend Subsystem Assessment (Green-Field)

Since this is a rewrite, effort is for building clean, not porting messy. AI agents can generate verbose Rust boilerplate efficiently.

### 1. Database — Easy
- **`rusqlite`** with `refinery` for migrations. Design schema fresh.
- On-disc format preserved: Markdown + YAML frontmatter files. DB is for indexing/metadata only.
- **Key crates:** `rusqlite`, `serde`, `serde_yaml`, `gray_matter` or manual YAML parsing
- **Cost:** 2-3 weeks

### 2. Core Engine Layer — Medium
- Fresh design with Rust idioms (traits, enums, Result). No need to mirror 43 TS classes.
- **No WXR import** — dropped entirely.
- **Key crates:** `pulldown-cmark` (Markdown), `liquid` (templates), `image` (thumbnails/WEBP), `git2` (Git, no LFS — shell out for LFS ops), `notify` (file watching), `uuid`, `serde_json`
- **Cost:** 8-12 weeks

### 3. AI/LLM Integration — Easy ✅ (simplified)
- **OpenAI-compatible endpoint only.** Single wire format (Chat Completions API).
- Single-shot calls only (no streaming, no chat, no tool call loops). Used for: translation, image description, title generation.
- **`reqwest`** + `serde_json` + OpenAI request/response structs. ~500 LOC.
- Works with OpenCode Zen, local Ollama, any OpenAI-compatible proxy.
- **Cost:** 1-2 weeks

### 4. Lua Scripting — Easy
- **`mlua`** crate: embed Lua 5.4. Sandboxed, ~300KB. Expose engine APIs via `UserData` trait.
- **Cost:** 2-3 weeks

### 5. Static Site Generation — Medium
- `pulldown-cmark` → HTML, `liquid` templates, `rayon` for parallel generation
- Pagefind CLI for search index (unchanged, it's a binary)
- RSS/Atom/sitemap generation via `quick-xml`
- **Cost:** 4-6 weeks

### 6. Publishing — Easy
- SSH: `ssh2` crate. rsync: shell out to `rsync`. Git: `git2`.
- **Cost:** 1-2 weeks

### 7. Embeddings & Similarity — Medium
- `ort` (ONNX Runtime) for local embeddings. `usearch` Rust bindings for HNSW index.
- **Cost:** 2-3 weeks

### 8. MCP Server — Optional (v2)
- Deferred. Not needed for v1.
- **Cost:** 0 weeks (v1)

### 9. Preview Server — Easy
- `axum` HTTP server serving generated HTML + media.
- **Cost:** 1-2 weeks

### 10. UI (Iced) — Hard (largest piece)
- Custom editor widget: `ropey` + `syntect` + `cosmic-text` (4-6 weeks)
- Application chrome: tree sidebar, tab bar, resizable panes, settings, modals (6-8 weeks)
- Chat panel, AI integration UI (2-3 weeks)
- Git diff view, validation views, import wizard (3-4 weeks)
- **Cost:** 15-21 weeks

### 11. Tests — Medium
- Rust `#[test]` + `mockall` for traits. AI agents generate test boilerplate well.
- Test as you build (TDD per CLAUDE.md rules)
- **Cost:** Folded into each subsystem (add ~40% to each estimate)

---

## What Gets Better (All Paths)

1. **Binary size:** 5-15MB vs 150-200MB
2. **Memory:** ~50-80% less RAM (no Chromium main process)
3. **Startup:** Significantly faster
4. **Generation perf:** `rayon` + `pulldown-cmark` + `liquid` = faster site generation
5. **Type safety:** Compile-time guarantees on everything
6. **Security:** Memory-safe, smaller supply-chain surface

## What Gets Worse (All Paths)

1. **Dev velocity:** Rust is slower to iterate than TypeScript
2. **AI SDK:** Maintain your own streaming multi-provider SDK
3. **Macro migration:** Existing user Python macros must be rewritten in Lua
4. **Talent pool:** Smaller contributor base

---

## Effort Summary (Green-Field with AI Agents)

| Subsystem | Weeks (with tests) |
|-----------|-------------------|
| Database + on-disc format | 3-4 |
| Core engines (posts, media, tags, projects) | 11-17 |
| AI/LLM (single-shot, OpenAI-compat) | 1-2 |
| Lua scripting | 3-4 |
| Static site generation | 6-8 |
| Publishing + Git | 2-3 |
| Embeddings + search | 3-4 |
| Preview server | 1-2 |
| **UI (Iced)** | **21-29** |
| Integration + polish | 4-6 |
| **Total** | **55-79 person-weeks** |

Compared to original estimate (76-103 weeks): **dropping WXR import, streaming chat, multi-provider AI, and MCP server saves ~20 weeks.**

With AI agents running 2-3 parallel workstreams: **4-8 months** of active sessions.

---

## Recommended Architecture: Iced + Rust

```
bds-rust/
├── crates/
│   ├── bds-core/          # Domain types, traits, on-disc format (Markdown+YAML)
│   ├── bds-db/            # SQLite layer (rusqlite + refinery migrations)
│   ├── bds-engine/        # Business logic (posts, media, tags, projects, generation)
│   ├── bds-ai/            # Multi-provider LLM client (streaming, tool calls)
│   ├── bds-lua/           # Lua scripting runtime (mlua)
│   ├── bds-git/           # Git operations (git2 + LFS shell-out)
│   ├── bds-publish/       # SSH/rsync deployment
│   ├── bds-search/        # Embeddings (ort) + similarity (usearch) + pagefind
│   ├── bds-mcp/           # MCP server
│   ├── bds-preview/       # HTTP preview server (axum)
│   └── bds-editor/        # Syntax-highlighting editor widget (ropey+syntect+cosmic-text)
├── src/                   # Iced application (UI, state, views)
├── assets/                # Icons, themes
└── Cargo.toml             # Workspace
```

**Key Rust crates:**
| Purpose | Crate |
|---------|-------|
| GUI framework | `iced` |
| Text buffer | `ropey` |
| Syntax highlighting | `syntect` |
| Text layout | `cosmic-text` |
| Database | `rusqlite` |
| Migrations | `refinery` |
| Markdown → HTML | `pulldown-cmark` |
| Templates | `liquid` |
| YAML frontmatter | `serde_yaml` |
| Image processing | `image` |
| HTTP server (preview) | `axum` |
| HTTP client (AI) | `reqwest` |
| Git | `git2` |
| SSH | `ssh2` |
| File watching | `notify` |
| Lua scripting | `mlua` |
| ONNX inference | `ort` |
| Vector search | `usearch` |
| Async runtime | `tokio` |
| Parallelism | `rayon` |
| Serialization | `serde` + `serde_json` |
| UUID | `uuid` |
| Error handling | `thiserror` + `anyhow` |

---

## Verdict

**Feasible: Yes.** With the constraints resolved (no WYSIWYG, Lua instead of Python, green-field, AI agents, no timeline):

- Every subsystem has proven Rust crate equivalents
- The hardest piece is the custom editor widget (~4-6 weeks) and AI streaming client (~5-7 weeks)
- Iced gives Mac-first cross-platform GUI without GTK's foreign feel or any JS runtime
- Single static binary, ~15-25MB, zero external dependencies
- AI agents are well-suited to Rust's verbose but structured patterns

**The main risk** is Iced's maturity (6/10). It's real software backing a real desktop environment (COSMIC), but the widget ecosystem is young. You will build things that React gave you for free. With AI agents and no deadline, this is acceptable.

---

## Remaining Questions

1. Confirm Iced vs GTK4-rs preference? (Iced = better Mac, more custom work. GTK = more widgets, worse Mac feel.)
2. Which AI providers are must-have? (Anthropic + OpenAI + Ollama? Or all 5?)
3. Should the MCP server remain, or is it optional for v1?
4. Is WordPress WXR import a v1 requirement?
