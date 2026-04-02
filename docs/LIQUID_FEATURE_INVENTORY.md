# bDS Liquid Feature Inventory

Inventoried from the 12 default templates in the TypeScript app (`src/main/engine/templates/`).

## Template Files Analyzed

1. `single-post.liquid` (main)
2. `post-list.liquid` (main)
3. `not-found.liquid` (main)
4. `macros/gallery.liquid`
5. `macros/youtube.liquid`
6. `macros/vimeo.liquid`
7. `macros/photo-archive.liquid`
8. `macros/tag-cloud.liquid`
9. `partials/head.liquid`
10. `partials/menu.liquid`
11. `partials/menu-items.liquid`
12. `partials/language-switcher.liquid`

## Tags Used

| Tag | Used In |
|---|---|
| `if` / `endif` | 9 templates |
| `elsif` | post-list |
| `else` | 7 templates |
| `for` / `endfor` | 7 templates |
| `assign` | 4 templates (not-found, post-list, head, single-post) |
| `render` | 5 templates (menu, menu-items, not-found, post-list, single-post) |

### NOT Used

`unless`, `case/when`, `capture`, `layout`, `include`, `comment`, `raw`, `increment`, `decrement`, `tablerow`, `cycle`

## Filters Used

| Filter | Type | Signature |
|---|---|---|
| `escape` | Built-in | `\| escape` |
| `default` | Built-in | `\| default: value` |
| `append` | Built-in | `\| append: string` |
| `url_encode` | Built-in | `\| url_encode` |
| `i18n` | **Custom** | `\| i18n: language` — translation lookup by key and language |
| `markdown` | **Custom** | `\| markdown: post.id, post_data_json_by_id, canonical_post_path_by_slug, canonical_media_path_by_source_path, language, language_prefix` — markdown-to-HTML with macro expansion and link resolution (6 args) |

### NOT Used

`date`, `truncate`, `split`, `join`, `where`, `group_by`, `map`, `sort`, `reverse`, `size` (as pipe filter), `strip`, `strip_html`, `downcase`, `upcase`, `replace`, `remove`, `first`, `last`, `abs`, `ceil`, `floor`, `round`, `plus`, `minus`, `times`, `divided_by`, `modulo`

## Operators

| Operator | Example |
|---|---|
| `==` | `item.href == '#'`, `archive_context.kind == 'tag'` |
| `>` | `menu_items.size > 0`, `blog_languages.size > 1` |
| `and` | `menu_items and menu_items.size > 0` |
| `or` | `archive_context.kind == 'tag' or archive_context.kind == 'category'` |
| `blank` | `canonical_post_href == blank` (nil/empty check) |
| bare truthiness | `{% if html_theme_attribute %}`, `{% if caption %}` |

### NOT Used

`!=`, `<`, `<=`, `>=`, `contains`

## Property Access Patterns

| Pattern | Examples |
|---|---|
| Dot notation | `archive_context.kind`, `post.title`, `item.media_path`, `lang.is_current` |
| `.size` property | `menu_items.size`, `items.size`, `post_categories.size` |
| Bracket notation (hash/map lookup) | `canonical_post_path_by_slug[post.slug]`, `tag_color_by_name[tag]` |

## Whitespace Stripping

`{%- -%}` used in 3 macro templates only: `photo-archive`, `gallery`, `tag-cloud`.

## For-Loop Features

| Feature | Used? |
|---|---|
| Basic `for x in collection` | YES |
| Nested `for` loops | YES (photo-archive, post-list) |
| Recursive `render` in loop | YES (menu-items renders itself for children) |
| `forloop.first` / `forloop.last` | NO |
| `limit` / `offset` | NO |
| `reversed` | NO |

## Render Tag Usage

- Uses named parameter passing: `{% render 'partials/menu-items', items: menu_items, include_calendar: true, language: language %}`
- Recursive self-render: menu-items calls `render 'partials/menu-items'` for nested children
- Partial paths use forward-slash notation: `'partials/head'`, `'partials/menu'`

## Two-Step Dynamic i18n Keys

`{% assign month_key = 'render.month.' | append: archive_context.month %}` then `{{ month_key | i18n: language }}`

## Scope Summary

The Rust Liquid implementation needs approximately **35% of the full specification**:

- **Tags**: `if`/`elsif`/`else`, `for`, `assign`, `render` (not `include`)
- **Built-in filters**: `escape`, `default`, `append`, `url_encode`
- **Custom filters**: `i18n` (key + language), `markdown` (content + 6 args)
- **Operators**: `==`, `>`, `and`, `or`, truthiness, `blank`
- **Access**: dot notation, `.size` property, bracket notation for hash/map lookups
- **Whitespace stripping**: `{%- -%}` support required
