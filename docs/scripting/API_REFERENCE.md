# RuDS Lua API Reference

Contract version: `0.4.0`

The `bds` global is the supported bridge from sandboxed Lua scripts to RuDS. The unmarked method signatures are identical to bDS2 so the same published script file can run in either application. Calls are synchronous, JSON-compatible values cross the bridge as Lua tables, and project-scoped methods always use the active project.

## Usage

A utility script exposes `main(input)` and calls the API through `bds`:

```lua
function main(input)
  local posts = bds.posts.get_all()
  bds.app.log("Found " .. #posts .. " posts")
  return posts
end
```

Macro scripts expose `render(input, context)` and transform scripts expose `main(input, context)`. Complete runnable files are in [`examples/`](examples/). Scripts cannot access the network, filesystem, processes, environment variables, or native Lua modules directly; use the documented host methods instead. Host failures return `nil` or `false` where the signature permits it.

## Conventions

- A parameter ending in `?` is optional.
- `T | nil` means the call can return no value. Check for `nil` before using it.
- `T[]` is a one-based Lua array.
- Public records are documented in [Lua API Types](TYPES.md).
- Dates and timestamps returned by RuDS records are ISO-8601 strings.
- Methods marked **RuDS extension** are not available to scripts running under bDS2.

## Contents

