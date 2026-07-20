# Agents Instructions for Blogging Desktop Server (bDS)

This is the Rust rewrite of an existing project bDS. The baseline implementation is bDS2, written in Elixir and living in ../bDS2 - if
in doubt about behaviour, look at the baseline code to verify.

This project has an allium spec in the folder specs/ - use it to verify behaviour against expected behaviour. It is synced from the bDS2 specs (../bDS2/specs). The command line utility is installed.

Invariants and behaviours in the allium spec should be covered by unit tests of the application code, to make sure the spec is followed.

## UI styling

- Before implementing or changing UI, read and follow `docs/UI_STYLE_GUIDE.md`.
- Reuse the shared styling primitives described there so new sidebars and editor areas remain consistent with the post editor.

## Plan Mode

- Make the plan extremely concise. Sacrifice grammar for the sake of concision.
- At the end of each plan, give me a list of unresolved questions to answer, if any.

## Commits

- our default branch is origin/master
- commit messages are short - one sentence. do not write long articles.
- pull requests are more verbose and especially give reasoning for changes

## Important facts

- update `README.md` whenever a user-visible feature is added, removed, or materially changed; keep it a compact overview with relevant pointers
- work is tracked as Gitea issues (`tea` CLI, repo `hugo/RuDS`); there are no plan documents — file or update issues when scope changes
- published posts don't have body in the database, the body content is only in the file
- functionality you implement have to be tied to UI
- UI you implement has to be tied to functionality
- you must use proper tools to generate migrations and snapshots, don't hack SQL
- we use an sqlite database. use sqlite semantics in snapshots and other artifacts
- on MacOS we use native menus and you have to hook them into the intercept for new menu items
- there are two areas of localization, you sometimes need both (menus for example)
- all automatic AI activities must be gated by airplane (offline) mode of the app and either use the local model or inform the user via toast
- metadata needs to be flushed to the filesystem and needs to be included in metadata diff tool and in rebuild from filesystem. All three aspects have to be in sync with each other.
- if you add new metadata, add them to publishing, metadata-diff and rebuild-from-database
- Rust and its ecosystem are new and moving fast. you're builtin knowledge is probably outdated. use the web to make sure you know what you are doing.

## important behaviour

- HEREDOCs don't work most of the time. Don't use them. Use editor tools to create proper scripts
- use red/green TDD for new implementations
- there are no "pre-existing" problems - you own every problem, you fix every problem
- don't leave unused code in the codebase, remove it instead
- after implementing / changing things, run the build and run tests to verify all works
- run `cargo test --workspace` with permission for its loopback test servers on the first attempt; AI mock-server and preview-server tests bind localhost and otherwise fail under sandboxing, causing a pointless rerun
- do not reference external JavaScript or CSS on CDNs, always bring it into the project
- do not embedd CSS/JavaScript into HTML, always reference .css and .js files in the project assets
- always make sure you follow proper i18n best practices. no untranslated string constants.
- when creating rust source code, always follow what the allium spec is saying for that part
- when tending the allium spec, make sure you validate the spec with the installed command line utility
- don't be lazy. don't defer or skip implementations just because you have to write code for that, that is ridiculous. if the spec says something has to be there, it has to be there.
