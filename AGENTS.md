# Agents Instructions for Blogging Desktop Server (bDS)

This is the Rust rewrite of an existing project bDS written in Typescript and living in ../bDS - if
in doubt about behaviour, look at the original code to verify.

This project has an allium spec in the folder spec/ - use it to verify behaviour against expected behaviour. It is based on the typescript implementation.

Invariants and behaviours in the allium spec should be covered by unit tests of the application code, to make sure the spec is followed.

## Plan Mode

- Make the plan extremely concise. Sacrifice grammar for the sake of concision.
- At the end of each plan, give me a list of unresolved questions to answer, if any.

## Commits

- our default branch is origin/master
- commit messages are short - one sentence. do not write long articles.
- pull requests are more verbose and especially give reasoning for changes

## Important facts

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

## important behaviour

- HEREDOCs don't work most of the time. Don't use them. Use editor tools to create proper scripts
- use red/green TDD for new implementations
- there are no "pre-existing" problems - you own every problem, you fix every problem
- don't leave unused code in the codebase, remove it instead
- after implementing / changing things, run the build and run tests to verify all works
- do not reference external JavaScript or CSS on CDNs, always bring it into the project
- do not embedd CSS/JavaScript into HTML, always reference .css and .js files in the project assets
- always make sure you follow proper i18n best practices. no untranslated string constants.

