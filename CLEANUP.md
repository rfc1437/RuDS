# Cleanup Backlog

Findings from a repo-wide audit (2026-07-18) for over-engineering, dead code, and
idiomatic-Rust issues beyond what clippy/rustfmt catch. Ordered by priority —
the first item is a real bug, the rest are cleanup. Each item is self-contained.

Ground rules (from AGENTS.md): red/green TDD, run build + tests after each item,
remove unused code instead of keeping it, verify behaviour against the allium
specs in `specs/` and the Elixir baseline in `../bDS2` when in doubt.

## 1. BUG: temp-file collision in atomic_write

`crates/bds-core/src/util/atomic_write.rs` uses `path.with_extension("tmp")`,
which *replaces* the last extension. Sibling files that differ only in their
final extension map to the same temp path:

- `esmeralda.en.md` → `esmeralda.en.tmp`
- `esmeralda.en.meta` → `esmeralda.en.tmp`

Concurrent writes (e.g. publishing writes post file + sidecars in parallel) can
interleave on the shared temp file and corrupt or cross-write content. It also
clobbers any real file named `*.tmp`.

**Fix:** append a unique suffix to the full filename instead of replacing the
extension, e.g. `format!("{}.{}.tmp", file_name, std::process::id())` or a
counter/uuid. Add a regression test writing `a.en.md` and `a.en.meta`
concurrently (or at least assert the two temp paths differ).

## 2. Parallel record-struct layer in the DB glue (~450 lines)

`crates/bds-core/src/db/from_row.rs` defines a `*Record` twin struct for all 12
tables plus hand-written `From`/`TryFrom` conversions in both directions. Most
fields are copied 1:1; only a handful need conversion (status/kind enums,
`i32` bools, JSON string-lists).

**Fix:** implement diesel `ToSql`/`FromSql` (or `serialize`/`deserialize`
wrappers) for `PostStatus`, `TemplateKind`, `TemplateStatus`, `ScriptKind`,
`ScriptStatus`, `NotificationEntity`, `NotificationAction`, plus a newtype for
JSON-serialized `Vec<String>` (tags/categories) and bool-as-i32, then derive
`Queryable`/`Selectable`/`Insertable`/`AsChangeset` on the domain models in
`crates/bds-core/src/model/` directly. Delete the record structs and the
`convert`/`convert_all` helpers. Keep behaviour identical: tags serialize as
JSON arrays, unknown enum strings must still error, `treat_none_as_null` must
be preserved.

## 3. Split the god file: bds-ui/src/app.rs (9,090 lines)

`update()` alone spans ~2,250 lines (line 982–3238). The fix pattern already
exists in the same file: `handle_tags_msg` and `handle_settings_msg` delegate a
message sub-enum to a dedicated handler.

**Fix:** apply the same pattern to the remaining domains — post editor,
media editor, template/script editors, preview/embedded webview, AI actions,
translation flows, sidebar/filtering. Move each handler group into its own
module (e.g. `crates/bds-ui/src/app/posts.rs`), keeping `Message` routing thin.
Pure refactor: no behaviour change, existing tests must stay green.

## 4. Dead task machinery in engine/task.rs (~130 lines)

Zero production call sites in the workspace for:

- `ProgressThrottle` (whole struct + its 2 tests) — its logic is already
  duplicated inside `TaskManager::report_progress`
- `TaskProgress` struct
- `try_start`
- `submit_grouped` and the `group_id`/`group_name` fields it fills (carried
  through `TaskEntry` and `TaskSnapshot`, never read by the UI)
- `label()`, `message()` accessors
- `next_queued` (test-only)

**Fix:** delete them and their tests.

**Related wiring gap (do NOT delete, investigate):** `evict_expired()` and
`is_cancelled()` are also never called from production code. That means
finished tasks are never evicted at runtime and cooperative cancellation never
actually interrupts running work. Either wire them up in `bds-ui/src/app.rs`
(evict on the task-snapshot refresh tick; check `is_cancelled` inside
long-running engine loops) or confirm against the allium spec what the intended
behaviour is.

## 5. Dead public functions (~250 lines)

Zero call sites anywhere in the workspace (verified by sweep):

- `crates/bds-core/src/engine/validate_translations.rs`:
  `validate_translations` — only `validate_translations_with_progress` is used
- `crates/bds-core/src/engine/meta.rs`: `update_blog_languages`,
  `update_project_metadata`
- `crates/bds-core/src/db/queries/generated_file_hash.rs`:
  `delete_generated_file_hash`, `list_generated_file_hashes_by_project`
