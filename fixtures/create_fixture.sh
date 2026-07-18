#!/bin/bash
# Create a small fixture DB and file tree from the real rfc1437 bDS project.
# Run once to populate fixtures/compatibility-projects/rfc1437-sample/
set -euo pipefail

SRC_DB="$HOME/Library/Application Support/Blogging Desktop Server/bds.db"
BLOG="/Users/gb/Blogs/rfc1437.de"
FIXTURE_DIR="$(dirname "$0")/compatibility-projects/rfc1437-sample"
FIXTURE_DB="$FIXTURE_DIR/bds.db"

PROJ_ID="1979237c-034d-41f6-99a0-f35eb57b3f6c"

# Post IDs to extract
POST_ESMERALDA="40a83ab1-423d-4310-aac4-642d84675007"
POST_GHOSTTY="6745981d-da41-4cfd-80ec-95ad339acf6f"
POST_CMUX="2665bfaa-8251-468d-a710-a4cf34dd81e2"  # post_link target from ghostty

# Media for esmeralda
MEDIA_ESMERALDA="eb0cf9d7-6fbd-4b74-9be3-759d6e16f240"

rm -rf "$FIXTURE_DIR"
mkdir -p "$FIXTURE_DIR"

# ---- Create fixture DB: dump only CREATE TABLE/INDEX from real DB, skip internals ----
sqlite3 "$FIXTURE_DB" "PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;"

sqlite3 "$SRC_DB" ".schema" \
    | grep -v "sqlite_sequence" \
    | grep -v "^CREATE TABLE IF NOT EXISTS \"refinery_schema_history\"" \
    | sed '/^CREATE TABLE __diesel_schema_migrations/,/^);$/d' \
    | sed '/^CREATE TABLE refinery_schema_history/,/^);$/d' \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract project ----
sqlite3 "$SRC_DB" ".mode insert projects" ".headers off" \
    "SELECT * FROM projects WHERE id = '$PROJ_ID';" | sqlite3 "$FIXTURE_DB"

# ---- Extract 3 published posts + the cmux post_link target ----
for PID in "$POST_ESMERALDA" "$POST_GHOSTTY" "$POST_CMUX"; do
    sqlite3 "$SRC_DB" ".mode insert posts" ".headers off" \
        "SELECT * FROM posts WHERE id = '$PID';" | sqlite3 "$FIXTURE_DB"
done

# ---- Insert a synthetic draft post (content in DB, no file) ----
sqlite3 "$FIXTURE_DB" "
INSERT INTO posts (id, project_id, title, slug, excerpt, content, status, author,
    created_at, updated_at, published_at, file_path, checksum, tags, categories,
    template_slug, language, do_not_translate)
VALUES (
    'aaaaaaaa-bbbb-cccc-dddd-eeeeeeeeeeee',
    '$PROJ_ID',
    'Draft Fixture Post',
    'draft-fixture-post',
    'A test draft post for fixture testing',
    'This is the **body** of a draft post.\n\nIt has multiple paragraphs and [a link](/somewhere).',
    'draft',
    'fixture-author',
    1700000000, 1700000001, NULL,
    '', NULL,
    '[\"test\",\"fixture\"]',
    '[\"tech\"]',
    NULL, 'de', 0
);
"

