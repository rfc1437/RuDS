# bDS Rust Rewrite - Vollständige Spezifikationssammlung

Diese Datei dient als Index zu allen Allium-Spezifikationen und Schema-Inventarisierungen für die Rust-Implementierung.

## Verfügbare Spezifikationen

### Kern-Spezifikationen

| Datei | Scope | Status | Beschreibung |
|-------|-------|--------|--------------|
| `bds.allium` | Core | ✅ Existiert | Haupt-Spezifikation mit allen Referenzen |
| `project.allium` | Core (Wave 1) | ✅ Existiert | Projekt-Management |
| `post.allium` | Core (Wave 1) | ✅ Existiert | Post-Lifecycle, Frontmatter, Dateistruktur |
| `media.allium` | Core (Wave 1) | ✅ Existiert | Media-Import, Thumbnails, Sidecars |
| `translation.allium` | Core (Wave 1) | ✅ Existiert | Post- und Media-Übersetzungen |
| `tag.allium` | Core (Wave 1) | ✅ Existiert | Tags mit Mass-Operationen |
| `template.allium` | Core (Wave 1/4) | ✅ Existiert | Liquid-Template-Management |
| `script.allium` | Core (Wave 6) | ✅ Existiert | Skripting (Lua in Rust) |
| `menu.allium` | Core (Read) | ✅ Existiert | OPML-Navigationsmenü |
| `metadata.allium` | Core (Wave 1) | ✅ Existiert | Projekt-Konfiguration, Kategorien, Publishing |

### Infrastruktur-Spezifikationen

| Datei | Scope | Status | Beschreibung |
|-------|-------|--------|--------------|
| `search.allium` | Core (Wave 1) | ✅ Existiert | FTS5 Full-Text Search mit Snowball |
| `generation.allium` | Core (Wave 4) | ✅ Existiert | Statische Site-Generierung |
| `preview.allium` | Core (Wave 4) | ✅ Existiert | Lokaler Preview-Server |
| `publishing.allium` | Core (Wave 5) | ✅ Existiert | SSH-Upload (SCP/rsync) |
| `task.allium` | Core (Wave 1) | ✅ Existiert | Background Task Manager |
| `i18n.allium` | Core (Alle) | ✅ Existiert | Split Localization (UI vs Content) |
| `schema.allium` | Core (Wave 1) | ✨ Neu | Vollständiges SQLite-Schema |
| `frontmatter.allium` | Core (Wave 1) | ✨ Neu | Alle Frontmatter-Formate |
| `template_context.allium` | Core (Wave 4) | ✨ Neu | Liquid-Template-Kontext |
| `media_processing.allium` | Core (Wave 1) | ✨ Neu | Thumbnail-Generierung, Bildverarbeitung |

### Integration-Spezifikationen

| Datei | Scope | Status | Beschreibung |
|-------|-------|--------|--------------|
| `git.allium` | Extension A | ✅ Existiert | Git-Operationen, LFS, Reconciliation |
| `import.allium` | Extension B | ✨ Neu | WordPress-WXR-Analyse, Konfliktprüfung und Importausführung |
| `mcp.allium` | Extension G | ✅ Existiert | MCP-Server (Tools, Resources) |
| `ai.allium` | Core/Extension C | ✅ Existiert | AI One-Shot Tasks und Chat |
| `embedding.allium` | Extension D | ✅ Existiert | Semantic Similarity (HNSW) |
| `cli_sync.allium` | Core (Wave 5) | ✅ Existiert | CLI-zu-App Notification Sync |
| `metadata_diff.allium` | Core (Wave 1) | ✅ Existiert | DB/Dateisystem-Diff und Rebuild |

### Von bDS2 übernommene Spezifikationen (Sync 2026-07)

| Datei | Scope | Status | Beschreibung |
|-------|-------|--------|--------------|
| `rendering.allium` | Core | ✨ Neu | Render-Subsystem (Assigns, Filter, Makros für Preview + Generation) |
| `cli.allium` | Extension G | ✨ Neu | Workspace-CLI-Tool (rebuild, repair, render, upload, …) |
| `events.allium` | Extension | ✨ Neu | Domain Event Bus (Multi-Client-Synchronisation) |
| `server.allium` | Extension | ✨ Neu | Headless Server Mode (SSH-Transport für TUI/GUI) |
| `tui.allium` | Extension | ✨ Neu | Terminal UI (zweiter Renderer über gemeinsamem UI-Core) |