- `crates/bds-core/src/db/queries/post.rs`: `list_posts_by_project_limited`
- `crates/bds-core/src/db/queries/media.rs`: `list_media_by_project_limited`
- `crates/bds-core/src/db/queries/post_link.rs`: `list_post_backlinks`,
  `list_post_outlinks`
- `crates/bds-core/src/db/from_row.rs`: `notification_entity_to_str`,
  `notification_action_to_str`

**Fix:** delete, along with any tests that exist only to exercise them.

**Special case:** `crates/bds-core/src/db/fts.rs::search_media` has only
test callers. AGENTS.md requires functionality to be tied to UI — check the
allium spec: if media search is specced, wire it into the UI; if not, delete.

## 6. Delete unused EngineContext

`crates/bds-core/src/engine/context.rs` — a "shared context passed to engine
operations" that zero engine operations take (every engine fn takes
`conn`/`project_id`/`data_dir` individually). Its only test asserts that struct
fields hold assigned values.

**Fix:** delete the file, its `pub use` in `crates/bds-core/src/engine/mod.rs`.

## 7. Derive EngineError with thiserror

`crates/bds-core/src/engine/error.rs` hand-writes `Display`, `Error::source`,
and four `From` impls (~85 lines). `DatabaseError` in
`crates/bds-core/src/db/connection.rs` already uses
`#[derive(thiserror::Error)]` — the crate is inconsistent.

**Fix:** convert `EngineError` to a thiserror derive (`#[error(...)]`,
`#[from]`, `#[source]`). Keep the `From<DatabaseError>` mapping and the
reqwest/serde_json/serde_yaml → `Parse` conversions (thiserror `#[from]` can't
map two sources into one variant with `.to_string()`, so those three stay as
small manual impls or become dedicated variants).

## 8. Single source of truth for status/kind enum strings

`PostStatus` ↔ string mapping exists three times:

1. serde `#[serde(rename_all = "lowercase")]` on the enum
   (`crates/bds-core/src/model/post.rs`)
2. `post_status_to_str` / `post_status` in `crates/bds-core/src/db/from_row.rs`
3. the `serde_json::to_string(&status).trim_matches('"')` hack in
   `crates/bds-core/src/util/frontmatter.rs` (`PostFrontmatter::from_post`,
   `TranslationFrontmatter::from_translation`)

**Fix:** give each enum (`PostStatus`, `TemplateKind`, `TemplateStatus`,
`ScriptKind`, `ScriptStatus`) an `as_str()` + `FromStr` impl next to its
definition, use it from the DB layer and frontmatter, delete the duplicates and
the serde_json hack. (Combines well with item 2.)

## 9. Derive frontmatter deserialization (~120 lines)

`crates/bds-core/src/util/frontmatter.rs`: the four `from_yaml` impls each
hand-roll near-identical `get_str`/`get_string_list` closures over
`serde_yaml::Value`.

**Fix:** replace with `#[derive(Deserialize)]` +
`#[serde(rename_all = "camelCase")]` structs and one shared
`deserialize_with` helper for ISO-8601 → unix-ms timestamps. Preserve current
lenient behaviour (numbers/bools coerced to strings where fields expect
strings; missing optional fields default).

**Do NOT touch the serialization side** (`to_yaml`): it is deliberately
hand-rolled for byte-identical golden output against the gray-matter format —
the `golden_output_*` tests assert exact equality with fixture files.

## 10. Minor shrinks

- `crates/bds-core/src/util/checksum.rs`: replace the inline `hex` module with
  `bytes.iter().map(|b| format!("{b:02x}")).collect::<String>()`. (Correctly
  avoids adding a `hex` crate dep — keep it that way.)
- `crates/bds-core/src/i18n/mod.rs`: `RENDER_KEYS` hardcodes 34 keys that must
  be kept in sync with the `.ftl` catalogs by hand. Enumerate the parsed
  `FluentResource` entries of the English render catalog instead, so new keys
  can't be silently missed.
- `crates/bds-core/src/i18n/mod.rs`: `locale_index` can be `locale as usize`
  with explicit discriminants on `UiLocale`.

---

# Test Suite Cleanup

Findings from a full test-suite review (2026-07-18). The suite is largely
healthy: engine/query tests run against real in-memory SQLite, AI tests run the
real HTTP client against a local TCP test server, golden-output tests pin
byte-compatibility with the bDS2 baseline fixtures. No mock-object testing
anti-pattern exists. The problems are hot-air tests (assertions the compiler
already guarantees or that test a literal against itself) and duplication.

## T1. Delete bds-ui/tests/app_smoke.rs entirely

Every test in the file is compile-time noise:

- `message_variants_constructable`, `new_message_variants_constructable`
  (~80 lines): construct `Message` enum variants and discard them — the
  compiler already guarantees this
