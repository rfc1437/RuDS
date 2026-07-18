# UI Style Guide

Use the post editor as the visual reference for every screen. New UI must feel like part of the same application in both the sidebar and editor area.

## Source of truth

- Reuse the primitives in `crates/bds-ui/src/components/inputs.rs`.
- Reuse the code-editor theme in `crates/bds-editor/src/widget.rs`.
- Follow `crates/bds-ui/src/views/post_editor.rs` for editor hierarchy and spacing.
- Extend a shared primitive when several screens need the same treatment; do not copy color or border closures into each view.

## Layout

- Use 16 px outer editor padding and 8–12 px spacing between groups.
- Put the editor header, metadata, summaries, previews, and content editors on `inputs::card` surfaces.
- Use `inputs::toolbar` for content modes and related actions.
- Use `inputs::disclosure_button` inside a compact card for collapsible sections. Expanded content belongs in a separate card directly below it.
- Keep sidebars visually quieter than editor content. Sidebar rows use 6 px corner radii, 5×8 px padding, and a distinct selected state.
- Avoid separators and boxes when spacing or a shared surface already communicates the grouping.

## Controls

- Text fields use `inputs::labeled_input` or `inputs::field_style`.
- Selects use `inputs::labeled_select`; multiline fields use `inputs::text_editor_style`.
- Primary actions use `inputs::primary_button`, secondary actions use `inputs::secondary_button`, and destructive actions use `inputs::danger_button`.
- Controls have 6 px corner radii; cards use 10 px; chips and badges may use pill radii.
- Keep one visually dominant primary action per action group.
- Put infrequent operations in Quick Actions instead of adding buttons to metadata forms.
- Do not use Iced's default field or button styling in application screens.

## Editor consistency

Every entity editor should have the same order:

1. Header card with title/status and actions.
2. Metadata card or collapsible metadata section.
3. Content toolbar when the content has modes or insertion actions.
4. Content editor or preview card that receives the remaining height.
5. Validation and timestamps as quiet supporting information.

Labels, placeholders, actions, empty states, and status text must use the UI localization system.

## Before merging UI work

- Compare the changed screen with the post editor at the same window size.
- Check its sidebar as well as its editor area.
- Check collapsed and expanded disclosures, hover/focus states, empty states, and disabled actions.
- Check media, script, template, Settings, and Tags screens when changing a shared primitive.
- Run `cargo test --workspace`, `cargo build --workspace`, and visually inspect the macOS app bundle.
