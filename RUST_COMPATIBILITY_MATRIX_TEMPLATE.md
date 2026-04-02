# bDS Rust Rewrite — Compatibility Matrix Template

## Purpose

This document is the required inventory of persisted fields and compatibility-sensitive behaviors. Fill it before engine implementation starts, then keep it current as the rewrite proceeds.

One row per persisted field or compatibility-sensitive artifact.

## How To Use

1. Start with posts, translations, media, templates, project metadata, menus, and generated outputs.
2. Record the current TypeScript behavior first.
3. Record the intended Rust behavior only if it is identical or explicitly approved as a divergence.
4. Link every row to tests or golden fixtures once available.

## Field Matrix

| Domain | Entity | Field / Artifact | Type | Current Source Of Truth | Persisted In | Read Path | Write Path | Rebuild From Files | Metadata Diff | Publish Impact | Generation Impact | Rust Status | Fixture / Test | Notes |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| posts | Post | id | string | DB + frontmatter | DB, frontmatter |  |  |  |  |  |  | not-started |  |  |
| posts | Post | title | string |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | slug | string |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | excerpt | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | content body | markdown | published body in file; draft body per current app rules | DB?, markdown file |  |  |  |  |  |  | not-started |  |  |
| posts | Post | status | enum |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | author | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | language | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | do_not_translate | bool |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | tags | list |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | categories | list |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | template_slug | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| posts | Post | checksum | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| translations | PostTranslation | translation_for | string |  |  |  |  |  |  |  |  | not-started |  |  |
| media | Media | alt | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| media | Media | caption | string? |  |  |  |  |  |  |  |  | not-started |  |  |
| media | MediaTranslation | translationFor | string |  | DB |  |  |  |  |  |  | not-started |  |  |
| media | MediaTranslation | language | string |  | DB |  |  |  |  |  |  | not-started |  |  |
| media | MediaTranslation | title | string? |  | DB |  |  |  |  |  |  | not-started |  |  |
| media | MediaTranslation | alt | string? |  | DB |  |  |  |  |  |  | not-started |  |  |
| media | MediaTranslation | caption | string? |  | DB |  |  |  |  |  |  | not-started |  |  |
| posts | PostLink | sourcePostId | string |  | DB |  |  |  |  |  |  | not-started |  |  |
| posts | PostLink | targetPostId | string |  | DB |  |  |  |  |  |  | not-started |  |  |
| posts | PostLink | linkText | string? |  | DB |  |  |  |  |  |  | not-started |  |  |
| templates | Template | file content | liquid |  |  |  |  |  |  |  |  | not-started |  |  |
| scripts | Script | runtime | runtime | Python in current app, Lua in Rust app | app behavior |  |  | n/a | n/a | yes | yes | approved-divergence |  | Explicit break |
| scripts | BlogmarkTransform | transform chain | runtime | Python in current app | app behavior |  |  | n/a | n/a | no | no | deferred |  | Deferred to extension Bucket H |
| menus | MenuDocument | source document | opml/xml |  |  |  |  |  |  |  |  | not-started |  |  |
| generation | SiteOutput | route path | path |  | generated files |  |  | n/a | n/a | yes | yes | not-started |  |  |
| generation | Feed | rss/atom output | xml |  | generated files |  |  | n/a | n/a | yes | yes | not-started |  |  |
| generation | Sitemap | sitemap output | xml |  | generated files |  |  | n/a | n/a | yes | yes | not-started |  |  |
| generation | GeneratedFileHash | content hash | string | DB | DB |  |  | n/a | n/a | yes | yes | not-started |  | Skip-unchanged-writes optimization |
| search | PostsFts | FTS5 index | virtual table | DB | DB (FTS5) |  |  | rebuild | n/a | no | no | not-started |  |  |
| search | MediaFts | FTS5 index | virtual table | DB | DB (FTS5) |  |  | rebuild | n/a | no | no | not-started |  |  |
| search | ClientSearch | search index | binary | generated files | generated files |  |  | n/a | n/a | yes | yes | not-started |  | Determine in M0: integrate via `pagefind` crate library API |
| media | Thumbnail | generated thumbnail | image file | filesystem | filesystem |  |  | regenerate | n/a | yes | yes | not-started |  | `image` crate; verify size/format matches current `sharp` output |
| macros | BuiltInMacro | gallery | Rust-native | app code | n/a |  |  | n/a | n/a | no | yes | not-started |  | Was JS in TS app; Rust in Rust app |
| macros | BuiltInMacro | youtube | Rust-native | app code | n/a |  |  | n/a | n/a | no | yes | not-started |  | Was JS in TS app; Rust in Rust app |
| macros | BuiltInMacro | vimeo | Rust-native | app code | n/a |  |  | n/a | n/a | no | yes | not-started |  | Was JS in TS app; Rust in Rust app |
| macros | BuiltInMacro | photo_archive | Rust-native | app code | n/a |  |  | n/a | n/a | no | yes | not-started |  | Was JS in TS app; Rust in Rust app |
| macros | BuiltInMacro | tag_cloud | Rust-native | app code | n/a |  |  | n/a | n/a | no | yes | not-started |  | Was JS in TS app; Rust in Rust app |