- `message_clone_works`: tests `#[derive(Clone)]`
- `bds_app_type_is_public`: an inner function that is never called; purely a
  type assertion with no runtime assertion

The file's own header admits `BdsApp::new()` cannot be tested here. The real
smoke coverage lives in `app.rs`'s inline tests (which drive `update()` against
a real in-memory DB via `new_for_tests`). Delete the file.

## T2. Hot air in bds-ui/tests/m2_validation.rs

- `toast_level_variants`, `toast_preserves_message`: assert that a constructor
  stores its arguments. Delete.
- `toast_ids_are_monotonically_increasing`, `fresh_toast_is_not_expired`:
  duplicate the inline tests in `crates/bds-ui/src/state/toast.rs`
  (`toast_ids_are_unique`, `fresh_toast_not_expired`). Keep one location
  (suggest inline), delete the other.
- `menu_actions_that_need_project` / `menu_actions_that_need_tab` /
  `menu_actions_gated_by_offline`: their own comment admits they don't test the
  gating logic — they only re-verify i18n keys (already covered for ALL actions
  in `menu_routing.rs::all_menu_actions_translate_in_all_locales`) and then
  `assert_eq!(array.len(), N)` on the literal array declared five lines above.
  Delete. **Real gap:** the actual enable/disable rules in
  `BdsApp::sync_menu_state` (app.rs) are untested — write a test that drives
  the real method (or extract its decision logic into a testable function).
- `accelerator_actions_match_spec`: asserts a locally declared array has 15
  elements and that i18n keys are non-empty; never touches the real
  accelerator registration in `platform/menu.rs`. Delete, or rewrite to
  inspect the actual menu construction.

## T3. Hot air in bds-core/tests/spec_claims.rs

- `remaining_value_specs_match` contains
  `assert_eq!(["en", "de", "fr", "it", "es"].len(), 5)` — a literal tested
  against itself. Drop that line (the serde asserts around it are fine).
- `slug_frozen_after_publish_semantics`: the name promises the slug-freeze
  invariant; the body only asserts `published_at` is Some/None after insert
  (already covered by roundtrip tests elsewhere). Either write the real test —
  attempt a slug change on a published post through the engine API and assert
  it's rejected — or delete.

## T4. Vacuous inline tests in bds-core/src/i18n/mod.rs

- `ui_locale_is_independent_type`: asserts `"de" != "fr"`. Delete.
- `translate_falls_back_to_english`: identical call and assertion as
  `translate_menu_labels`, and it does NOT test fallback — the key it uses has
  a German translation. Replace with a key that exists only in en.ftl (that
  actually exercises `add_resource_overriding` fallback), or delete.
- `detect_os_locale_does_not_panic`: marginal smoke; keep or drop, zero cost.

## T5. Tests that only exist to cover dead code

Deleted together with their subjects (see items 4–6 above):

- `engine/task.rs`: `progress_throttle_initial_reports`,
  `progress_throttle_suppresses_rapid` (test the dead `ProgressThrottle`)
- `engine/context.rs`: `context_holds_references` (asserts struct fields hold
  assigned values)
- `engine/error.rs`: `display_variants`, `from_io_error` become
  derive-testing once EngineError moves to thiserror; delete with item 7

## T6. Change-detector test in bds-ui/tests/menu_routing.rs

`menu_action_count_matches_spec` asserts `MenuAction::ALL.len() == 28`. It must
be hand-bumped on every menu change and only ever catches forgetting to update
itself; the useful properties (every action has a key, keys unique, all locales
translate) are covered by the other three tests in the file. Delete.

## Explicitly fine (do not "clean up")

- `tests/orm_boundary.rs`, `tests/i18n_completeness.rs`,
  `tests/packaging_assets.rs`: lints-as-tests. They don't test runtime
  behaviour but mechanically enforce AGENTS.md rules (no raw SQL outside the
  backend boundary, no untranslated keys, packaging config integrity). Keep.
- `fixture_readability.rs::fixture_database_files_are_not_modified_by_tests`:
  meta-test guarding fixture integrity for the whole compat suite. Keep.
- `m1_validation.rs`: the per-field diff-detection matrix looks repetitive but
  each test pins one metadata-diff field required by the spec. Keep.
- AI engine tests spawn a real local TCP server — fake at the network
  boundary, real production client code exercised. This is the right pattern.
- `util/timestamp.rs::now_is_recent` looks trivial but guards
  milliseconds-vs-seconds confusion. Keep.

---

## Explicitly out of scope

- `crates/bds-cli` being a stub: intentional, part of upcoming extensions.
- The hand-rolled YAML *serialization* in frontmatter.rs and sidecar.rs:
  required for byte-compatibility with the baseline output.
