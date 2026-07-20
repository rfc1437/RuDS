# RuDS MCP Server

## In this article

- [What the MCP server is](#what-the-mcp-server-is)
- [Enabling and connecting](#enabling-and-connecting)
- [Security model](#security-model)
- [Resources](#resources)
- [Tools](#tools)
- [The proposal workflow](#the-proposal-workflow)
- [Configuring coding agents](#configuring-coding-agents)

## What the MCP server is

RuDS exposes your active project to external AI tools through the Model Context Protocol (MCP). An agent such as Claude Code or GitHub Copilot can browse posts, media, tags, categories, and statistics, run searches, and draft changes — while every write stays under your explicit control in the desktop app.

Two transports serve the same surface:

- **stdio**: the packaged `bds-mcp` executable speaks MCP over stdin/stdout. This is what coding agents launch directly.
- **HTTP**: a stateless localhost-only endpoint at `http://127.0.0.1:4124/mcp` (POST), started by the desktop app when enabled in **Settings → MCP**, and by the headless server for its loopback clients.

Both operate on the shared application database and the active project.

## Enabling and connecting

Open **Settings → MCP** in the desktop app to:

- enable or disable the HTTP endpoint and see its status and address,
- review pending write proposals (see below),
- install or remove ready-made agent configuration for supported tools.

The stdio binary needs no enablement; running it is the connection.

## Security model

- The HTTP endpoint binds to `127.0.0.1` only and validates the `Host` and `Origin` headers against localhost, so browsers cannot be tricked into calling it cross-origin. Only `POST` and `OPTIONS` are accepted.
- Reads are direct and read-only. Writes never take effect on their own: every write tool creates an inert, persisted proposal that a human must approve in the desktop app.
- Agent configuration written by RuDS never contains secrets.

## Resources

Resources use the `bds://` scheme and are read-only.

| Resource | Content |
|---|---|
| `bds://project` | Active project identity and metadata |
| `bds://posts` | Blog posts (paginated via `bds://posts{?cursor}`) |
| `bds://media` | Media items (paginated via `bds://media{?cursor}`) |
| `bds://tags` | All tags |
| `bds://categories` | All categories |
| `bds://stats` | Blog statistics |
| `bds://posts/{id}/media` | Media linked to one post |
| `bds://media/{id}/image` | The media image itself |

Post detail includes full content and metadata plus backlinks and outgoing links; published bodies are read from the canonical files.

## Tools

Read and search tools return results directly:

| Tool | Purpose |
|---|---|
| `check_term` | Check whether a term is a category, a tag, or both, with post counts |
| `search_posts` | Full-text and filtered post search with pagination, backlinks, and outgoing links |
| `count_posts` | Count filtered posts grouped by year, month, tag, category, or status |
| `read_post_by_slug` | Read full post content and metadata, optionally in a translated language |
| `get_post_translations` | List every translation for a post |
| `get_media_translations` | List every translated metadata record for a media item |

Write tools create proposals instead of writing:

| Tool | Proposes |
|---|---|
| `draft_post` | A new post (title, content, excerpt, tags, categories, author, language) |
| `propose_post_metadata` | Metadata changes to an existing post |
| `propose_media_metadata` | Title, alt text, caption, or tag changes for a media item |
| `upsert_media_translation` | Translated media metadata for one language |
| `propose_script` | A Lua script (validated before proposing) |
| `propose_template` | A Liquid template (validated before proposing) |

## The proposal workflow

1. An agent calls a write tool. RuDS validates the payload and stores it as a pending proposal; nothing else happens.
2. The proposal appears under **Settings → MCP** in the desktop app with its full payload.
3. You accept or reject it. Acceptance applies the change exactly once through the same engines the editors use — files, search indexes, embeddings, and domain events all behave like a normal edit. Rejection discards it.
4. Pending proposals expire automatically after 30 minutes. Concurrent resolution is safe: a proposal can only be applied once, and the agent can query the outcome.

This keeps agents useful for drafting and bulk suggestions while a human stays the only path to persistent change.

## Configuring coding agents

**Settings → MCP** can install a guarded configuration for:

| Agent | What is written |
|---|---|
| Claude Code | An entry in `~/.claude.json` launching the packaged `bds-mcp` stdio binary |
| GitHub Copilot | The equivalent MCP server entry in Copilot's configuration file |

Installation is opt-in per agent, shows what will be written, never stores secrets, and can be removed the same way.