## Neuerstellte Spezifikationen (diese Session)

### 1. `schema.allium`

**Zweck:** Vollständige Inventarisierung des SQLite-Schemas aus dem TypeScript-Projekt.

**Inhalt:**
- 22 Entity-Definitionen mit allen Feldern und Typen
- Alle Relationship-Tabellen (PostLink, PostMedia)
- Alle Metadata-Tabellen (Settings, GeneratedFileHashes)
- FTS5 Virtual Tables für Search (posts_fts, media_fts)
- AI/Chat-Tabellen (Conversations, Messages, Model Catalog)
- Embedding-Tabellen (USearch keys, dismissed duplicates)
- Import-Tabellen (WXR definitions)
- Notification-Tabellen (CLI-to-App sync)
- Alle Unique-Constraints und Indexe
- Migration-History (Version 0001-0010)

**Kritische Details:**
- `posts` Tabelle: 21 Felder inkl. Legacy-Felder
- `post_translations` Tabelle: 12 Felder
- `media` Tabelle: 17 Felder
- `media_translations` Tabelle: 6 Felder
- `scripts` Tabelle: 13 Felder (Lua in Rust)
- `templates` Tabelle: 13 Felder
- FTS5 benötigt Snowball Stemmer für 24 Sprachen
- Embedding vectors: 384-dimensional Float32 (1536 bytes)

### 2. `frontmatter.allium`

**Zweck:** Exakte Spezifikation aller YAML-Frontmatter-Formate.

**Inhalt:**
- **Post-Dateiformat:** `posts/{YYYY}/{MM}/{slug}.md`
  - Required fields: id, title, slug, status, createdAt, updatedAt, tags, categories
  - Conditional fields: excerpt, author, language, templateSlug, publishedAt
  - `doNotTranslate` nur wenn true
  
- **Translation-Dateiformat:** `posts/{YYYY}/{MM}/{slug}.{language}.md`
  - Gleiche Struktur wie Posts mit language-Override
  
- **Media-Sidecar-Format:** `media/{id}.md`
  - Required: id, filename, originalName, mimeType, size, createdAt, updatedAt, tags
  - Optional: title, alt, caption, author, language, width, height
  
- **Template-Format:** `templates/{slug}.liquid`
  - Required: id, slug, title, kind, enabled, version, createdAt, updatedAt
  
- **Script-Format:** `scripts/{slug}.lua` (Rust) / `{slug}.py` (TypeScript)
  - Required: id, slug, title, kind, entrypoint, enabled, version, createdAt, updatedAt
  
- **Tags-Dateiformat:** `meta/tags.json`
  - Sortiertes JSON-Array ohne interne IDs
  
- **Projekt-Metadaten:**
  - `meta/project.json` - Projekt-Konfiguration
  - `meta/categories.json` - Kategorie-Liste
  - `meta/category-meta.json` - Render-Einstellungen pro Kategorie
  - `meta/publishing.json` - SSH-Konfiguration
  - `meta/menu.opml` - Navigation im OPML-Format

**Format-Konventionen:**
- Timestamps als Unix-Milliseconds (ISO 8601 in YAML)
- 2-Space Indentation
- Arrays als YAML-Liste
- Booleans als lowercase true/false
- Atomare Writes (temp file + rename)

### 3. `template_context.allium`

**Zweck:** Vollständige Spezifikation des an Liquid-Templates übergebenen Datenkontexts.

**Inhalt:**
- **Global Render Context:** 30+ Top-Level-Variablen
  - `language`, `language_prefix`, `html_theme_attribute`
  - `blog_languages` (List<BlogLanguage>)
  - `alternate_links` (hreflang für SEO)
  - `menu_items` (hierarchische Struktur)
  - `post` (PostContext für Single-Post-Pages)
  - `day_blocks` (für Archiv-Seiten)
  - `canonical_post_path_by_slug` (Lookup-Map)
  - `post_data_json_by_id` (Lookup-Map)
  