## Behavior Matrix

Use this for compatibility-sensitive behaviors that are not a single persisted field.

| Area | Behavior | Current App Rule | Rust Target Rule | Allowed Divergence | Validation Method | Status | Notes |
|---|---|---|---|---|---|---|---|
| posts | published body storage | body not stored in DB when published | identical | no | fixture DB + file assertions | not-started |  |
| localization | UI locale | follows OS locale | identical | no | manual + integration test | not-started |  |
| localization | render language | follows project setting | identical | no | preview/generation tests | not-started |  |
| menus | native menu routing | current app uses native menus | native menus via muda (cross-platform: NSMenu, Win32, GTK) | implementation changes, behavior identical | menu integration tests | not-started |  |
| scripts | user Python scripts | supported in current app | unsupported in Rust app | yes | migration note + runtime detection tests | approved-divergence |  |
| rendering | built-in macros | JS server-side (gallery, youtube, vimeo, photo_archive, tag_cloud) | Rust-native in bds-core/render | implementation language changes, output must match | golden-generated-site comparisons | not-started | These are NOT Python macros |
| rendering | Liquid feature subset | liquidjs 10.25 (full spec available) | Rust Liquid (scoped to used subset) | implementation may differ for unused features | template compatibility suite | not-started | Only ~35% of spec used by default templates |
| slugs | slug generation | `transliteration` npm package | `deunicode` Rust crate | possible edge-case differences | slug corpus tests in M0 fixtures | not-started | Verify against real content |
| editor | content editing | Milkdown WYSIWYG (default) | plain-text syntax-highlighting editor (bds-editor: ropey + syntect + cosmic-text) + live preview | yes | n/a | approved-divergence | Rich editor deferred to extension Bucket I, builds on bds-editor foundation |
| preview | asset sourcing | local assets only | identical | no | HTML assertions | not-started |  |
| runtime | JS dependency | Electron (Chromium + Node.js) | no JavaScript anywhere — pure Rust + native APIs | yes (intentional) | build verification: no JS in dependency tree | approved-divergence | Supply-chain security constraint |
| runtime | async executor | Node.js event loop | tokio | yes (internal) | n/a | approved-divergence | Used for preview server, publish, file watching |
| media | thumbnail generation | `sharp` (libvips) | `image` crate | output dimensions and format must match | golden-file comparison of thumbnails | not-started | Fallback to `libvips-rs` if quality/perf insufficient |
| generation | client-side search | determine in M0 | `pagefind` crate library API (`PagefindIndex`) | output must match | golden-generated-site comparisons | not-started | Rust library dep, no CLI binary |
| generation | parallel rendering | single-threaded in current app | `rayon` for parallel page rendering | yes (faster, same output) | golden-generated-site comparisons | approved-divergence | Output must be identical regardless of parallelism |
| ai | one-shot AI operations | not in current app | `reqwest` against configurable OpenAI-compatible endpoint | yes (new feature) | mocked endpoint tests | approved-divergence | Translation, alt text, title suggestion. Entirely optional — app works without endpoint configured |

## Status Values

- `not-started`
- `in-progress`
- `verified`
- `approved-divergence`
- `blocked`

## Sign-Off Checklist

- every persisted field used by core is represented
- every approved divergence is explicit
- every row links to a fixture or test before release
- metadata diff, rebuild, and publish implications are filled in for all relevant rows