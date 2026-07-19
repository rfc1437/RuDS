# RuDS User Guide

## In this article

- [Who this guide is for](#who-this-guide-is-for)
- [How RuDS works](#how-ruds-works)
- [Getting started](#getting-started)
- [Understanding the interface](#understanding-the-interface)
- [Working with posts](#working-with-posts)
- [Working with pages](#working-with-pages)
- [Working with media](#working-with-media)
- [Working with translations](#working-with-translations)
- [Using macros](#using-macros)
- [Using scripting](#using-scripting)
- [Using the AI assistant](#using-the-ai-assistant)
- [Organizing with tags](#organizing-with-tags)
- [Using blogmarks](#using-blogmarks)
- [Importing from WordPress (WXR)](#importing-from-wordpress-wxr)
- [Using Git (Source Control)](#using-git-source-control)
- [Configuring settings](#configuring-settings)
- [Checking and repairing metadata](#checking-and-repairing-metadata)
- [Managing templates](#managing-templates)
- [Generating and publishing](#generating-and-publishing)
- [Typical editorial workflows](#typical-editorial-workflows)
- [Working fully offline](#working-fully-offline)
- [Running a headless server and the terminal UI](#running-a-headless-server-and-the-terminal-ui)
- [Troubleshooting and recovery](#troubleshooting-and-recovery)
- [Team conventions](#team-conventions)

## Who this guide is for

This guide is for people who use RuDS day to day to create, edit, organize, translate, generate, and publish blog content. It is written for editors, content managers, and project owners who need reliable guidance on what each part of the application does and how to use it safely.

If you need implementation notes, project architecture, or development setup, use the repository README. This guide stays focused on end-user operation and editorial decisions.

### Key takeaways

- RuDS documentation should help with real editorial work, not only isolated clicks.
- Each chapter explains purpose first, then usage.
- Safe content handling and recoverability matter throughout the application.

[↑ Back to In this article](#in-this-article)

---

## How RuDS works

RuDS is a local-first writing and publishing workspace. You can draft, revise, structure, preview, and publish content on your machine without depending on constant internet access. Optional remote Git synchronization and AI-assisted workflows extend that model, but they do not replace it.

Three states matter in day-to-day work. A draft is your in-progress state. Publishing marks a local content state as published inside your project. A Git commit creates a recoverable snapshot that can be reviewed, synchronized, and restored. These actions are related, but they are not the same operation.

The recommended sequence remains simple: edit in draft, publish when the content is ready, then commit immediately. That is the safest pattern for protecting work and keeping project history understandable.

### Key takeaways

- RuDS is designed for local reliability first.
- Publish and commit are different actions and both matter.
- The safe default lifecycle is: Draft -> Publish -> Commit.

[↑ Back to In this article](#in-this-article)

---

## Getting started

Before you begin editorial work, confirm that the project context is correct. Open RuDS and select the right project. If this is a new project, create it and define its identity early, including project name and description.

Next, open Settings and verify the project data path and Public Base URL. The data path should match your backup strategy. The Public Base URL should be set early because sitemap and feed generation depend on it.

Finally, define language and author defaults. These defaults reduce repetitive edits and keep output consistent when multiple contributors work in the same project.

### Key takeaways

- Set project identity, data location, and Public Base URL at the beginning.
- Configure language and author defaults before regular editing starts.
- Early setup decisions reduce later cleanup.

[↑ Back to In this article](#in-this-article)

---

## Understanding the interface

The RuDS interface is organized around workflows rather than isolated forms. The Activity Bar on the left moves between major areas such as Posts, Pages, Media, Tags, Import, Source Control, and Settings. The Sidebar changes with the active area and helps with filtering, selection, and navigation. The Editor area is where most work happens and supports tabbed editing for content, configuration, and analysis views.

The bottom panel and status area matter during longer operations such as imports, rebuild actions, metadata scans, and media work. Toasts provide quick feedback. The Output panel provides deeper detail when something needs attention.

Tab behavior is optimized for quick scanning and focused editing. Single click often opens a transient tab. Double click or explicit actions pin a tab for longer work.

### Key takeaways

- Use the Activity Bar for section-level context switching.
- Use the Sidebar for finding and narrowing content.
- Pin tabs when you move from inspection to editing.

[↑ Back to In this article](#in-this-article)

---

## Working with posts

The Posts section is for chronological content such as articles, notes, and recurring updates. In most editorial teams, Posts are the primary outward-facing stream.

A post combines title, body content, category, tags, excerpt, and status. Titles establish topic. Body content carries the narrative. Categories provide broad structure. Tags support finer discovery. Status should be used intentionally so collaborative workflows stay clear.

A reliable post workflow is: draft to completion, review structure and metadata, preview the result, publish when editorially ready, then commit immediately.

When you want help refining post metadata, use Quick Actions in the post editor and review AI suggestions for title, summary, and slug. Treat this as editorial assistance, not an automatic rewrite.

### Key takeaways

- Use Posts for date-oriented and regularly updated content.
- Categories and tags serve different purposes: broad grouping versus precise discovery.
- Publish only when editorially ready, then commit right away.

[↑ Back to In this article](#in-this-article)

---

## Working with pages

Pages are for durable, non-chronological content such as About, Contact, legal notices, and other structural information. Use Pages when content should stay stable in navigation and should not be interpreted as part of a time-based feed.

Because pages are revisited over longer periods, naming consistency matters. Keep titles and slugs predictable, avoid unnecessary structural churn, and follow your project navigation conventions.

The working pattern is similar to posts: draft, review, preview, publish, commit. The difference is editorial intent: pages prioritize clarity and long-term maintainability over release cadence.

### Key takeaways

- Use Pages for stable structural content.
- Keep titles and slugs consistent for maintainability.
- Apply the same safe lifecycle: Draft -> Publish -> Commit.

[↑ Back to In this article](#in-this-article)

---

## Working with media

The Media section is where you import, describe, and maintain assets used by posts and pages. It is not only a file list; it is also where accessibility and descriptive quality are enforced through metadata.

When importing media, add metadata while context is still fresh. Alt text should describe meaning for accessibility. Captions should support reader understanding. Media tags should help later retrieval and reuse.

You can also drag image files into the post editor or paste screenshots from the clipboard. RuDS imports the image into the media library, links it to the current post, and inserts the Markdown image at the cursor position.

### Key takeaways

- Media management includes metadata quality, not only file import.
- Add alt text and captions during import, not as a postponed task.
- Commit content and related media in the same change when possible.

[↑ Back to In this article](#in-this-article)

---

## Working with translations

RuDS supports translating both posts and media metadata into multiple languages. Translations are stored separately from canonical content so localized variants do not drift into unrelated records.

### Post translations

Each post has a canonical language and can have translations for additional languages. Translations keep their own title, excerpt, and content, while canonical metadata such as category, tags, slug, and publish state stays centralized.

The post editor shows the current language, existing translations, and missing languages. Posts marked Do Not Translate are excluded from automatic translation and from alternate language trees during site generation.

Published translation body content follows the same filesystem rule as published posts: the body lives in the file, not in the database.

### Media translations

Media items can have translated title, alt text, and caption values per language. The binary asset stays shared; only descriptive text varies by language.

### Automatic translation cascade

When blog languages are configured, RuDS can fill missing translations for posts and linked media. Automatic translation respects airplane mode and the configured AI runtime. If an automatic action cannot run in the current AI mode, RuDS reports that through the UI instead of silently inventing a result.

### Key takeaways

- Post translations store title, excerpt, and content separately from the canonical post.
- Media translations store translated descriptive text while the asset stays shared.
- Automatic translation keeps posts and linked media aligned across configured languages.
- Do Not Translate excludes content from multi-language workflows.

[↑ Back to In this article](#in-this-article)

---

## Using macros

Macros let you insert dynamic content blocks directly inside Markdown by using `[[macro_name ...]]` syntax. RuDS expands these macros during preview and generated output using local assets only.

Built-in macros include YouTube, Vimeo, gallery, photo archive, and tag cloud helpers. Use them when you want reusable rich blocks without dropping into raw HTML.

### Key takeaways

- Macros are inserted directly in Markdown and expanded during preview and publishing.
- Use macro parameters to control behavior without leaving the editor.
- Built-in macros remain the first choice for common embedded content blocks.

[↑ Back to In this article](#in-this-article)

---

## Using scripting

Scripts in RuDS are Lua files stored in your project's `scripts/` directory. Published scripts are written as `.lua` files with frontmatter metadata, so they stay portable and Git-reviewable.

Each script has a Kind (`macro`, `transform`, or `utility`) and an Entrypoint. Utility and transform scripts typically default to `main`. Macro scripts default to `render`.

### Transform scripts

Transform scripts run during blogmark import to normalize or enrich incoming post data before the post is created. The entrypoint receives a post table and can optionally receive a context table.

```lua
function main(post, context)
  local title = (post.title or ""):gsub("^%s+", ""):gsub("%s+$", "")

  if title ~= "" and not title:match("^%[Clipped%]") then
    post.title = "[Clipped] " .. title
  end

  post.categories = { "Inbox", "Research" }
  return post
end
```

`context.source` identifies the import source. `context.url` contains the original bookmarked URL when that information exists.

### Macro scripts

Macro scripts let you create custom `[[macro_name ...]]` blocks that expand during preview and generation. The entrypoint receives a context table and the current post table.

```lua
function render(context, post)
  local params = context.params or {}
  local title = (post and post.title) or "Unknown"
  local label = params.label or ""

  return {
    html = "<p>" .. title .. ": " .. label .. "</p>"
  }
end
```

Built-in macros take priority over custom Lua macros that reuse the same slug.

### API access

Lua scripts can call the application API through `bds`. The in-app API tab is rendered from the live Lua capability map, and [API.md](API.md) is generated from the same source.

```lua
local result = bds.posts.get("post-id")
```

### Key takeaways

- Scripts in RuDS are Lua files, not Python files.
- Published scripts are stored as `.lua` files with frontmatter metadata.
- `main` is the usual entrypoint for utility and transform scripts; `render` is the usual entrypoint for macros.
- The scripting API is documented with Lua examples and kept in sync with the live runtime.

[↑ Back to In this article](#in-this-article)

---

## Using the AI assistant

The AI assistant is integrated into RuDS to help with editorial tasks such as search, analysis, metadata suggestions, translation, and structured content inspection.

The assistant works on your project data. Depending on configuration, requests can run against the configured online endpoint or the airplane-mode endpoint. Automatic AI actions remain gated by airplane mode rules in the app, and RuDS surfaces status through toasts and the Output area instead of silently bypassing that policy.

The assistant can present results as text, tables, cards, charts, metrics, lists, forms, and tabbed views. Ask plainly for the result you need, or request a specific presentation when that helps your workflow.

### Key takeaways

- The assistant works with your project content and metadata.
- AI configuration can be online or airplane-mode based, depending on your setup.
- Automatic AI actions respect airplane mode and report availability through the UI.
- Ask for a table, chart, list, or form when a specific shape is useful.

[↑ Back to In this article](#in-this-article)

---

## Organizing with tags

Tags are your precision taxonomy tool. Over time, even well-managed projects accumulate near-duplicate tags, naming inconsistencies, and labels that no longer help readers or editors. Use the Tags area to keep taxonomy useful.

After significant taxonomy cleanup, create a focused commit that captures the change clearly.

### Key takeaways

- Tags improve discovery only if naming stays consistent.
- Merge and rename operations should be deliberate and reviewed.
- Commit taxonomy changes in focused snapshots.

[↑ Back to In this article](#in-this-article)

---

## Using blogmarks

Blogmarks provide a quick way to save links from the browser directly into RuDS as new posts. Generate the bookmarklet from Settings, add it to your browser bar, and click it when you want to capture a page into the current project.

Transform scripts can normalize incoming blogmark posts before creation. Use them for title cleanup, default tags, or source-specific formatting.

### Key takeaways

- Blogmarks turn the browser into a one-click content capture tool.
- Generate the bookmarklet from Settings and add it to your browser bar.
- Use transform scripts to enrich incoming posts automatically.

[↑ Back to In this article](#in-this-article)

---

## Importing from WordPress (WXR)

The Import section supports structured migration from WordPress exports. Treat import as a staged workflow: analyze first, adjust mappings, then execute. For larger sites, iterative passes are usually safer than a single rigid import.

### Key takeaways

- Treat WXR import as analyze, adjust, execute.
- Iterative passes are safer than one large import.
- Validate representative output before committing migrated content.

[↑ Back to In this article](#in-this-article)

---

## Using Git (Source Control)

Source Control in RuDS is the foundation for reliable recovery and collaboration. Publishing marks local editorial state, but Git commits provide durable history.

In a normal cycle, synchronize first, complete editorial changes, publish when ready, commit with a specific message, then push when you want to share the result.

### Key takeaways

- Git provides recoverable history; publishing alone does not.
- A stable rhythm is: sync, edit, publish, commit, push.
- Specific commit messages improve teamwork and recovery.

[↑ Back to In this article](#in-this-article)

---

## Configuring settings

Settings define how the project behaves. Project settings control identity, paths, public URL context, and render languages. Editor settings shape day-to-day working defaults. AI settings are optional and should enhance, not define, your editorial workflow.

Maintenance actions such as rebuilds and diff scans are repair tools for specific situations, not part of routine editing.

### Key takeaways

- Settings affect long-term consistency across the project.
- Optional integrations should not replace the core workflow.
- Rebuild actions are corrective tools, not daily habits.

[↑ Back to In this article](#in-this-article)

---

## Checking and repairing metadata

Over time, metadata stored in the database and metadata stored on disk can drift apart, especially after external edits, merges, or file operations. The Metadata Diff tool detects these inconsistencies and lets you repair them without rebuilding everything.

The scan covers posts, media, scripts, and templates. Results are grouped by entity type, and field pills let you focus on one kind of difference at a time.

Use DB to File when the database is correct. Use File to DB when the filesystem is correct.

### Key takeaways

- Metadata Diff compares database records against files on disk.
- Field pills help you bulk-repair one difference type at a time.
- Use it after external changes, not as part of routine editing.

[↑ Back to In this article](#in-this-article)

---

## Managing templates

Templates control the Liquid layout used when RuDS generates HTML pages. Template kinds determine where they are used: `post`, `list`, `not-found`, and `partial`.

Templates are stored as files with frontmatter metadata in the project data directory, so they are portable and Git-reviewable.

### Key takeaways

- Templates define the generated HTML layout.
- Four template kinds cover page, list, not-found, and reusable partial rendering.
- Templates are filesystem-backed and Git-friendly.

[↑ Back to In this article](#in-this-article)

---

## Generating and publishing

Publishing in RuDS is a staged process: publish content locally, generate or validate-and-apply site changes, commit the result, then deploy when ready.

Full generation builds the entire static site. Site validation detects missing, extra, and updated routes so RuDS can re-render only what changed. This is the practical incremental workflow for most daily editorial changes.

When blog languages are configured, generation produces language-aware route trees, per-language feeds, and alternate language metadata.

### Key takeaways

- Full generation produces the complete site.
- Validate and Apply is the efficient daily workflow for incremental publishing.
- Public Base URL must be set before generation.
- Commit generated output before deploying for recoverability.

[↑ Back to In this article](#in-this-article)

---

## Typical editorial workflows

Short link posts benefit from a lightweight workflow: create, add concise context, classify, preview once, publish, commit. Long-form articles benefit from a fuller cycle: draft thoroughly, add media, review metadata, preview carefully, publish, commit content and media together.

Across both patterns, the safety baseline stays the same: Draft -> Publish -> Commit.

### Key takeaways

- Use a lightweight workflow for short notes and links.
- Use a fuller workflow for long-form content with media.
- Keep the same safety baseline in both cases.

[↑ Back to In this article](#in-this-article)

---

## Working fully offline

RuDS is designed so core editorial work can continue without network access. You can create and revise content, manage metadata, preview locally, and publish within local project state while offline.

When AI is involved, airplane mode determines which automatic actions are allowed and which endpoint class is used. Keep local commits frequent even when you are not pushing to a remote.

### Key takeaways

- Core editing and publishing workflows work offline.
- Local commits still matter when no remote is available.
- Reconnect and synchronize in a controlled order.

[↑ Back to In this article](#in-this-article)

---

## Running a headless server and the terminal UI

RuDS can run without a window on a server (for example a Linux VPS) and be used from several places at once. Start it with `BDS_MODE=server`; the same release binary that runs the desktop app then runs headless. All clients connect through one SSH port (default 2222) — the web endpoint itself stays private on the server.

Access is controlled by SSH keys, not passwords. On first start the server creates an `ssh/` folder next to its database (for example `~/Library/Application Support/bds/ssh/` on macOS) containing the host key and an empty `authorized_keys` file. Paste the public keys of every machine that may connect into `authorized_keys`, one per line — the same file format OpenSSH uses.

To work in a terminal, connect with plain `ssh -p 2222 user@server`: the terminal UI opens directly. You can browse posts, media, templates, scripts, and tags, create and edit posts, publish, preview images, and run the one-shot AI actions. The status line at the bottom always shows the available keys.

To work in the desktop app against a remote server, use **File → Connect to Server…** and enter `user@host` (or `user@host:port`). The app connects with the SSH key from its own local `ssh/` folder and shows the remote workspace in the window; **File → Disconnect from Server** returns to the local workspace. A plain browser works too, through a manual SSH tunnel (`ssh -p 2222 -L 4010:127.0.0.1:4010 user@server`, then open `http://127.0.0.1:4010`).

Everything you see stays synchronized: when a script, another client, or a pipeline changes content on the server, every connected terminal and window updates. The interface language is a server-side setting — changing it in any client changes it for all of them.

### Key takeaways

- `BDS_MODE=server` runs the same app headless; only the SSH port is exposed.
- Access is public-key only: manage `authorized_keys` in the server's `ssh/` folder.
- `ssh` gives the terminal UI; **File → Connect to Server…** gives the desktop app; both use the same keys.
- All connected clients stay synchronized and share one interface language.

[↑ Back to In this article](#in-this-article)

---

## Troubleshooting and recovery

If content looks correct locally but is missing for collaborators, the usual cause is that changes were published but not committed and pushed. Check repository status, create a commit, then push to the expected remote.

If content lists or references become inconsistent after manual file changes, start with Metadata Diff. If broader inconsistency remains, use rebuild tools to realign database and filesystem state.

### Key takeaways

- Most missing remote content issues are commit or push gaps.
- Metadata Diff is the first repair tool after external file changes.
- Frequent meaningful commits are the strongest safety net.

[↑ Back to In this article](#in-this-article)

---

## Team conventions

Shared conventions reduce ambiguity and merge friction. Teams should agree on category definitions, tag naming rules, publish-readiness criteria, and commit message patterns.

A practical minimum rule is simple: any content considered published should be committed promptly.

### Key takeaways

- Explicit conventions improve speed and reduce avoidable conflict.
- Start with a small rule set and enforce it consistently.
- Minimum standard: published content should be committed promptly.

[↑ Back to In this article](#in-this-article)