- **PostContext:** 18 Felder inkl. linked_media, outgoing_links, incoming_links
  
- **MediaContext:** 11 Felder für Media in Templates
  
- **PaginationContext:** 10 Felder für Paginierung
  
- **Liquid-Filters:**
  - Built-in: `default`, `escape`, `url_encode`, `append`
  - Custom: `i18n` (Übersetzungs-Lookup)
  - Custom: `markdown` (Markdown→HTML mit Macro-Expansion)
  
- **Built-in Macros:**
  - `gallery` - Bildergalerie
  - `youtube` - YouTube-Einbettung
  - `vimeo` - Vimeo-Einbettung
  - `photo_archive` - Foto-Archiv-Grid
  - `tag_cloud` - Tag-Cloud mit Größen-Faktoren
  
- **Template-Lookup-Regeln:**
  - Priority: post-specific → tag-specific → category-specific → default
  - Partials via `{% render 'partial' %}`

### 4. `media_processing.allium`

**Zweck:** Exakte Spezifikation der Media-Verarbeitung (Thumbnails, Format-Konversion, EXIF).

**Inhalt:**
- **Datei-Organisation:**
  - Binär: `media/{timestamp}_{random}.{ext}`
  - Sidecar: `media/{id}.md`
  - Thumbnail: `thumbnails/{id}.webp`
  - Thumbnail-Source: `thumbnails/{id}_source.{ext}`
  
- **Thumbnail-Konfiguration:**
  - Größe: 400x300 (default)
  - Fit: "cover" (crop to fill)
  - Qualität: 80% WEBP
  - Format: WEBP
  
- **Bildverarbeitung:**
  - Input-Formate: JPEG, PNG, GIF, WEBP, TIFF, BMP, HEIC, HEIF
  - EXIF-Orientierung muss berücksichtigt werden
  - EXIF wird aus Thumbnails entfernt (Privacy)
  - Original-Format bleibt erhalten
  
- **Import-Flow:**
  1. Validiere Dateityp
  2. Generiere eindeutigen Dateinamen
  3. Kopiere nach media/
  4. Generiere Thumbnail
  5. Erstelle Sidecar mit Metadaten
  6. Indexiere für Search (FTS5)
  7. Generiere Embedding (wenn aktiviert)
  
- **Media-Übersetzungen:**
  - Dateipfad: `media/{id}/{language}.md`
  - Felder: title, alt, caption pro Sprache
  
- **Validierungs-Regeln:**
  - Fehlende Binary-Dateien
  - Fehlende Sidecar-Dateien
  - Fehlende Thumbnails
  - Korrupte Bilddateien
  - Orphan Media (nicht verlinkt)

## Verfügbare Rust-Plan-Dokumente

| Datei | Beschreibung |
|-------|--------------|
| `RUST_PLAN_CORE.md` | Core-Umfang mit aktuellem Implementierungsstand und offenen Punkten |
| `RUST_PLAN_EXTENSION.md` | Extensions mit aktuellem Implementierungsstand und offenen Punkten |

## Spezifikations-Abdeckungs-Analyse

### Vollständig Abgedeckt ✅

1. **Database Schema** - `schema.allium` deckt alle 22 Tabellen mit allen Feldern
2. **File Formats** - `frontmatter.allium` deckt alle Dateitypen und Formate
3. **Post Lifecycle** - `post.allium` + `translation.allium` vollständig
4. **Media Processing** - `media.allium` + `media_processing.allium` vollständig
5. **Template System** - `template.allium` + `template_context.allium` vollständig
6. **Search** - `search.allium` mit FTS5 und Snowball Stemming
7. **Generation** - `generation.allium` mit allen Section-Typen
8. **Publishing** - `publishing.allium` mit SCP und rsync
9. **AI Integration** - `ai.allium` mit One-Shot Operations
10. **i18n** - `i18n.allium` mit Split Localization

### Spezifikations-Lücken vor dieser Session ❌