# ---- Extract translations for selected posts ----
sqlite3 "$SRC_DB" ".mode insert post_translations" ".headers off" \
    "SELECT * FROM post_translations WHERE translation_for IN ('$POST_ESMERALDA', '$POST_GHOSTTY', '$POST_CMUX');" \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract media (esmeralda's 1 item) ----
sqlite3 "$SRC_DB" ".mode insert media" ".headers off" \
    "SELECT * FROM media WHERE id = '$MEDIA_ESMERALDA';" | sqlite3 "$FIXTURE_DB"

# ---- Extract media_translations for that media ----
sqlite3 "$SRC_DB" ".mode insert media_translations" ".headers off" \
    "SELECT * FROM media_translations WHERE translation_for = '$MEDIA_ESMERALDA';" \
    | sqlite3 "$FIXTURE_DB" 2>/dev/null || true

# ---- Extract post_media links ----
sqlite3 "$SRC_DB" ".mode insert post_media" ".headers off" \
    "SELECT * FROM post_media WHERE post_id IN ('$POST_ESMERALDA', '$POST_GHOSTTY', '$POST_CMUX');" \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract post_links (ghostty -> cmux) ----
sqlite3 "$SRC_DB" ".mode insert post_links" ".headers off" \
    "SELECT * FROM post_links WHERE source_post_id = '$POST_GHOSTTY';" \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract 5 tags ----
sqlite3 "$SRC_DB" ".mode insert tags" ".headers off" \
    "SELECT * FROM tags WHERE project_id = '$PROJ_ID' AND name IN ('fotografie','programmierung','sysadmin','mac-os-x','natur');" \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract the 1 template ----
sqlite3 "$SRC_DB" ".mode insert templates" ".headers off" \
    "SELECT * FROM templates WHERE project_id = '$PROJ_ID';" | sqlite3 "$FIXTURE_DB"

# ---- Extract 2 scripts ----
sqlite3 "$SRC_DB" ".mode insert scripts" ".headers off" \
    "SELECT * FROM scripts WHERE project_id = '$PROJ_ID' AND slug IN ('test_script', 'bgg_link');" \
    | sqlite3 "$FIXTURE_DB"

# ---- Extract a couple of settings ----
sqlite3 "$SRC_DB" ".mode insert settings" ".headers off" \
    "SELECT * FROM settings LIMIT 5;" | sqlite3 "$FIXTURE_DB"

# ---- Extract 1 ai_provider + 1 ai_model (to test those tables read) ----
sqlite3 "$SRC_DB" ".mode insert ai_providers" ".headers off" \
    "SELECT * FROM ai_providers LIMIT 1;" | sqlite3 "$FIXTURE_DB"
sqlite3 "$SRC_DB" ".mode insert ai_models" ".headers off" \
    "SELECT * FROM ai_models LIMIT 1;" | sqlite3 "$FIXTURE_DB"
sqlite3 "$SRC_DB" ".mode insert ai_catalog_meta" ".headers off" \
    "SELECT * FROM ai_catalog_meta;" | sqlite3 "$FIXTURE_DB"

# ==================================================================
# Copy filesystem files (posts, sidecars, templates, scripts, meta)
# NO binary media files — only .meta sidecars and .md text files
# ==================================================================

# Posts
for F in \
    posts/2005/11/esmeralda.md \
    posts/2005/11/esmeralda.en.md \
    posts/2005/11/esmeralda.de.md \
    posts/2026/03/ghostty.md \
    posts/2026/03/ghostty.en.md \
    posts/2026/03/cmux-das-terminal-fur-multitasking.md \
    posts/2026/03/cmux-das-terminal-fur-multitasking.en.md \
; do
    if [ -f "$BLOG/$F" ]; then
        mkdir -p "$FIXTURE_DIR/$(dirname "$F")"
        cp "$BLOG/$F" "$FIXTURE_DIR/$F"
    fi
done

# Media sidecars only (not binaries)
for F in \
    media/2005/11/eb0cf9d7-6fbd-4b74-9be3-759d6e16f240.jpg.meta \
; do
    if [ -f "$BLOG/$F" ]; then
        mkdir -p "$FIXTURE_DIR/$(dirname "$F")"
        cp "$BLOG/$F" "$FIXTURE_DIR/$F"
    fi
done

# Template
mkdir -p "$FIXTURE_DIR/templates"
cp "$BLOG/templates/testvorlage.liquid" "$FIXTURE_DIR/templates/" 2>/dev/null || true

# Scripts (2)
mkdir -p "$FIXTURE_DIR/scripts"
cp "$BLOG/scripts/test_script.py" "$FIXTURE_DIR/scripts/" 2>/dev/null || true
cp "$BLOG/scripts/bgg_link.py" "$FIXTURE_DIR/scripts/" 2>/dev/null || true

# Meta JSON files
mkdir -p "$FIXTURE_DIR/meta"
for F in project.json categories.json category-meta.json publishing.json tags.json; do
    cp "$BLOG/meta/$F" "$FIXTURE_DIR/meta/" 2>/dev/null || true
done

# Menu OPML
cp "$BLOG/meta/menu.opml" "$FIXTURE_DIR/meta/" 2>/dev/null || true

echo "=== Fixture DB row counts ==="
sqlite3 "$FIXTURE_DB" "
SELECT 'projects', COUNT(*) FROM projects
UNION ALL SELECT 'posts', COUNT(*) FROM posts
UNION ALL SELECT 'post_translations', COUNT(*) FROM post_translations
UNION ALL SELECT 'media', COUNT(*) FROM media
UNION ALL SELECT 'media_translations', COUNT(*) FROM media_translations
UNION ALL SELECT 'tags', COUNT(*) FROM tags
UNION ALL SELECT 'templates', COUNT(*) FROM templates
UNION ALL SELECT 'scripts', COUNT(*) FROM scripts
UNION ALL SELECT 'post_links', COUNT(*) FROM post_links
UNION ALL SELECT 'post_media', COUNT(*) FROM post_media
UNION ALL SELECT 'settings', COUNT(*) FROM settings
UNION ALL SELECT 'ai_providers', COUNT(*) FROM ai_providers
UNION ALL SELECT 'ai_models', COUNT(*) FROM ai_models
UNION ALL SELECT 'ai_catalog_meta', COUNT(*) FROM ai_catalog_meta;
"

echo ""
echo "=== Fixture files ==="
find "$FIXTURE_DIR" -type f | sort | sed "s|$FIXTURE_DIR/||"

echo ""
echo "=== DB size ==="
ls -lh "$FIXTURE_DB"
