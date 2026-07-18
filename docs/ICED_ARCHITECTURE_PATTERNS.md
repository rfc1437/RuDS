# Iced Architecture Patterns (bDS Rust Rewrite)

Iced 0.13, wgpu renderer, muda for native menus, rfd for file dialogs.

## Message Design

- Root `Message` enum in `app.rs`: `MenuEvent(MenuId)`, `Noop`, plus child view variants.
- Child view messages wrapped in parent enum variants: `Message::Editor(editor::Message)`.
- Keep messages flat where possible -- avoid deep nesting beyond two levels.
- Use `Task<Message>` for async operations (file I/O, dialogs, database queries).
- Noop absorbs events that need no action (e.g., unhandled menu IDs).

## Subscription Model

- Menu events: `iced::event::listen_with` polling `MenuEvent::receiver().try_recv()` each tick.
- Future: filesystem watcher subscription for external db/file changes.
- Subscriptions are declarative -- `subscription()` returns the full set every frame; Iced diffs internally.
- Each subscription keyed by a unique ID to avoid duplicates.

## Custom Widget Pattern (CodeEditor)

- Implements `iced::advanced::Widget<Message, Theme, Renderer>`.
- Persistent state via `widget::tree::State` holding `EditorState` (cursor pos, selection, scroll offset).
- `tag()` returns `TypeId::of::<EditorState>()` for widget tree identity.
- `state()` returns `State::new(EditorState::default())` for initialization.
- `on_event()` handles keyboard/mouse input, returns `Status::Captured` or `Status::Ignored`.
- `draw()` uses `renderer.fill_quad()` for backgrounds/cursors/gutters, `renderer.fill_text()` for content.
- Takes `&mut EditorBuffer` reference for direct mutation on input -- buffer is not owned by the widget.
- Focus management: `is_focused` flag in `EditorState`, set on mouse press inside bounds, cleared on press outside.

## Platform Integration

- muda menus built once at startup; call `init_for_nsapp()` on macOS before event loop.
- Menu IDs stored in a map (`HashMap<MenuId, Message>`) for routing `MenuEvent` to app `Message`.
- macOS lifecycle hooks (`application:openFile:`, `openURLs:`) via objc2 -- deferred to M2.
- rfd dialogs: `rfd::AsyncFileDialog` spawned behind `Task::perform`, result mapped to a `Message` variant.

## Rendering

- Iced uses cosmic-text internally for text shaping and layout.
- Custom widgets call `renderer.fill_text()` with a `Text` struct: font, size, bounds, horizontal/vertical alignment.
- Use `Font::MONOSPACE` for the editor, default system font for UI chrome.
- `renderer.fill_quad()` for solid-color rectangles: backgrounds, cursor bar, gutter column, selections.
- Clipping: call `renderer.with_layer()` to restrict drawing to widget bounds.

## State Management

- Top-level app state lives in `BdsApp` struct (db connection, open buffers, UI flags).
- Child widget state in `widget::tree::State` -- survives across `view()` calls as long as widget identity matches.
- `EditorBuffer` (rope-based text buffer via ropey) owned by `BdsApp`, passed as `&mut` to widget on `view()` and `on_event()`.
- Database connection (`bds_core::db::Database`) opened once at startup, held in `BdsApp`, not cloned.
- No global mutable state -- everything flows through the `Message` -> `update()` -> `view()` cycle.