1. **Schema nicht dokumentiert** - Nur aus TypeScript-Code lesbar
2. **Frontmatter-Regeln nicht explizit** - Feld-Logik verstreut in Engine-Code
3. **Template-Kontext unvollständig** - Variable-Liste existierte nicht zentral
4. **Media-Processing-Regeln implizit** - Thumbnail-Größen, EXIF-Handling nicht spezifiziert

### Status nach dieser Session ✅

Alle kritischen Lücken geschlossen:
- ✅ Vollständiges SQLite-Schema inventarisiert
- ✅ Alle Frontmatter-Formate spezifiziert
- ✅ Template-Kontext vollständig dokumentiert
- ✅ Media-Processing-Regeln explizit gemacht

## Nächste Schritte für die Implementierung

### Wave 0 (Foundation)
1. Cargo-Workspace aufsetzen (bds-core, bds-editor, bds-ui, bds-cli)
2. SQLite-Connection mit Diesel (gebündeltes SQLite)
3. Eingebettete Diesel-Migrationen
4. bds-editor PoC (ropey + syntect + cosmic-text)
5. Iced App Shell mit muda-Menüs
6. Slug-Compatibility-Tests (deunicode vs transliteration)

### Wave 1 (Data Layer)
1. Alle Engines implementieren (Project, Post, Media, Tag, Meta, etc.)
2. Frontmatter-Parser/Writer für alle Dateitypen
3. Thumbnail-Generierung mit image-crate
4. FTS5-Index mit Snowball-Stemming
5. Metadata-Diff und Rebuild
6. Round-trip-Tests für alle Entity-Typen

### Wave 2 (Native Shell)
1. Muda-Menü-Bar mit allen Menüs
2. rfd-Datei-Dialoge
3. Iced-Message-Routing
4. macOS Lifecycle-Shim (objc2)
5. Tab-Management und Workspace-Layout

### Wave 3 (Authoring UI)
1. Post-Editor mit bds-editor
2. Media-Browser und Editor
3. Template-Editor
4. Script-Editor
5. Settings-View
6. Tag/Category-Management

### Wave 4 (Rendering)
1. Markdown-Render mit pulldown-cmark
2. Liquid-Template-Engine (subset)
3. Built-in Macros (gallery, youtube, vimeo, etc.)
4. Preview-Server (axum)
5. Site-Generation mit rayon-Parallelisierung
6. One-Shot AI Operations (reqwest)
7. Pagefind-Search-Index

### Wave 5 (Publishing)
1. SSH/SCP Upload mit ssh2-crate
2. rsync-Integration
3. Publish-Progress-UI
4. Validierung vor Publish

### Wave 6 (Lua Scripting)
1. Lua-Runtime mit mlua
2. Lua-API-Bridge
3. Script-Execution
4. Generated API-Documentation

## Referenz-Implementierung

Für Implementierungsdetails immer referenzieren:
- **TypeScript-Code:** `/Users/gb/Projects/bDS/src/main/engine/`
- **Spezifikationen:** `/Users/gb/Projects/RuDS/specs/`
- **Pläne:** `/Users/gb/Projects/RuDS/RUST_*.md`

## Kompatibilitäts-Garantien

Die Rust-Implementierung muss garantieren:

1. **Datenbank-Kompatibilität** - Alle TypeScript-Daten lesbar
2. **Datei-Format-Kompatibilität** - Frontmatter byte-genau gleich
3. **Generierungs-Kompatibilität** - Output-Hashes übereinstimmend (normalisiert)
4. **Slug-Kompatibilität** - deunicode Output muss transliteration Output entsprechen
5. **URL-Kompatibilität** - Gleiche Routes und Canonical URLs
6. **Such-Kompatibilität** - FTS5 mit identischem Stemming
7. **Template-Kompatibilität** - Identisches Liquid-Subset

## Spezifikations-Quellen

Alle Spezifikationen wurden extrahiert aus:

- TypeScript-Engine-Code: `/Users/gb/Projects/bDS/src/main/engine/*.ts`
- TypeScript-Schema: `/Users/gb/Projects/bDS/src/main/database/schema.ts`
- TypeScript-Tests: `/Users/gb/Projects/bDS/tests/engine/*.test.ts`
- Bestehende Allium-Specs: `/Users/gb/Projects/RuDS/specs/*.allium`
