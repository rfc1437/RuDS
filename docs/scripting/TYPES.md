# Lua API Types

Contract version: `0.4.0`

These are the public, JSON-compatible records returned by the Lua host API. They contain no database handles or private implementation fields.

## Value conventions

- `T | nil` marks an optional field.
- `T[]` is a one-based Lua array.
- `table` is a JSON-compatible Lua table whose shape depends on the operation.
- ISO-8601 values are strings such as `2026-07-19T08:00:00Z`.

## Contents

- [`ProjectData`](#projectdata)
- [`ProjectMetadata`](#projectmetadata)
- [`PostData`](#postdata)
- [`MediaData`](#mediadata)
- [`ScriptData`](#scriptdata)
- [`TemplateData`](#templatedata)
- [`TagData`](#tagdata)
- [`TaskData`](#taskdata)
- [`TaskStatus`](#taskstatus)
- [`GitProviderData`](#gitproviderdata)
- [`GitRepositoryState`](#gitrepositorystate)
- [`GitFileStatus`](#gitfilestatus)
- [`GitStatusResult`](#gitstatusresult)
- [`GitSyncStatus`](#gitsyncstatus)
- [`GitCommitData`](#gitcommitdata)
- [`GitHistoryResult`](#githistoryresult)
- [`GitNetworkResult`](#gitnetworkresult)
- [`GitCommitResult`](#gitcommitresult)
- [`ValidationResult`](#validationresult)

## `ProjectData`

Project record stored in the application database.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `data_path` | `string \| nil` | No | Filesystem path containing project data. |
| `description` | `string \| nil` | No | Human-readable description. |
| `id` | `string` | Yes | Stable record identifier. |
| `is_active` | `boolean` | Yes | Whether this is the active project. |
| `name` | `string` | Yes | Human-readable name. |
| `slug` | `string` | Yes | URL-safe record identifier. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `ProjectMetadata`

Current project metadata and publishing settings snapshot.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `blog_languages` | `string[]` | Yes | Languages configured for the blog. |
| `categories` | `string[]` | Yes | Assigned category names. |
| `default_author` | `string \| nil` | No | Default post author name. |
| `description` | `string \| nil` | No | Human-readable description. |
| `main_language` | `string \| nil` | No | BCP 47 language code. |
| `name` | `string` | Yes | Human-readable name. |
| `public_url` | `string \| nil` | No | Published site base URL. |
| `publishing_preferences` | `table` | Yes | Project publishing configuration. |

## `PostData`

Post record with link graph data added for scripting.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `backlinks` | `table[]` | Yes | Links from other posts to this post. |
| `categories` | `string[]` | Yes | Assigned category names. |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `id` | `string` | Yes | Stable record identifier. |
| `language` | `string \| nil` | No | BCP 47 language code. |
| `links_to` | `table[]` | Yes | Links from this post to other posts. |
| `project_id` | `string` | Yes | Identifier of the owning project. |
| `slug` | `string` | Yes | URL-safe record identifier. |
| `status` | `string` | Yes | Current lifecycle state. |
| `tags` | `string[]` | Yes | Assigned tag names. |
| `title` | `string` | Yes | Human-readable title. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `MediaData`

Media record stored for a project.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `alt` | `string \| nil` | No | Alternative text for the media. |
| `caption` | `string \| nil` | No | Media caption. |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `file_path` | `string` | Yes | Stored media file path. |
| `id` | `string` | Yes | Stable record identifier. |
| `mime_type` | `string` | Yes | Media MIME type. |
| `original_name` | `string` | Yes | Original imported filename. |
| `project_id` | `string` | Yes | Identifier of the owning project. |
| `tags` | `string[]` | Yes | Assigned tag names. |
| `title` | `string \| nil` | No | Human-readable title. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `ScriptData`

Lua script record.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `enabled` | `boolean` | Yes | Whether the record is enabled. |
| `entrypoint` | `string` | Yes | Lua function invoked by the runtime. |
| `id` | `string` | Yes | Stable record identifier. |
| `kind` | `string` | Yes | Public enum value. |
| `project_id` | `string` | Yes | Identifier of the owning project. |
| `slug` | `string` | Yes | URL-safe record identifier. |
| `status` | `string` | Yes | Current lifecycle state. |
| `title` | `string` | Yes | Human-readable title. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `TemplateData`

Template record for site rendering.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `enabled` | `boolean` | Yes | Whether the record is enabled. |
| `id` | `string` | Yes | Stable record identifier. |
| `kind` | `string` | Yes | Public enum value. |
| `project_id` | `string` | Yes | Identifier of the owning project. |
| `slug` | `string` | Yes | URL-safe record identifier. |
| `status` | `string` | Yes | Current lifecycle state. |
| `title` | `string` | Yes | Human-readable title. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `TagData`

Tag record stored for a project.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `color` | `string \| nil` | No | Optional display color. |
| `created_at` | `ISO-8601 string` | Yes | Creation timestamp. |
| `id` | `string` | Yes | Stable record identifier. |
| `name` | `string` | Yes | Human-readable name. |
| `post_template_slug` | `string \| nil` | No | Template selected for tagged posts. |
| `project_id` | `string` | Yes | Identifier of the owning project. |
| `updated_at` | `ISO-8601 string` | Yes | Last-update timestamp. |

## `TaskData`

Public task snapshot returned by the task manager.

**Lua shape**

```lua
{
  id = "example-id",
  message = "Working",
  name = "Example",
  progress = 0.5,
  status = "draft",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `id` | `string` | Yes | Stable record identifier. |
| `message` | `string \| nil` | No | Latest user-facing task message. |
| `name` | `string` | Yes | Human-readable name. |
| `progress` | `number \| nil` | No | Completion value reported by the task. |
| `status` | `string` | Yes | Current lifecycle state. |

## `TaskStatus`

Aggregate task status snapshot.

**Lua shape**

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

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `active_count` | `integer` | Yes | Number of active tasks. |
| `pending_count` | `integer` | Yes | Number of queued tasks. |
| `running_count` | `integer` | Yes | Number of currently running tasks. |
| `tasks` | `TaskData[]` | Yes | Tasks included in this status snapshot. |

## `GitProviderData`

Git hosting provider classification used by bDS2 repository state.

**Lua shape**

```lua
{
  kind = "utility",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `kind` | `string` | Yes | Public enum value. |

## `GitRepositoryState`

Repository state returned by bds.sync.get_repo_state and get_remote_state.

**Lua shape**

```lua
{
  current_branch = "example",
  has_lfs = true,
  is_initialized = true,
  provider = {
  kind = "utility",
},
  remote_url = "example",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `current_branch` | `string \| nil` | No | Checked-out branch name. |
| `has_lfs` | `boolean` | Yes | Whether Git LFS tracking is configured. |
| `is_initialized` | `boolean` | Yes | Whether the project folder is a Git repository. |
| `provider` | `GitProviderData \| nil` | No | Hosting provider inferred from the remote URL. |
| `remote_url` | `string \| nil` | No | Configured origin URL. |

## `GitFileStatus`

One changed path in the active repository.

**Lua shape**

```lua
{
  old_path = "example",
  path = "example",
  status = "draft",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `old_path` | `string \| nil` | No | Previous repository-relative path for a rename. |
| `path` | `string` | Yes | Current repository-relative path. |
| `status` | `string` | Yes | Current lifecycle state. |

## `GitStatusResult`

Repository working-tree status returned by bds.sync.get_status.

**Lua shape**

```lua
{
  files = {
  {
    old_path = "example",
    path = "example",
    status = "draft",
  }
},
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `files` | `GitFileStatus[]` | Yes | Changed repository paths. |

## `GitSyncStatus`

Whether a commit exists locally, remotely, or in both histories.

**Lua shape**

```lua
{
  kind = "utility",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `kind` | `string` | Yes | Public enum value. |

## `GitCommitData`

Commit entry returned in bds.sync history.

**Lua shape**

```lua
{
  author = "example",
  date = "2026-07-19T08:00:00Z",
  hash = "example",
  subject = "example",
  sync_status = {
  kind = "utility",
},
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `author` | `string \| nil` | No | Commit author. |
| `date` | `ISO-8601 string \| nil` | No | ISO-8601 commit date. |
| `hash` | `string` | Yes | Git object identifier. |
| `subject` | `string \| nil` | No | Commit subject. |
| `sync_status` | `GitSyncStatus` | Yes | Local/remote presence classification. |

## `GitHistoryResult`

Commit history returned by bds.sync.get_history.

**Lua shape**

```lua
{
  commits = {
  {
    author = "example",
    date = "2026-07-19T08:00:00Z",
    hash = "example",
    subject = "example",
    sync_status = {
    kind = "utility",
  },
  }
},
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `commits` | `GitCommitData[]` | Yes | Commit history entries. |

## `GitNetworkResult`

Result of a successful bds.sync network operation.

**Lua shape**

```lua
{
  output = "example",
  updated = true,
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `output` | `string` | Yes | Git command output. |
| `updated` | `boolean` | Yes | Whether the Git network command completed successfully. |

## `GitCommitResult`

Result of successfully committing all pending repository changes.

**Lua shape**

```lua
{
  message = "Working",
  output = "example",
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `message` | `string` | Yes | Latest user-facing task message. |
| `output` | `string` | Yes | Git command output. |

## `ValidationResult`

Template validation result.

**Lua shape**

```lua
{
  errors = { "example" },
  valid = true,
}
```

| Field | Type | Required | Meaning |
| --- | --- | --- | --- |
| `errors` | `string[]` | Yes | Validation error messages. |
| `valid` | `boolean` | Yes | Whether validation succeeded. |
