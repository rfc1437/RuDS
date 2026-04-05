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
}

/// Persistent widget state across frames.
#[derive(Default)]
struct EditorState {
    is_focused: bool,
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

        MonoMetrics { char_width, line_height }
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

/// A syntax-highlighting code editor widget for Iced.
///
/// M0 PoC: renders highlighted text with line numbers, handles keyboard
/// input for basic editing, supports cursor movement and vertical scrolling.
pub struct CodeEditor<'a, Message> {
    buffer: &'a mut EditorBuffer,
    highlighter: &'a Highlighter,
    extension: &'a str,
    on_change: Option<Box<dyn Fn(EditorMessage) -> Message + 'a>>,
}

impl<'a, Message> CodeEditor<'a, Message> {
    pub fn new(
        buffer: &'a mut EditorBuffer,
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
        let _state = tree.state.downcast_ref::<EditorState>();

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
        let (cursor_line, cursor_col) = self.buffer.cursor();
        let scroll = self.buffer.scroll_offset();
        let visible_lines = (bounds.height / metrics.line_height) as usize + 1;

        let font = iced::Font::MONOSPACE;

        // Render visible lines
        for vis_idx in 0..visible_lines {
            let line_idx = scroll + vis_idx;
            if line_idx >= self.buffer.line_count() {
                break;
            }

            let y = bounds.y + vis_idx as f32 * metrics.line_height;
            if y + metrics.line_height < bounds.y || y > bounds.y + bounds.height {
                continue;
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
                    line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.line_height)),
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

            // Line content
            if let Some(line) = self.buffer.line(line_idx) {
                let mut line_text: String = line.chars().collect();
                // Strip trailing newline for display
                if line_text.ends_with('\n') {
                    line_text.pop();
                }

                let text_x = bounds.x + GUTTER_WIDTH + 8.0;
                renderer.fill_text(
                    text::Text {
                        content: line_text,
                        bounds: Size::new(bounds.width - GUTTER_WIDTH - 8.0, metrics.line_height),
                        size: Pixels(FONT_SIZE),
                        line_height: iced::widget::text::LineHeight::Absolute(Pixels(metrics.line_height)),
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

                // Draw cursor on current line
                if line_idx == cursor_line {
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
    }

    fn on_event(
        &mut self,
        tree: &mut widget::Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> Status {
        let state = tree.state.downcast_mut::<EditorState>();
        let bounds = layout.bounds();
        let metrics = mono_metrics();

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.is_focused = true;
                    // Place cursor at click position
                    if let Some(pos) = cursor.position_in(bounds) {
                        let line = (pos.y / metrics.line_height) as usize + self.buffer.scroll_offset();
                        let col = ((pos.x - GUTTER_WIDTH - 8.0).max(0.0) / metrics.char_width) as usize;
                        self.buffer.set_cursor(line, col);
                    }
                    return Status::Captured;
                } else {
                    state.is_focused = false;
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -(y * 3.0) as isize,
                    mouse::ScrollDelta::Pixels { y, .. } => -(y / metrics.line_height) as isize,
                };
                self.buffer.scroll_by(lines);
                return Status::Captured;
            }
            Event::Keyboard(keyboard::Event::KeyPressed { key, .. }) if state.is_focused => {
                match key {
                    keyboard::Key::Named(keyboard::key::Named::ArrowUp) => {
                        self.buffer.move_up();
                        let vis = (bounds.height / metrics.line_height) as usize;
                        self.buffer.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                        self.buffer.move_down();
                        let vis = (bounds.height / metrics.line_height) as usize;
                        self.buffer.ensure_cursor_visible(vis);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowLeft) => {
                        self.buffer.move_left();
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowRight) => {
                        self.buffer.move_right();
                    }
                    keyboard::Key::Named(keyboard::key::Named::Home) => {
                        self.buffer.move_home();
                    }
                    keyboard::Key::Named(keyboard::key::Named::End) => {
                        self.buffer.move_end();
                    }
                    keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                        self.buffer.backspace();
                    }
                    keyboard::Key::Named(keyboard::key::Named::Delete) => {
                        self.buffer.delete_forward();
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter) => {
                        self.buffer.insert("\n");
                    }
                    keyboard::Key::Character(ref c) => {
                        self.buffer.insert(c);
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

impl<'a, Message, Renderer> From<CodeEditor<'a, Message>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + text::Renderer<Font = iced::Font> + 'a,
    Message: 'a,
{
    fn from(editor: CodeEditor<'a, Message>) -> Self {
        Self::new(editor)
    }
}
