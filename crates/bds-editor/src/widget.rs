use std::cell::RefCell;
use std::sync::OnceLock;

use cosmic_text::{Attrs, Buffer as CosmicBuffer, Family, FontSystem, Metrics, Shaping};
use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::text;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::Status;
use iced::keyboard;
use iced::mouse;
use iced::{Color, Element, Event, Length, Pixels, Point, Rectangle, Size, Theme};

use crate::buffer::EditorBuffer;
use crate::highlight::Highlighter;

/// Messages emitted by the CodeEditor widget.
#[derive(Debug, Clone)]
pub enum EditorMessage {
    ContentChanged(String),
    SaveRequested,
}

/// Persistent widget state across frames.
#[derive(Default)]
struct EditorState {
    is_focused: bool,
    /// Track drag state for click-and-drag selection
    is_dragging: bool,
    /// Last click time for double-click detection (ms)
    last_click_time: Option<std::time::Instant>,
    last_click_line: usize,
    last_click_col: usize,
}

/// Font metrics measured via cosmic-text, cached globally.
pub struct MonoMetrics {
    pub char_width: f32,
    pub line_height: f32,
}

static MONO_METRICS: OnceLock<MonoMetrics> = OnceLock::new();

/// Measure the monospace font metrics using cosmic-text.
pub fn mono_metrics() -> &'static MonoMetrics {
    MONO_METRICS.get_or_init(|| {
        let mut font_system = FontSystem::new();
        let metrics = Metrics::new(FONT_SIZE, (FONT_SIZE * 1.43).ceil());
        let mut buffer = CosmicBuffer::new(&mut font_system, metrics);
        buffer.set_size(&mut font_system, Some(500.0), Some(100.0));
        buffer.set_text(
            &mut font_system,
            "M",
            Attrs::new().family(Family::Monospace),
            Shaping::Advanced,
        );
        buffer.shape_until_scroll(&mut font_system, false);

        let mut char_width = FONT_SIZE * 0.6; // fallback
        let mut line_height = (FONT_SIZE * 1.43).ceil(); // fallback

        for run in buffer.layout_runs() {
            if let Some(glyph) = run.glyphs.first() {
                char_width = glyph.w;
            }
            line_height = run.line_height;
            break;
        }

        MonoMetrics {
            char_width,
            line_height,
        }
    })
}

const GUTTER_WIDTH: f32 = 50.0;
const FONT_SIZE: f32 = 14.0;
const BG_COLOR: Color = Color::from_rgb(0.18, 0.20, 0.25);
const GUTTER_BG: Color = Color::from_rgb(0.15, 0.17, 0.21);
const TEXT_COLOR: Color = Color::from_rgb(0.85, 0.85, 0.85);
const GUTTER_TEXT: Color = Color::from_rgb(0.45, 0.48, 0.55);
const CURSOR_COLOR: Color = Color::from_rgb(0.9, 0.9, 0.2);
const ACTIVE_LINE_NUM: Color = Color::from_rgb(0.75, 0.78, 0.85);
const SELECTION_BG: Color = Color::from_rgba(0.26, 0.54, 0.79, 0.40);

/// Convert syntect RGBA color to Iced Color.
fn syntect_to_iced(c: syntect::highlighting::Color) -> Color {
    Color::from_rgba(
        c.r as f32 / 255.0,
        c.g as f32 / 255.0,
        c.b as f32 / 255.0,
        c.a as f32 / 255.0,
    )
}

/// A syntax-highlighting code editor widget for Iced.
pub struct CodeEditor<'a, Message> {
    buffer: &'a RefCell<EditorBuffer>,
    highlighter: &'a Highlighter,
    extension: &'a str,
    on_change: Option<Box<dyn Fn(EditorMessage) -> Message + 'a>>,
}

impl<'a, Message> CodeEditor<'a, Message> {
    pub fn new(
        buffer: &'a RefCell<EditorBuffer>,
        highlighter: &'a Highlighter,
        extension: &'a str,
    ) -> Self {
        Self {
            buffer,
            highlighter,
            extension,
            on_change: None,
        }
    }

