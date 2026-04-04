# bDS Compatibility Inventory: TypeScript vs Rust

Tracks feature parity between the TypeScript bDS app and the Rust rewrite (RuDS).

Legend: `[x]` implemented/verified, `[ ]` not yet implemented

---

## Database Schema

- [x] projects table
- [x] posts table (with published_* legacy columns)
- [x] post_translations table
- [x] media table
- [x] media_translations table (mediaTranslations)
- [x] tags table
- [x] templates table (with kind/status defaults matching TypeScript)
- [x] scripts table (with kind/status defaults matching TypeScript)
- [x] post_links table (postLinks)
- [x] post_media table
- [x] settings table
- [x] generated_file_hashes table
- [x] db_notifications table
- [x] chat_conversations table
- [x] chat_messages table
- [x] ai_providers table
- [x] ai_models table
- [x] ai_model_modalities table
- [x] ai_catalog_meta table
- [x] embedding_keys table
- [x] dismissed_duplicate_pairs table
- [x] import_definitions table
- [x] All 10 unique indexes
- [x] FTS5 virtual tables (posts_fts, media_fts) -- present in fixture DB, runtime creation deferred to M1
- [x] Default values match TypeScript (status, kind, enabled, version, entrypoint)

## File Formats

- [ ] Post frontmatter (YAML) read/write
- [ ] Translation file format (posts/YYYY/MM/slug.lang.md)
- [ ] Media sidecar (.meta) read/write
- [ ] Template frontmatter read/write
- [ ] Script frontmatter read/write
- [ ] meta/tags.json
- [ ] meta/project.json
- [ ] meta/categories.json
- [ ] meta/category-meta.json
- [ ] meta/publishing.json
- [ ] meta/menu.opml

## Slug Generation

- [x] Basic transliteration (Unicode to ASCII, lowercase, hyphens, trim)
- [x] German umlauts: ae/oe/ue/ss/Ae/Oe/Ue (matches TypeScript transliteration npm)
- [x] Uniqueness: base, then {slug}-2..999, then {slug}-{timestamp}

## Content Location

- [x] Published posts have NULL content in DB, body in filesystem .md
- [x] Draft posts have content in DB
- [x] Published translations have NULL content in DB
- [x] Published templates have NULL content in DB
- [x] Published scripts have NULL content in DB

## Rendering

- [ ] Liquid template rendering (subset: if/elsif/else, for, assign, render, whitespace stripping)
- [ ] Liquid filters: escape, url_encode, default, append, i18n, markdown
- [ ] Markdown rendering via pulldown-cmark
- [ ] Built-in macros: gallery, youtube, vimeo, photo_archive, tag_cloud
- [ ] RSS/Atom feed generation
- [ ] Sitemap generation
- [ ] Generated file hash tracking (incremental)
- [ ] Pagefind search index generation (via pagefind crate library API)

## Search

- [ ] FTS5 post indexing with Snowball stemmers (24 languages)
- [ ] FTS5 media indexing
- [ ] Cross-language stemming

## Publishing

- [ ] SSH/SCP upload
- [ ] Rsync upload
- [ ] Three parallel upload targets (html, thumbnails, media)
- [ ] .meta files excluded from upload

## AI Integration

- [ ] Two-endpoint model (online + airplane)
- [ ] One-shot operations (translate, analyze, etc.)
- [ ] Chat with tool use
- [ ] Model catalog refresh
- [ ] Secure key storage (OS keychain)

## Thumbnails

- [ ] Small (150px), Medium (400px), Large (800px), AI (448x448 JPEG)
- [ ] WEBP output

## MCP Server

- [ ] HTTP + stdio transports
- [ ] Read-only tools
- [ ] Proposal-based write tools
