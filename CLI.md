# RuDS Command Line, Headless Server, and Terminal UI

## In this article

- [What the CLI is](#what-the-cli-is)
- [Installing the CLI](#installing-the-cli)
- [Global flags and output](#global-flags-and-output)
- [Command reference](#command-reference)
- [Creating content from the command line](#creating-content-from-the-command-line)
- [Running the headless server](#running-the-headless-server)
- [Using the terminal UI](#using-the-terminal-ui)
- [How the CLI and the desktop app stay in sync](#how-the-cli-and-the-desktop-app-stay-in-sync)

## What the CLI is

`bds-cli` is the automation surface of RuDS. It runs the same engines against the same application database and the same projects as the desktop app — there is no separate state, no export/import step, and no second configuration. Anything the CLI changes shows up in an open desktop window automatically.

Typical uses: scripted rebuilds and site generation, publishing from CI or cron, creating posts from other tools, importing images in bulk, running utility Lua scripts, and starting the headless server or the terminal UI.

## Installing the CLI

Install the launcher from the desktop app under **Settings → Data**, or run `bds-cli install` from a packaged binary. Both place a small forwarding launcher in `~/.local/bin` that executes the packaged CLI next to the shared runtime. Make sure `~/.local/bin` is on your `PATH`.

## Global flags and output

| Flag | Effect |
|---|---|
| `--json` | Print a stable JSON result envelope instead of human-readable text |
| `--airplane` | Block network commands and route automatic AI work to the local (airplane) endpoint |

The JSON envelope always has the shape `{"ok": bool, "command": string, "message": string, "data": object, "progress": [string], "notices": [string]}`, which makes the CLI safe to drive from scripts and LLM agents. Errors exit with code 1 and a formatted message on stderr; unknown commands and invalid options do the same.

With `--airplane`, `upload`, `push`, and `pull` are refused, and AI-assisted steps (language detection, translation, image enrichment) use the configured airplane endpoint. When no local endpoint is configured, the CLI prints a notice and falls back to offline heuristics where they exist — the command-line equivalent of the desktop's airplane-mode toast.

## Command reference

All commands operate on the active project from the shared project registry unless stated otherwise.

| Command | What it does |
|---|---|
| `rebuild` | Rebuild the cache database from the active project's files. `--incremental` applies only detected differences and imports orphan files |
| `repair <part>` | Run one derived-data repair: `post-links`, `media-links`, `thumbnails`, `embeddings`, or `search` |
| `render` | Render the generated site. `--incremental` validates output and applies only differences; `--force` re-renders everything, ignoring content hashes |
| `upload` | Upload generated HTML, thumbnails, and media using the project publishing settings, waiting for the publish job |
| `push` | Push the project repository to origin |
| `pull` | Fast-forward pull, then reconcile the cache database with the pulled files |
| `post` | Create a post from flags or JSON on stdin (see below) |
| `media <file>` | Import one image with best-effort AI enrichment and translations. `--language` overrides detection |
| `gallery <images…>` | Create a post and import/link all supplied images through the shared gallery pipeline |
| `config get\|set\|list` | Read or update global application settings; `list` shows effective values with secrets redacted |
| `project list\|add\|switch` | List, add (`--name` optional), or switch projects in the shared registry |
| `lua <script> [args…]` | Run an enabled utility Lua script from the active project in the sandboxed runtime |
| `server` | Start the authenticated headless SSH server (see below) |
| `tui` | Start the interactive terminal UI locally |
| `install` | Install the forwarding launcher in `~/.local/bin` |

## Creating content from the command line

`post` accepts either flags or a JSON object on stdin:

```
bds-cli post --title "Release notes" --content "…" --tags releases,news
echo '{"title":"Release notes","content":"…","tags":["releases"]}' | bds-cli post --stdin
```

Available flags: `--title`, `--content`, `--excerpt`, `--author`, `--language`, `--template`, `--tags`, `--categories`, `--no-translate`, `--stdin`. When `--language` is omitted the language is auto-detected (AI endpoint when available, offline heuristic otherwise, with a printed notice). After creation the same automatic translation cascade as the desktop app is scheduled and awaited.

`gallery` takes the same post flags plus image paths; every image is imported, linked to the new post, AI-enriched, and translated like the desktop gallery workflow. `media` runs the same pipeline for a single image without creating a post.

## Running the headless server

`bds-cli server` runs RuDS without a window — same binary family, same database, same engines. Launching the desktop binary with `BDS_MODE=server` is equivalent.

| Flag | Effect |
|---|---|
| `--bind <ip>` | SSH listen address. Defaults to loopback; external access must be explicit |
| `--port <port>` | SSH listen port (default 2222) |
| `--database <path>` | Application database path override |
| `--data-dir <path>` | Private application data directory containing the SSH key material |

Only the SSH port is ever exposed. On first start the server generates a restrictive RSA host key and an empty `authorized_keys` file in an `ssh/` folder inside the private application data directory. Add one public key per line — standard OpenSSH format. `authorized_keys` is re-read on every authentication, so key changes need no restart. There are no passwords.

Clients connect in two ways:

- **Terminal**: `ssh -p 2222 user@server` opens the terminal UI directly in the SSH session. Each client gets its own session; all sessions share the server's data and UI language.
- **Desktop app**: **File → Connect to Server…** with `user@host` or `user@host:port`. The desktop uses its own generated Ed25519 identity and trust-on-first-use `known_hosts`, negotiates the native protocol over the same SSH connection, and shows the remote workspace in the window. **File → Disconnect from Server** returns to the local workspace.

All connected clients stay synchronized through ordered domain events: an edit in one session refreshes lists and closes deleted-entity tabs everywhere.

## Using the terminal UI

Start it locally with `bds-cli tui`, or get it automatically over `ssh` to a headless server. The status line at the bottom always shows the keys available in the current context.

### Views and navigation

| Key | View |
|---|---|
| `1` | Posts |
| `2` | Media |
| `3` | Templates |
| `4` | Scripts |
| `5` | Tags |
| `6` | Settings |
| `7` | Git |

`↑`/`↓` or `j`/`k` move the sidebar selection, `Enter` opens the selected entry, `Esc` goes back, `q` quits.

### Working with content

- Posts open in a syntax-highlighted, soft-wrapped editor. `n` creates a new post. `Ctrl+S` saves the draft, `Ctrl+P` saves and publishes, `Ctrl+E` toggles the rendered Markdown preview, `Ctrl+G` runs the one-shot AI post analysis (airplane-gated).
- Templates and scripts open with HTML+Liquid and Lua highlighting.
- Images open in the terminal image preview.
- `/` opens a live filter for the current view: plain words search text, `tag:x` and `category:x` filter like the desktop chips, and an ISO date (`yyyy-mm-dd`) narrows to that day. Tokens combine with AND; `Enter` keeps the filter, `Esc` clears it.
- `:` opens the command prompt with the parameterless Blog-menu commands (metadata diff, validate site, force render, rebuilds, reindex, translations, duplicates, upload, preview URL). `:?` shows the list as help. Completed metadata-diff and site-validation tasks open a report panel; `Enter` applies the whole report, `Esc` closes it.
- `p` opens the project switcher; `o` switches to a folder prompt with bash-style tab completion to open a folder as a new project (a full rebuild is queued automatically).
- In Tags: `n` create, `Enter` rename, `c` cycle color, `t` cycle template, `d` then `y` delete, `s` sync from posts; in Merge, `Space` marks and `m` merges.
- In Git: `c` commit (stage all), `u` pull (fast-forward only), `s` push, `Enter` jumps the diff to a file.
- Settings sections mirror the desktop editor; `Enter` edits a field in the status line, `Ctrl+S` saves the section.

## How the CLI and the desktop app stay in sync

Every CLI mutation writes a notification row into the shared database. A running desktop app watches for these, invalidates its caches, and refreshes the affected views within moments — no manual reload. Processed notifications are pruned automatically.

Because CLI, TUI, server, and desktop all use the same engines, invariants hold everywhere: published bodies live in files, metadata flushes to the filesystem, and rebuilds reconcile external changes.