    pub fn on_change(mut self, f: impl Fn(EditorMessage) -> Message + 'a) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }
}

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer> for CodeEditor<'a, Message>
where
    Renderer: renderer::Renderer + text::Renderer<Font = iced::Font>,
    Message: 'a,
{
    fn tag(&self) -> widget::tree::Tag {
        widget::tree::Tag::of::<EditorState>()
    }

    fn state(&self) -> widget::tree::State {
        widget::tree::State::new(EditorState::default())
    }

    fn size(&self) -> Size<Length> {
        Size::new(Length::Fill, Length::Fill)
    }

    fn layout(
        &self,
        _tree: &mut widget::Tree,
        _renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        let size = limits.max();
        layout::Node::new(size)
    }

    fn draw(
        &self,
        tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();
        let state = tree.state.downcast_ref::<EditorState>();
        let buf = self.buffer.borrow();

        // Background
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            },
            BG_COLOR,
        );

        // Gutter background
        let gutter_bounds = Rectangle {
            width: GUTTER_WIDTH,
            ..bounds
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: gutter_bounds,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            },
            GUTTER_BG,
        );

        let metrics = mono_metrics();
        let (cursor_line, cursor_col) = buf.cursor();
        let scroll = buf.scroll_offset();
        let visible_lines = (bounds.height / metrics.line_height) as usize + 1;
        let selection = buf.selection();

        // Pre-compute highlighted lines for visible range
        let syntax = self.highlighter.syntax_for_extension(self.extension);
        let full_text = buf.text();
        let highlighted = self.highlighter.highlight_lines(&full_text, syntax);

        let font = iced::Font::MONOSPACE;

        // Render visible lines
        for vis_idx in 0..visible_lines {
            let line_idx = scroll + vis_idx;
            if line_idx >= buf.line_count() {
                break;
            }

            let y = bounds.y + vis_idx as f32 * metrics.line_height;
            if y + metrics.line_height < bounds.y || y > bounds.y + bounds.height {
                continue;
            }

            let text_x = bounds.x + GUTTER_WIDTH + 8.0;

            // Draw selection highlight for this line
            if let Some(sel) = selection {
                if !sel.is_empty() {
                    let (start, end) = sel.ordered();
                    let line_len = buf
                        .line(line_idx)
                        .map(|l| {
                            let len = l.len_chars();
                            if len > 0 && l.char(len - 1) == '\n' {
                                len - 1
                            } else {
                                len
                            }
                        })
                        .unwrap_or(0);

                    if line_idx >= start.0 && line_idx <= end.0 {
                        let sel_start_col = if line_idx == start.0 { start.1 } else { 0 };
                        let sel_end_col = if line_idx == end.0 {
                            end.1
                        } else {
                            line_len + 1
                        };
                        let sel_x = text_x + sel_start_col as f32 * metrics.char_width;
                        let sel_w =
                            (sel_end_col - sel_start_col) as f32 * metrics.char_width;
                        if sel_w > 0.0 {
                            renderer.fill_quad(
                                renderer::Quad {
                                    bounds: Rectangle {
                                        x: sel_x,
                                        y,
                                        width: sel_w,
                                        height: metrics.line_height,
                                    },
                                    border: iced::Border::default(),
                                    shadow: iced::Shadow::default(),
                                },
                                SELECTION_BG,
                            );
                        }
                    }
                }
            }

            // Line number
            let line_num = format!("{:>4}", line_idx + 1);
            let num_color = if line_idx == cursor_line {
                ACTIVE_LINE_NUM
            } else {
                GUTTER_TEXT
            };
            renderer.fill_text(
                text::Text {
                    content: line_num,
                    bounds: Size::new(GUTTER_WIDTH - 8.0, metrics.line_height),
                    size: Pixels(FONT_SIZE),
                    line_height: iced::widget::text::LineHeight::Absolute(Pixels(
                        metrics.line_height,
                    )),
                    font,
                    horizontal_alignment: iced::alignment::Horizontal::Right,
                    vertical_alignment: iced::alignment::Vertical::Top,
                    shaping: iced::widget::text::Shaping::Basic,
                    wrapping: iced::widget::text::Wrapping::None,
                },
                Point::new(bounds.x, y),
                num_color,
                bounds,
            );

            // Line content with syntax highlighting
            if line_idx < highlighted.len() {
                let spans = &highlighted[line_idx];
                let mut x_off = text_x;
                for (style, span_text) in spans {
                    let mut display_text = span_text.clone();
                    if display_text.ends_with('\n') {
                        display_text.pop();
                    }
                    if display_text.is_empty() {
                        continue;
                    }
                    let color = syntect_to_iced(style.foreground);
                    let span_width = display_text.len() as f32 * metrics.char_width;
                    renderer.fill_text(
                        text::Text {
                            content: display_text,
                            bounds: Size::new(span_width + metrics.char_width, metrics.line_height),
                            size: Pixels(FONT_SIZE),
                            line_height: iced::widget::text::LineHeight::Absolute(Pixels(
                                metrics.line_height,
                            )),
                            font,
                            horizontal_alignment: iced::alignment::Horizontal::Left,
                            vertical_alignment: iced::alignment::Vertical::Top,
                            shaping: iced::widget::text::Shaping::Advanced,
                            wrapping: iced::widget::text::Wrapping::None,
                        },
                        Point::new(x_off, y),
                        color,
                        bounds,
                    );
                    x_off += span_width;
                }
            } else if let Some(line) = buf.line(line_idx) {
                // Fallback: plain text rendering
                let mut line_text: String = line.chars().collect();
                if line_text.ends_with('\n') {
                    line_text.pop();
                }
                renderer.fill_text(
                    text::Text {
                        content: line_text,
                        bounds: Size::new(
                            bounds.width - GUTTER_WIDTH - 8.0,
                            metrics.line_height,
                        ),
                        size: Pixels(FONT_SIZE),
                        line_height: iced::widget::text::LineHeight::Absolute(Pixels(
                            metrics.line_height,
                        )),
                        font,
                        horizontal_alignment: iced::alignment::Horizontal::Left,
                        vertical_alignment: iced::alignment::Vertical::Top,
                        shaping: iced::widget::text::Shaping::Advanced,
                        wrapping: iced::widget::text::Wrapping::None,
                    },
                    Point::new(text_x, y),
                    TEXT_COLOR,
                    bounds,
                );
            }

            // Draw cursor on current line
            if line_idx == cursor_line && state.is_focused {
                let cursor_x = text_x + cursor_col as f32 * metrics.char_width;
                renderer.fill_quad(
                    renderer::Quad {
                        bounds: Rectangle {
                            x: cursor_x,
                            y,
                            width: 2.0,
                            height: metrics.line_height,
                        },
                        border: iced::Border::default(),
                        shadow: iced::Shadow::default(),
                    },
                    CURSOR_COLOR,
                );
            }
        }
    }

    fn on_event(
        &mut self,
        tree: &mut widget::Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> Status {
        let state = tree.state.downcast_mut::<EditorState>();
        let bounds = layout.bounds();
        let metrics = mono_metrics();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.is_focused = true;
                    state.is_dragging = true;

                    if let Some(pos) = cursor.position_in(bounds) {
                        let mut buf = self.buffer.borrow_mut();
                        let line = (pos.y / metrics.line_height) as usize
                            + buf.scroll_offset();
                        let col = ((pos.x - GUTTER_WIDTH - 8.0).max(0.0) / metrics.char_width)
                            as usize;

                        // Double-click detection
                        let now = std::time::Instant::now();
                        let is_double_click = state
                            .last_click_time
                            .map(|t| now.duration_since(t).as_millis() < 400)
                            .unwrap_or(false)
                            && state.last_click_line == line
                            && (state.last_click_col as isize - col as isize).unsigned_abs() < 3;

                        if is_double_click {
                            buf.select_word_at(line, col);
                            state.last_click_time = None; // reset
                        } else {
                            buf.clear_selection();
                            buf.set_cursor(line, col);
                            state.last_click_time = Some(now);
                            state.last_click_line = line;
                            state.last_click_col = col;
                        }
                    }
                    return Status::Captured;
                } else {
                    state.is_focused = false;
                }
            }
            Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                state.is_dragging = false;
            }
            Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                if state.is_dragging && state.is_focused {
                    if let Some(pos) = cursor.position_in(bounds) {
                        let mut buf = self.buffer.borrow_mut();
                        let line = (pos.y / metrics.line_height) as usize
                            + buf.scroll_offset();
                        let col = ((pos.x - GUTTER_WIDTH - 8.0).max(0.0) / metrics.char_width)
                            as usize;
                        // Extend selection by simulating shift+movement
                        let clamped_line = line.min(buf.line_count().saturating_sub(1));
                        let clamped_col = if clamped_line < buf.line_count() {
                            col.min(
                                buf
                                    .line(clamped_line)
                                    .map(|l| {
                                        let len = l.len_chars();
                                        if len > 0 && l.char(len - 1) == '\n' {
                                            len - 1
                                        } else {
                                            len
                                        }
                                    })
                                    .unwrap_or(0),
                            )
                        } else {
                            0
                        };
                        let anchor_line = buf
                            .selection()
                            .map(|s| s.anchor_line)
                            .unwrap_or(buf.cursor().0);
                        let anchor_col = buf
                            .selection()
                            .map(|s| s.anchor_col)
                            .unwrap_or(buf.cursor().1);
                        buf.set_selection(
                                anchor_line,
                                anchor_col,
                                clamped_line,
                                clamped_col,
                            );
                        buf.set_cursor(clamped_line, clamped_col);
                    }
                    return Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -(y * 3.0) as isize,
                    mouse::ScrollDelta::Pixels { y, .. } => {
                        -(y / metrics.line_height) as isize
                    }
                };
                self.buffer.borrow_mut().scroll_by(lines);
                return Status::Captured;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key, modifiers, ..
            }) if state.is_focused => {
                let vis = (bounds.height / metrics.line_height) as usize;
                let is_cmd = modifiers.command();
                let is_shift = modifiers.shift();
                let is_alt = modifiers.alt();

                match key {
                    // Cmd+Z = undo, Cmd+Shift+Z = redo
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "z" => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            if is_shift { buf.redo(); } else { buf.undo(); }
                        }
                        self.emit_change(shell);
                    }
                    // Cmd+Y = redo (Windows convention)
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "y" => {
                        self.buffer.borrow_mut().redo();
                        self.emit_change(shell);
                    }
                    // Cmd+A = select all
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "a" => {
                        self.buffer.borrow_mut().select_all();
                    }
                    // Cmd+C = copy
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "c" => {
                        let text = self.buffer.borrow().selected_text();
                        if !text.is_empty() {
                            clipboard.write(iced::advanced::clipboard::Kind::Standard, text);
                        }
                    }
                    // Cmd+X = cut
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "x" => {
                        let text = self.buffer.borrow().selected_text();
                        if !text.is_empty() {
                            clipboard.write(iced::advanced::clipboard::Kind::Standard, text);
                            self.buffer.borrow_mut().delete_selection();
                            self.emit_change(shell);
                        }
                    }
                    // Cmd+V = paste
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "v" => {
                        if let Some(text) =
                            clipboard.read(iced::advanced::clipboard::Kind::Standard)
                        {
                            self.buffer.borrow_mut().insert(&text);
                            self.emit_change(shell);
                        }
                    }
                    // Cmd+S = save
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "s" => {
                        if let Some(ref on_change) = self.on_change {
                            shell.publish((on_change)(EditorMessage::SaveRequested));
                        }
                    }
                    // Arrow keys with modifiers
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_up();
                        } else {
                            buf.move_up();
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_down();
                        } else {
                            buf.move_down();
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift && is_alt {
                            buf.select_word_left();
                        } else if is_shift {
                            buf.select_left();
                        } else if is_alt {
                            buf.move_word_left();
                        } else {
                            buf.move_left();
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift && is_alt {
                            buf.select_word_right();
                        } else if is_shift {
                            buf.select_right();
                        } else if is_alt {
                            buf.move_word_right();
                        } else {
                            buf.move_right();
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Home) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_home();
                        } else {
                            buf.move_home();
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::End) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_end();
                        } else {
                            buf.move_end();
                        }
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageUp) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_page_up(vis);
                        } else {
                            buf.move_page_up(vis);
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageDown) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_page_down(vis);
                        } else {
                            buf.move_page_down(vis);
                        }
                        buf.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            if is_alt {
                                // Option+Backspace = delete word left
                                buf.select_word_left();
                                buf.delete_selection();
                            } else {
                                buf.backspace();
                            }
                        }
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Delete) => {
                        self.buffer.borrow_mut().delete_forward();
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter) => {
                        self.buffer.borrow_mut().insert("\n");
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Tab) => {
                        self.buffer.borrow_mut().insert("    ");
                        self.emit_change(shell);
                    }
                    keyboard::Key::Character(ref c) if !is_cmd => {
                        self.buffer.borrow_mut().insert(c);
                        self.emit_change(shell);
                    }
                    _ => return Status::Ignored,
                }
                return Status::Captured;
            }
            _ => {}
        }
        Status::Ignored
    }
}

impl<'a, Message> CodeEditor<'a, Message> {
    fn emit_change(&self, shell: &mut Shell<'_, Message>) {
        if let Some(ref on_change) = self.on_change {
            let text = self.buffer.borrow().text();
            shell.publish((on_change)(EditorMessage::ContentChanged(text)));
        }
    }
}

impl<'a, Message, Renderer> From<CodeEditor<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + text::Renderer<Font = iced::Font> + 'a,
    Message: 'a,
{
    fn from(editor: CodeEditor<'a, Message>) -> Self {
        Self::new(editor)
    }
}