- [Root helpers](#root-helpers)
- [`bds.app`](#bdsapp)
- [`bds.chat`](#bdschat)
- [`bds.embeddings`](#bdsembeddings)
- [`bds.media`](#bdsmedia)
- [`bds.meta`](#bdsmeta)
- [`bds.posts`](#bdsposts)
- [`bds.projects`](#bdsprojects)
- [`bds.publish`](#bdspublish)
- [`bds.scripts`](#bdsscripts)
- [`bds.tags`](#bdstags)
- [`bds.tasks`](#bdstasks)
- [`bds.templates`](#bdstemplates)
- [Public data types](TYPES.md)

## Root helpers

### `bds.report_progress`

Report progress for the current managed job.

> **RuDS extension:** this helper is not part of the portable bDS2 API.

**Signature**

```text
bds.report_progress(payload: table) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `payload` | `table` | Yes | `{ current = 1, total = 10, message = "Working" }` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.report_progress({ current = 1, total = 10, message = "Working" })
```

**Example response**

```lua
true
```

## `bds.app`

### `bds.app.copy_to_clipboard`

Copy text to the system clipboard.

**Signature**

```text
bds.app.copy_to_clipboard(text: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `text` | `string` | Yes | `"Example content"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.copy_to_clipboard("Example content")
```

**Example response**

```lua
true
```

### `bds.app.get_data_paths`

Return filesystem paths for the current application and project data.

**Signature**

```text
bds.app.get_data_paths() -> table
```

**Parameters**

None.

**Returns**

`table`.

**Example call**

```lua
local result = bds.app.get_data_paths()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.app.get_blogmark_bookmarklet`

Return the Blogmark bookmarklet JavaScript source.

**Signature**

```text
bds.app.get_blogmark_bookmarklet() -> string
```

**Parameters**

None.

**Returns**

`string`.

**Example call**

```lua
local result = bds.app.get_blogmark_bookmarklet()
```

**Example response**

```lua
"example"
```

### `bds.app.get_system_language`

Return the current UI locale (the server-side UI language setting, falling back to the OS locale when unset).

**Signature**

```text
bds.app.get_system_language() -> string | nil
```

**Parameters**

None.

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.get_system_language()
```

**Example response**

```lua
"example"
```

### `bds.app.get_default_project_path`

Return the current project's filesystem path.

**Signature**

```text
bds.app.get_default_project_path() -> string | nil
```

**Parameters**

None.

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.get_default_project_path()
```

**Example response**

```lua
"example"
```

### `bds.app.get_title_bar_metrics`

Return desktop title bar inset metrics when available.

**Signature**

```text
bds.app.get_title_bar_metrics() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.get_title_bar_metrics()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.app.log`

Append a line to the script output stream. Multiple arguments are joined with spaces. Output appears in the desktop app's Output panel (and on stdout in the CLI); Lua's global `print` is routed the same way.

**Signature**

```text
bds.app.log(text: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `text` | `string` | Yes | `"Example content"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.log("Example content")
```

**Example response**

```lua
true
```

### `bds.app.notify_renderer_ready`

Notify the host application that the renderer is ready.

**Signature**

```text
bds.app.notify_renderer_ready() -> boolean
```

**Parameters**

None.

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.notify_renderer_ready()
```

**Example response**

```lua
true
```

### `bds.app.open_folder`

Open a folder in the system file manager.

**Signature**

```text
bds.app.open_folder(folder_path: string) -> string
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `folder_path` | `string` | Yes | `"/path/to/item"` |

**Returns**

`string`.

**Example call**

```lua
local result = bds.app.open_folder("/path/to/item")
```

**Example response**

```lua
"example"
```

### `bds.app.read_project_metadata`

Read project metadata from a project folder path.

**Signature**

```text
bds.app.read_project_metadata(folder_path: string) -> ProjectMetadata | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `folder_path` | `string` | Yes | `"/path/to/item"` |

**Returns**

`ProjectMetadata | nil`. See [`ProjectMetadata`](TYPES.md#projectmetadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.read_project_metadata("/path/to/item")
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.app.select_folder`

Show the native folder picker and return the chosen path.

**Signature**

```text
bds.app.select_folder(title?: string) -> string | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `title` | `string` | No | `"Example post"` |

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.select_folder("Example post")
```

**Example response**

```lua
"example"
```

### `bds.app.set_preview_post_target`

Set the current preview-post target used by desktop integrations.

**Signature**

```text
bds.app.set_preview_post_target(post_id?: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | No | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.set_preview_post_target("example-id")
```

**Example response**

```lua
true
```

### `bds.app.show_item_in_folder`

Reveal a file or folder in the system file manager.

**Signature**

```text
bds.app.show_item_in_folder(item_path: string) -> nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `item_path` | `string` | Yes | `"/path/to/item"` |

**Returns**

`nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.show_item_in_folder("/path/to/item")
```

**Example response**

```lua
nil
```

### `bds.app.trigger_menu_action`

Trigger a native menu action by action id.

**Signature**

```text
bds.app.trigger_menu_action(action: string) -> nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `action` | `string` | Yes | `"new-post"` |

**Returns**

`nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.app.trigger_menu_action("new-post")
```

**Example response**

```lua
nil
```

### `bds.app.progress`

Report numeric progress for the current script execution, optionally including a total and a user-facing message.

> **RuDS extension:** this helper is not part of the portable bDS2 API.

**Signature**

```text
bds.app.progress(current: number, total?: number, message?: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `current` | `number` | Yes | `1` |
| `total` | `number` | No | `100` |
| `message` | `string` | No | `"Working"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.progress(1, 100, "Working")
```

**Example response**

```lua
true
```

### `bds.app.toast`

Show bounded user feedback from a script. Transform scripts are subject to the runtime toast budget.

> **RuDS extension:** this helper is not part of the portable bDS2 API.

**Signature**

```text
bds.app.toast(message: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `message` | `string` | Yes | `"Working"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.app.toast("Working")
```

**Example response**

```lua
true
```

## `bds.chat`

### `bds.chat.detect_post_language`

Detect the language of post title and content.

**Signature**

```text
bds.chat.detect_post_language(title: string, content: string) -> table
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `title` | `string` | Yes | `"Example post"` |
| `content` | `string` | Yes | `"Example content"` |

**Returns**

`table`.

**Example call**

```lua
local result = bds.chat.detect_post_language("Example post", "Example content")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.chat.analyze_post`

Analyze a post using the configured AI runtime.

**Signature**

```text
bds.chat.analyze_post(post_id: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.chat.analyze_post("example-id")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.chat.translate_post`

Translate a post and persist the translation.

**Signature**

```text
bds.chat.translate_post(post_id: string, language: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.chat.translate_post("example-id", "en")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.chat.analyze_media_image`

Analyze a media image using the configured AI runtime.

**Signature**

```text
bds.chat.analyze_media_image(media_id: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.chat.analyze_media_image("example-id")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.chat.detect_media_language`

Detect the language of media metadata.

**Signature**

```text
bds.chat.detect_media_language(title: string, alt?: string, caption?: string) -> table
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `title` | `string` | Yes | `"Example post"` |
| `alt` | `string` | No | `"A descriptive alternative"` |
| `caption` | `string` | No | `"Example caption"` |

**Returns**

`table`.

**Example call**

```lua
local result = bds.chat.detect_media_language("Example post", "A descriptive alternative", "Example caption")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.chat.translate_media_metadata`

Translate media metadata and persist the translation.

**Signature**

```text
bds.chat.translate_media_metadata(media_id: string, language: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.chat.translate_media_metadata("example-id", "en")
```

**Example response**

```lua
{ key = "value" }
```

## `bds.embeddings`

### `bds.embeddings.compute_similarities`

Compute similarity scores from one source post to target posts.

**Signature**

```text
bds.embeddings.compute_similarities(post_id: string, target_ids: table) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `target_ids` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.compute_similarities("example-id", { title = "Example" })
```

**Example response**

```lua
{ key = "value" }
```

### `bds.embeddings.dismiss_pair`

Dismiss a duplicate candidate pair.

**Signature**

```text
bds.embeddings.dismiss_pair(post_id_a: string, post_id_b: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id_a` | `string` | Yes | `"example"` |
| `post_id_b` | `string` | Yes | `"example"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.embeddings.dismiss_pair("example", "example")
```

**Example response**

```lua
true
```

### `bds.embeddings.find_duplicates`

Find duplicate post candidates for the current project.

**Signature**

```text
bds.embeddings.find_duplicates() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.find_duplicates()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.embeddings.find_similar`

Find posts similar to the given post id.

**Signature**

```text
bds.embeddings.find_similar(post_id: string, limit?: integer) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `limit` | `integer` | No | `nil` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.find_similar("example-id", nil)
```

**Example response**

```lua
{ key = "value" }
```

### `bds.embeddings.get_progress`

Get embedding index progress for the current project.

**Signature**

```text
bds.embeddings.get_progress() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.get_progress()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.embeddings.index_unindexed_posts`

Index posts missing embeddings for the current project.

**Signature**

```text
bds.embeddings.index_unindexed_posts() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.index_unindexed_posts()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.embeddings.suggest_tags`

Suggest tags for a post from semantic similarity.

**Signature**

```text
bds.embeddings.suggest_tags(post_id: string, exclude_tags?: table) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `exclude_tags` | `table` | No | `{ title = "Example" }` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.embeddings.suggest_tags("example-id", { title = "Example" })
```

**Example response**

```lua
{ key = "value" }
```

## `bds.media`

### `bds.media.delete_translation`

Delete a media translation by language.

**Signature**

```text
bds.media.delete_translation(media_id: string, language: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.media.delete_translation("example-id", "en")
```

**Example response**

```lua
true
```

### `bds.media.filter`

Filter media using year, month, tags, language, or date range fields.

**Signature**

```text
bds.media.filter(filters: table) -> MediaData[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `filters` | `table` | Yes | `{ status = "draft" }` |

**Returns**

`MediaData[]`. See [`MediaData`](TYPES.md#mediadata).

**Example call**

```lua
local result = bds.media.filter({ status = "draft" })
```

**Example response**

```lua
{
  {
    alt = "A descriptive alternative",
    caption = "Example caption",
    created_at = "2026-07-19T08:00:00Z",
    file_path = "/path/to/item",
    id = "example-id",
    mime_type = "image/png",
    original_name = "image.png",
    project_id = "example-id",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.media.import`

Import media into the current project.

**Signature**

```text
bds.media.import(data: table) -> MediaData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`MediaData | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.import({ title = "Example" })
```

**Example response**

```lua
{
  alt = "A descriptive alternative",
  caption = "Example caption",
  created_at = "2026-07-19T08:00:00Z",
  file_path = "/path/to/item",
  id = "example-id",
  mime_type = "image/png",
  original_name = "image.png",
  project_id = "example-id",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.media.get_by_year_month`

Get media counts grouped by year and month.

**Signature**

```text
bds.media.get_by_year_month() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.media.get_by_year_month()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.media.get_file_path`

Return the absolute file path for a media item.

**Signature**

```text
bds.media.get_file_path(media_id: string) -> string | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.get_file_path("example-id")
```

**Example response**

```lua
"example"
```

### `bds.media.update`

Update media metadata by id.

**Signature**

```text
bds.media.update(id: string, data: table) -> MediaData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`MediaData | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  alt = "A descriptive alternative",
  caption = "Example caption",
  created_at = "2026-07-19T08:00:00Z",
  file_path = "/path/to/item",
  id = "example-id",
  mime_type = "image/png",
  original_name = "image.png",
  project_id = "example-id",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.media.delete`

Delete a media item by id.

**Signature**

```text
bds.media.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.media.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.media.get`

Fetch one media item by id.

**Signature**

```text
bds.media.get(id: string) -> MediaData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`MediaData | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.get("example-id")
```

**Example response**

```lua
{
  alt = "A descriptive alternative",
  caption = "Example caption",
  created_at = "2026-07-19T08:00:00Z",
  file_path = "/path/to/item",
  id = "example-id",
  mime_type = "image/png",
  original_name = "image.png",
  project_id = "example-id",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.media.get_all`

Fetch all media in the current project.

**Signature**

```text
bds.media.get_all() -> MediaData[]
```

**Parameters**

None.

**Returns**

`MediaData[]`. See [`MediaData`](TYPES.md#mediadata).

**Example call**

```lua
local result = bds.media.get_all()
```

**Example response**

```lua
{
  {
    alt = "A descriptive alternative",
    caption = "Example caption",
    created_at = "2026-07-19T08:00:00Z",
    file_path = "/path/to/item",
    id = "example-id",
    mime_type = "image/png",
    original_name = "image.png",
    project_id = "example-id",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.media.get_tags`

Return tag names used by media in the current project.

**Signature**

```text
bds.media.get_tags() -> string[]
```

**Parameters**

None.

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.media.get_tags()
```

**Example response**

```lua
{ "example" }
```

### `bds.media.get_tags_with_counts`

Return media tags with usage counts.

**Signature**

```text
bds.media.get_tags_with_counts() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.media.get_tags_with_counts()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.media.get_thumbnail`

Return a media thumbnail as a data URL for the requested size.

**Signature**

```text
bds.media.get_thumbnail(media_id: string, size?: string) -> string | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `size` | `string` | No | `"small"` |

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.get_thumbnail("example-id", "small")
```

**Example response**

```lua
"example"
```

### `bds.media.get_translation`

Return one media translation by language.

**Signature**

```text
bds.media.get_translation(media_id: string, language: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.get_translation("example-id", "en")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.media.get_translations`

Return all translations for a media item.

**Signature**

```text
bds.media.get_translations(media_id: string) -> table[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.media.get_translations("example-id")
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.media.get_url`

Return the project-relative public URL path for a media item.

**Signature**

```text
bds.media.get_url(media_id: string) -> string | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.get_url("example-id")
```

**Example response**

```lua
"example"
```

### `bds.media.rebuild_from_files`

Rebuild media records from sidecar files on disk.

**Signature**

```text
bds.media.rebuild_from_files() -> MediaData[] | nil
```

**Parameters**

None.

**Returns**

`MediaData[] | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.rebuild_from_files()
```

**Example response**

```lua
{
  {
    alt = "A descriptive alternative",
    caption = "Example caption",
    created_at = "2026-07-19T08:00:00Z",
    file_path = "/path/to/item",
    id = "example-id",
    mime_type = "image/png",
    original_name = "image.png",
    project_id = "example-id",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.media.regenerate_missing_thumbnails`

Generate thumbnails for media items that are missing them.

**Signature**

```text
bds.media.regenerate_missing_thumbnails() -> table
```

**Parameters**

None.

**Returns**

`table`.

**Example call**

```lua
local result = bds.media.regenerate_missing_thumbnails()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.media.regenerate_thumbnails`

Regenerate all thumbnails for one media item.

**Signature**

```text
bds.media.regenerate_thumbnails(media_id: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.regenerate_thumbnails("example-id")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.media.reindex_text`

Reindex post and media search text for the current project.

**Signature**

```text
bds.media.reindex_text() -> boolean
```

**Parameters**

None.

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.media.reindex_text()
```

**Example response**

```lua
true
```

### `bds.media.replace_file`

Replace the binary file behind an existing media item.

**Signature**

```text
bds.media.replace_file(media_id: string, source_path: string) -> MediaData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `source_path` | `string` | Yes | `"/path/to/item"` |

**Returns**

`MediaData | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.replace_file("example-id", "/path/to/item")
```

**Example response**

```lua
{
  alt = "A descriptive alternative",
  caption = "Example caption",
  created_at = "2026-07-19T08:00:00Z",
  file_path = "/path/to/item",
  id = "example-id",
  mime_type = "image/png",
  original_name = "image.png",
  project_id = "example-id",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.media.search`

Search media by free-text query.

**Signature**

```text
bds.media.search(query: string) -> MediaData[] | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `query` | `string` | Yes | `"rust"` |

**Returns**

`MediaData[] | nil`. See [`MediaData`](TYPES.md#mediadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.search("rust")
```

**Example response**

```lua
{
  {
    alt = "A descriptive alternative",
    caption = "Example caption",
    created_at = "2026-07-19T08:00:00Z",
    file_path = "/path/to/item",
    id = "example-id",
    mime_type = "image/png",
    original_name = "image.png",
    project_id = "example-id",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.media.upsert_translation`

Create or update a media translation.

**Signature**

```text
bds.media.upsert_translation(media_id: string, language: string, data: table) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `media_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.media.upsert_translation("example-id", "en", { title = "Example" })
```

**Example response**

```lua
{ key = "value" }
```

## `bds.meta`

### `bds.meta.get_project_metadata`

Read metadata for the current project.

**Signature**

```text
bds.meta.get_project_metadata() -> ProjectMetadata
```

**Parameters**

None.

**Returns**

`ProjectMetadata`. See [`ProjectMetadata`](TYPES.md#projectmetadata).

**Example call**

```lua
local result = bds.meta.get_project_metadata()
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.meta.update_project_metadata`

Update metadata for the current project. Keys omitted from updates keep their current values.

**Signature**

```text
bds.meta.update_project_metadata(updates: table) -> ProjectMetadata | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `updates` | `table` | Yes | `{ description = "Updated description" }` |

**Returns**

`ProjectMetadata | nil`. See [`ProjectMetadata`](TYPES.md#projectmetadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.update_project_metadata({ description = "Updated description" })
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.meta.set_project_metadata`

Replace project metadata fields for the current project.

**Signature**

```text
bds.meta.set_project_metadata(updates: table) -> ProjectMetadata | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `updates` | `table` | Yes | `{ description = "Updated description" }` |

**Returns**

`ProjectMetadata | nil`. See [`ProjectMetadata`](TYPES.md#projectmetadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.set_project_metadata({ description = "Updated description" })
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.meta.add_category`

Add a category to the current project.

**Signature**

```text
bds.meta.add_category(name: string) -> ProjectMetadata | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `name` | `string` | Yes | `"Example"` |

**Returns**

`ProjectMetadata | nil`. See [`ProjectMetadata`](TYPES.md#projectmetadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.add_category("Example")
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.meta.remove_category`

Remove a category from the current project.

**Signature**

```text
bds.meta.remove_category(name: string) -> ProjectMetadata | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `name` | `string` | Yes | `"Example"` |

**Returns**

`ProjectMetadata | nil`. See [`ProjectMetadata`](TYPES.md#projectmetadata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.remove_category("Example")
```

**Example response**

```lua
{
  blog_languages = { "example" },
  categories = { "example" },
  default_author = "Ada Author",
  description = "example",
  main_language = "en",
  name = "Example",
  public_url = "https://example.com",
  publishing_preferences = { key = "value" },
}
```

### `bds.meta.add_tag`

Add a tag record to the current project if it does not already exist.

**Signature**

```text
bds.meta.add_tag(name: string) -> string[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `name` | `string` | Yes | `"Example"` |

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.meta.add_tag("Example")
```

**Example response**

```lua
{ "example" }
```

### `bds.meta.remove_tag`

Remove a tag record from the current project by name.

**Signature**

```text
bds.meta.remove_tag(name: string) -> string[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `name` | `string` | Yes | `"Example"` |

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.meta.remove_tag("Example")
```

**Example response**

```lua
{ "example" }
```

### `bds.meta.get_categories`

Get project categories.

**Signature**

```text
bds.meta.get_categories() -> string[]
```

**Parameters**

None.

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.meta.get_categories()
```

**Example response**

```lua
{ "example" }
```

### `bds.meta.get_tags`

Get tag names for the current project.

**Signature**

```text
bds.meta.get_tags() -> string[]
```

**Parameters**

None.

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.meta.get_tags()
```

**Example response**

```lua
{ "example" }
```

### `bds.meta.get_publishing_preferences`

Get publishing preferences for the current project.

**Signature**

```text
bds.meta.get_publishing_preferences() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.get_publishing_preferences()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.meta.set_publishing_preferences`

Set publishing preferences for the current project.

**Signature**

```text
bds.meta.set_publishing_preferences(prefs: table) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `prefs` | `table` | Yes | `{ publish_drafts = false }` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.set_publishing_preferences({ publish_drafts = false })
```

**Example response**

```lua
{ key = "value" }
```

### `bds.meta.clear_publishing_preferences`

Reset publishing preferences to defaults.

**Signature**

```text
bds.meta.clear_publishing_preferences() -> table | nil
```

**Parameters**

None.

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.meta.clear_publishing_preferences()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.meta.sync_on_startup`

Synchronize startup metadata state and return tags, categories, and project metadata.

**Signature**

```text
bds.meta.sync_on_startup() -> table
```

**Parameters**

None.

**Returns**

`table`.

**Example call**

```lua
local result = bds.meta.sync_on_startup()
```

**Example response**

```lua
{ key = "value" }
```

## `bds.posts`

### `bds.posts.create`

Create a post in the current project.

**Signature**

```text
bds.posts.create(data: table) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.create({ title = "Example" })
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.discard`

Discard unpublished post changes and restore the last published version from disk.

**Signature**

```text
bds.posts.discard(id: string) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.discard("example-id")
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.filter`

Filter posts using status, tags, categories, language, year, month, or date range fields.

**Signature**

```text
bds.posts.filter(filters: table) -> PostData[] | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `filters` | `table` | Yes | `{ status = "draft" }` |

**Returns**

`PostData[] | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.filter({ status = "draft" })
```

**Example response**

```lua
{
  {
    backlinks = { { key = "value" } },
    categories = { "example" },
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    language = "en",
    links_to = { { key = "value" } },
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.posts.generate_unique_slug`

Generate a unique slug from a title, optionally excluding one post id.

**Signature**

```text
bds.posts.generate_unique_slug(title: string, exclude_post_id?: string) -> string
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `title` | `string` | Yes | `"Example post"` |
| `exclude_post_id` | `string` | No | `"example-id"` |

**Returns**

`string`.

**Example call**

```lua
local result = bds.posts.generate_unique_slug("Example post", "example-id")
```

**Example response**

```lua
"example"
```

### `bds.posts.get_by_status`

Fetch posts filtered by a specific status.

**Signature**

```text
bds.posts.get_by_status(status: string) -> PostData[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `status` | `string` | Yes | `"draft"` |

**Returns**

`PostData[]`. See [`PostData`](TYPES.md#postdata).

**Example call**

```lua
local result = bds.posts.get_by_status("draft")
```

**Example response**

```lua
{
  {
    backlinks = { { key = "value" } },
    categories = { "example" },
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    language = "en",
    links_to = { { key = "value" } },
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.posts.get_by_year_month`

Get post counts grouped by year and month.

**Signature**

```text
bds.posts.get_by_year_month() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_by_year_month()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.get_dashboard_stats`

Return aggregate post dashboard counts for the current project.

**Signature**

```text
bds.posts.get_dashboard_stats() -> table
```

**Parameters**

None.

**Returns**

`table`.

**Example call**

```lua
local result = bds.posts.get_dashboard_stats()
```

**Example response**

```lua
{ key = "value" }
```

### `bds.posts.get_linked_by`

Return posts that link to the given post.

**Signature**

```text
bds.posts.get_linked_by(post_id: string) -> table[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_linked_by("example-id")
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.get_links_to`

Return posts linked from the given post.

**Signature**

```text
bds.posts.get_links_to(post_id: string) -> table[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_links_to("example-id")
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.get_preview_url`

Return the local preview URL for a post, optionally with draft and language query parameters.

**Signature**

```text
bds.posts.get_preview_url(post_id: string, options?: table) -> string | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `options` | `table` | No | `{ language = "en" }` |

**Returns**

`string | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.get_preview_url("example-id", { language = "en" })
```

**Example response**

```lua
"example"
```

### `bds.posts.update`

Update a post by id.

**Signature**

```text
bds.posts.update(id: string, data: table) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.delete`

Delete a post by id.

**Signature**

```text
bds.posts.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.posts.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.posts.get`

Fetch one post by id.

**Signature**

```text
bds.posts.get(id: string) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.get("example-id")
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.get_all`

Fetch all posts in the current project.

**Signature**

```text
bds.posts.get_all() -> PostData[]
```

**Parameters**

None.

**Returns**

`PostData[]`. See [`PostData`](TYPES.md#postdata).

**Example call**

```lua
local result = bds.posts.get_all()
```

**Example response**

```lua
{
  {
    backlinks = { { key = "value" } },
    categories = { "example" },
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    language = "en",
    links_to = { { key = "value" } },
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.posts.get_by_slug`

Fetch one post by slug.

**Signature**

```text
bds.posts.get_by_slug(slug: string) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `slug` | `string` | Yes | `"example-post"` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.get_by_slug("example-post")
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.get_categories`

Get category names used by posts in the current project.

**Signature**

```text
bds.posts.get_categories() -> string[]
```

**Parameters**

None.

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.posts.get_categories()
```

**Example response**

```lua
{ "example" }
```

### `bds.posts.get_categories_with_counts`

Get post categories with usage counts.

**Signature**

```text
bds.posts.get_categories_with_counts() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_categories_with_counts()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.get_tags`

Get tag names used by posts in the current project.

**Signature**

```text
bds.posts.get_tags() -> string[]
```

**Parameters**

None.

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.posts.get_tags()
```

**Example response**

```lua
{ "example" }
```

### `bds.posts.get_tags_with_counts`

Get post tags with usage counts.

**Signature**

```text
bds.posts.get_tags_with_counts() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_tags_with_counts()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.get_translation`

Get a single translation for a post by language.

**Signature**

```text
bds.posts.get_translation(post_id: string, language: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.get_translation("example-id", "en")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.posts.get_translations`

Get all translations for a post.

**Signature**

```text
bds.posts.get_translations(post_id: string) -> table[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.posts.get_translations("example-id")
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.posts.has_published_version`

Check whether a post has a published version.

**Signature**

```text
bds.posts.has_published_version(post_id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.posts.has_published_version("example-id")
```

**Example response**

```lua
true
```

### `bds.posts.is_slug_available`

Return whether a slug is available in the current project, optionally excluding one post id.

**Signature**

```text
bds.posts.is_slug_available(slug: string, exclude_post_id?: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `slug` | `string` | Yes | `"example-post"` |
| `exclude_post_id` | `string` | No | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.posts.is_slug_available("example-post", "example-id")
```

**Example response**

```lua
true
```

### `bds.posts.publish`

Publish a post by id.

**Signature**

```text
bds.posts.publish(id: string) -> PostData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`PostData | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.publish("example-id")
```

**Example response**

```lua
{
  backlinks = { { key = "value" } },
  categories = { "example" },
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  language = "en",
  links_to = { { key = "value" } },
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  tags = { "example" },
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.posts.publish_translation`

Publish one translation of a post by language.

**Signature**

```text
bds.posts.publish_translation(post_id: string, language: string) -> table | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `post_id` | `string` | Yes | `"example-id"` |
| `language` | `string` | Yes | `"en"` |

**Returns**

`table | nil`. `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.publish_translation("example-id", "en")
```

**Example response**

```lua
{ key = "value" }
```

### `bds.posts.rebuild_from_files`

Rebuild post records from published files.

**Signature**

```text
bds.posts.rebuild_from_files() -> PostData[] | nil
```

**Parameters**

None.

**Returns**

`PostData[] | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.rebuild_from_files()
```

**Example response**

```lua
{
  {
    backlinks = { { key = "value" } },
    categories = { "example" },
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    language = "en",
    links_to = { { key = "value" } },
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.posts.rebuild_links`

Rebuild the post link graph for the current project.

**Signature**

```text
bds.posts.rebuild_links() -> boolean
```

**Parameters**

None.

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.posts.rebuild_links()
```

**Example response**

```lua
true
```

### `bds.posts.reindex_text`

Reindex post and media search text for the current project.

**Signature**

```text
bds.posts.reindex_text() -> boolean
```

**Parameters**

None.

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.posts.reindex_text()
```

**Example response**

```lua
true
```

### `bds.posts.search`

Search posts by free-text query.

**Signature**

```text
bds.posts.search(query: string) -> PostData[] | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `query` | `string` | Yes | `"rust"` |

**Returns**

`PostData[] | nil`. See [`PostData`](TYPES.md#postdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.posts.search("rust")
```

**Example response**

```lua
{
  {
    backlinks = { { key = "value" } },
    categories = { "example" },
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    language = "en",
    links_to = { { key = "value" } },
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    tags = { "example" },
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

## `bds.projects`

### `bds.projects.create`

Create a project.

**Signature**

```text
bds.projects.create(data: table) -> ProjectData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`ProjectData | nil`. See [`ProjectData`](TYPES.md#projectdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.projects.create({ title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  data_path = "/path/to/item",
  description = "example",
  id = "example-id",
  is_active = true,
  name = "Example",
  slug = "example-post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.projects.delete`

Delete a project by id.

**Signature**

```text
bds.projects.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.projects.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.projects.delete_with_data`

Delete a project by id and remove its project directory.

**Signature**

```text
bds.projects.delete_with_data(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.projects.delete_with_data("example-id")
```

**Example response**

```lua
true
```

### `bds.projects.get`

Fetch one project by id.

**Signature**

```text
bds.projects.get(id: string) -> ProjectData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`ProjectData | nil`. See [`ProjectData`](TYPES.md#projectdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.projects.get("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  data_path = "/path/to/item",
  description = "example",
  id = "example-id",
  is_active = true,
  name = "Example",
  slug = "example-post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.projects.get_all`

Fetch all projects.

**Signature**

```text
bds.projects.get_all() -> ProjectData[]
```

**Parameters**

None.

**Returns**

`ProjectData[]`. See [`ProjectData`](TYPES.md#projectdata).

**Example call**

```lua
local result = bds.projects.get_all()
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    data_path = "/path/to/item",
    description = "example",
    id = "example-id",
    is_active = true,
    name = "Example",
    slug = "example-post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.projects.get_active`

Fetch the active project.

**Signature**

```text
bds.projects.get_active() -> ProjectData | nil
```

**Parameters**

None.

**Returns**

`ProjectData | nil`. See [`ProjectData`](TYPES.md#projectdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.projects.get_active()
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  data_path = "/path/to/item",
  description = "example",
  id = "example-id",
  is_active = true,
  name = "Example",
  slug = "example-post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.projects.set_active`

Set the active project by id.

**Signature**

```text
bds.projects.set_active(id: string) -> ProjectData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`ProjectData | nil`. See [`ProjectData`](TYPES.md#projectdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.projects.set_active("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  data_path = "/path/to/item",
  description = "example",
  id = "example-id",
  is_active = true,
  name = "Example",
  slug = "example-post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.projects.update`

Update a project by id.

**Signature**

```text
bds.projects.update(id: string, data: table) -> ProjectData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`ProjectData | nil`. See [`ProjectData`](TYPES.md#projectdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.projects.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  data_path = "/path/to/item",
  description = "example",
  id = "example-id",
  is_active = true,
  name = "Example",
  slug = "example-post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

## `bds.publish`

### `bds.publish.upload_site`

Upload the rendered site using the provided publishing credentials.

**Signature**

```text
bds.publish.upload_site(credentials: table) -> TaskData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `credentials` | `table` | Yes | `{ host = "example.com", username = "author" }` |

**Returns**

`TaskData | nil`. See [`TaskData`](TYPES.md#taskdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.publish.upload_site({ host = "example.com", username = "author" })
```

**Example response**

```lua
{
  id = "example-id",
  message = "Working",
  name = "Example",
  progress = 0.5,
  status = "draft",
}
```

## `bds.scripts`

### `bds.scripts.create`

Create a script in the current project.

**Signature**

```text
bds.scripts.create(data: table) -> ScriptData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`ScriptData | nil`. See [`ScriptData`](TYPES.md#scriptdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.scripts.create({ title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  entrypoint = "main",
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.scripts.update`

Update a script by id.

**Signature**

```text
bds.scripts.update(id: string, data: table) -> ScriptData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`ScriptData | nil`. See [`ScriptData`](TYPES.md#scriptdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.scripts.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  entrypoint = "main",
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.scripts.delete`

Delete a script by id.

**Signature**

```text
bds.scripts.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.scripts.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.scripts.get`

Fetch one script by id.

**Signature**

```text
bds.scripts.get(id: string) -> ScriptData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`ScriptData | nil`. See [`ScriptData`](TYPES.md#scriptdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.scripts.get("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  entrypoint = "main",
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.scripts.get_all`

Fetch all scripts in the current project.

**Signature**

```text
bds.scripts.get_all() -> ScriptData[]
```

**Parameters**

None.

**Returns**

`ScriptData[]`. See [`ScriptData`](TYPES.md#scriptdata).

**Example call**

```lua
local result = bds.scripts.get_all()
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    enabled = true,
    entrypoint = "main",
    id = "example-id",
    kind = "utility",
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.scripts.publish`

Publish a script by id.

**Signature**

```text
bds.scripts.publish(id: string) -> ScriptData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`ScriptData | nil`. See [`ScriptData`](TYPES.md#scriptdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.scripts.publish("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  entrypoint = "main",
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.scripts.rebuild_from_files`

Rebuild script records from published files.

**Signature**

```text
bds.scripts.rebuild_from_files() -> ScriptData[] | nil
```

**Parameters**

None.

**Returns**

`ScriptData[] | nil`. See [`ScriptData`](TYPES.md#scriptdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.scripts.rebuild_from_files()
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    enabled = true,
    entrypoint = "main",
    id = "example-id",
    kind = "utility",
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

## `bds.tags`

### `bds.tags.create`

Create a tag in the current project.

**Signature**

```text
bds.tags.create(data: table) -> TagData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`TagData | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.create({ title = "Example" })
```

**Example response**

```lua
{
  color = "#336699",
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  name = "Example",
  post_template_slug = "example-post",
  project_id = "example-id",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.tags.update`

Update a tag by id.

**Signature**

```text
bds.tags.update(id: string, data: table) -> TagData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`TagData | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  color = "#336699",
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  name = "Example",
  post_template_slug = "example-post",
  project_id = "example-id",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.tags.delete`

Delete a tag by id.

**Signature**

```text
bds.tags.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.tags.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.tags.get`

Fetch one tag by id.

**Signature**

```text
bds.tags.get(id: string) -> TagData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`TagData | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.get("example-id")
```

**Example response**

```lua
{
  color = "#336699",
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  name = "Example",
  post_template_slug = "example-post",
  project_id = "example-id",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.tags.get_all`

Fetch all tags in the current project.

**Signature**

```text
bds.tags.get_all() -> TagData[]
```

**Parameters**

None.

**Returns**

`TagData[]`. See [`TagData`](TYPES.md#tagdata).

**Example call**

```lua
local result = bds.tags.get_all()
```

**Example response**

```lua
{
  {
    color = "#336699",
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    name = "Example",
    post_template_slug = "example-post",
    project_id = "example-id",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.tags.get_by_name`

Fetch one tag by name.

**Signature**

```text
bds.tags.get_by_name(name: string) -> TagData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `name` | `string` | Yes | `"Example"` |

**Returns**

`TagData | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.get_by_name("Example")
```

**Example response**

```lua
{
  color = "#336699",
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  name = "Example",
  post_template_slug = "example-post",
  project_id = "example-id",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.tags.get_posts_with_tag`

Get post ids using a specific tag.

**Signature**

```text
bds.tags.get_posts_with_tag(tag_id: string) -> string[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `tag_id` | `string` | Yes | `"example-id"` |

**Returns**

`string[]`.

**Example call**

```lua
local result = bds.tags.get_posts_with_tag("example-id")
```

**Example response**

```lua
{ "example" }
```

### `bds.tags.get_with_counts`

Fetch tags with usage counts.

**Signature**

```text
bds.tags.get_with_counts() -> table[]
```

**Parameters**

None.

**Returns**

`table[]`.

**Example call**

```lua
local result = bds.tags.get_with_counts()
```

**Example response**

```lua
{ { key = "value" } }
```

### `bds.tags.merge`

Merge source tags into a target tag.

**Signature**

```text
bds.tags.merge(source_tag_ids: table, target_tag_id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `source_tag_ids` | `table` | Yes | `{ "tag-1", "tag-2" }` |
| `target_tag_id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.tags.merge({ "tag-1", "tag-2" }, "example-id")
```

**Example response**

```lua
true
```

### `bds.tags.rename`

Rename a tag by id.

**Signature**

```text
bds.tags.rename(id: string, new_name: string) -> TagData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `new_name` | `string` | Yes | `"Example"` |

**Returns**

`TagData | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.rename("example-id", "Example")
```

**Example response**

```lua
{
  color = "#336699",
  created_at = "2026-07-19T08:00:00Z",
  id = "example-id",
  name = "Example",
  post_template_slug = "example-post",
  project_id = "example-id",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.tags.sync_from_posts`

Sync tag records from post tags.

**Signature**

```text
bds.tags.sync_from_posts() -> TagData[] | nil
```

**Parameters**

None.

**Returns**

`TagData[] | nil`. See [`TagData`](TYPES.md#tagdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tags.sync_from_posts()
```

**Example response**

```lua
{
  {
    color = "#336699",
    created_at = "2026-07-19T08:00:00Z",
    id = "example-id",
    name = "Example",
    post_template_slug = "example-post",
    project_id = "example-id",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

## `bds.tasks`

### `bds.tasks.get`

Fetch one task by id.

**Signature**

```text
bds.tasks.get(id: string) -> TaskData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`TaskData | nil`. See [`TaskData`](TYPES.md#taskdata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.tasks.get("example-id")
```

**Example response**

```lua
{
  id = "example-id",
  message = "Working",
  name = "Example",
  progress = 0.5,
  status = "draft",
}
```

### `bds.tasks.status_snapshot`

Fetch the current task status snapshot.

**Signature**

```text
bds.tasks.status_snapshot() -> TaskStatus
```

**Parameters**

None.

**Returns**

`TaskStatus`. See [`TaskStatus`](TYPES.md#taskstatus).

**Example call**

```lua
local result = bds.tasks.status_snapshot()
```

**Example response**

```lua
{
  active_count = 1,
  pending_count = 1,
  running_count = 1,
  tasks = {
  {
    id = "example-id",
    message = "Working",
    name = "Example",
    progress = 0.5,
    status = "draft",
  }
},
}
```

### `bds.tasks.cancel`

Cancel a task by id.

**Signature**

```text
bds.tasks.cancel(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.tasks.cancel("example-id")
```

**Example response**

```lua
true
```

### `bds.tasks.get_all`

Fetch all tasks currently tracked by the task manager.

**Signature**

```text
bds.tasks.get_all() -> TaskData[]
```

**Parameters**

None.

**Returns**

`TaskData[]`. See [`TaskData`](TYPES.md#taskdata).

**Example call**

```lua
local result = bds.tasks.get_all()
```

**Example response**

```lua
{
  {
    id = "example-id",
    message = "Working",
    name = "Example",
    progress = 0.5,
    status = "draft",
  }
}
```

### `bds.tasks.get_running`

Fetch running tasks currently tracked by the task manager.

**Signature**

```text
bds.tasks.get_running() -> TaskData[]
```

**Parameters**

None.

**Returns**

`TaskData[]`. See [`TaskData`](TYPES.md#taskdata).

**Example call**

```lua
local result = bds.tasks.get_running()
```

**Example response**

```lua
{
  {
    id = "example-id",
    message = "Working",
    name = "Example",
    progress = 0.5,
    status = "draft",
  }
}
```

### `bds.tasks.clear_completed`

Clear completed tasks from the in-memory task list.

**Signature**

```text
bds.tasks.clear_completed() -> boolean
```

**Parameters**

None.

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.tasks.clear_completed()
```

**Example response**

```lua
true
```

## `bds.templates`

### `bds.templates.create`

Create a template in the current project.

**Signature**

```text
bds.templates.create(data: table) -> TemplateData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`TemplateData | nil`. See [`TemplateData`](TYPES.md#templatedata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.create({ title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.templates.update`

Update a template by id.

**Signature**

```text
bds.templates.update(id: string, data: table) -> TemplateData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |
| `data` | `table` | Yes | `{ title = "Example" }` |

**Returns**

`TemplateData | nil`. See [`TemplateData`](TYPES.md#templatedata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.update("example-id", { title = "Example" })
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.templates.delete`

Delete a template by id.

**Signature**

```text
bds.templates.delete(id: string) -> boolean
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`boolean`. `false` means the operation was rejected or failed.

**Example call**

```lua
local result = bds.templates.delete("example-id")
```

**Example response**

```lua
true
```

### `bds.templates.get`

Fetch one template by id.

**Signature**

```text
bds.templates.get(id: string) -> TemplateData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`TemplateData | nil`. See [`TemplateData`](TYPES.md#templatedata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.get("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.templates.get_all`

Fetch all templates in the current project.

**Signature**

```text
bds.templates.get_all() -> TemplateData[]
```

**Parameters**

None.

**Returns**

`TemplateData[]`. See [`TemplateData`](TYPES.md#templatedata).

**Example call**

```lua
local result = bds.templates.get_all()
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    enabled = true,
    id = "example-id",
    kind = "utility",
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.templates.publish`

Publish a template by id.

**Signature**

```text
bds.templates.publish(id: string) -> TemplateData | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `id` | `string` | Yes | `"example-id"` |

**Returns**

`TemplateData | nil`. See [`TemplateData`](TYPES.md#templatedata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.publish("example-id")
```

**Example response**

```lua
{
  created_at = "2026-07-19T08:00:00Z",
  enabled = true,
  id = "example-id",
  kind = "utility",
  project_id = "example-id",
  slug = "example-post",
  status = "draft",
  title = "Example post",
  updated_at = "2026-07-19T08:00:00Z",
}
```

### `bds.templates.get_enabled_by_kind`

Fetch enabled templates filtered by kind.

**Signature**

```text
bds.templates.get_enabled_by_kind(kind: string) -> TemplateData[]
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `kind` | `string` | Yes | `"utility"` |

**Returns**

`TemplateData[]`. See [`TemplateData`](TYPES.md#templatedata).

**Example call**

```lua
local result = bds.templates.get_enabled_by_kind("utility")
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    enabled = true,
    id = "example-id",
    kind = "utility",
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.templates.rebuild_from_files`

Rebuild template records from published files.

**Signature**

```text
bds.templates.rebuild_from_files() -> TemplateData[] | nil
```

**Parameters**

None.

**Returns**

`TemplateData[] | nil`. See [`TemplateData`](TYPES.md#templatedata). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.rebuild_from_files()
```

**Example response**

```lua
{
  {
    created_at = "2026-07-19T08:00:00Z",
    enabled = true,
    id = "example-id",
    kind = "utility",
    project_id = "example-id",
    slug = "example-post",
    status = "draft",
    title = "Example post",
    updated_at = "2026-07-19T08:00:00Z",
  }
}
```

### `bds.templates.validate`

Validate Liquid template syntax.

**Signature**

```text
bds.templates.validate(content: string) -> ValidationResult | nil
```

**Parameters**

| Name | Type | Required | Example |
| --- | --- | --- | --- |
| `content` | `string` | Yes | `"Example content"` |

**Returns**

`ValidationResult | nil`. See [`ValidationResult`](TYPES.md#validationresult). `nil` means no value was available or the host operation failed.

**Example call**

```lua
local result = bds.templates.validate("Example content")
```

**Example response**

```lua
{
  errors = { "example" },
  valid = true,
}
```
