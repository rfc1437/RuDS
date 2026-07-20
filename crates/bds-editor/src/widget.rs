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
use iced::{Color, Element, Event, Length, Pixels, Point, Rectangle, Shadow, Size, Theme, Vector};

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
    /// Scrollbar drag state: Some(offset from thumb top to click y)
    scrollbar_drag: Option<f32>,
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

        if let Some(run) = buffer.layout_runs().next() {
            if let Some(glyph) = run.glyphs.first() {
                char_width = glyph.w;
            }
            line_height = run.line_height;
        }

        MonoMetrics {
            char_width,
            line_height,
        }
    })
}

const GUTTER_WIDTH: f32 = 44.0;
const TEXT_PADDING: f32 = 12.0;
const FONT_SIZE: f32 = 14.0;
const TEXT_COLOR: Color = rgb8(0xD4, 0xD4, 0xD4);
const GUTTER_TEXT: Color = rgb8(0x85, 0x85, 0x85);
const CURSOR_COLOR: Color = rgb8(0xAE, 0xAF, 0xAD);
const ACTIVE_LINE_NUM: Color = rgb8(0xC6, 0xC6, 0xC6);
const SELECTION_BG: Color = rgba8(0x26, 0x4F, 0x78, 0.85);
const SCROLLBAR_WIDTH: f32 = 10.0;
const SCROLLBAR_TRACK: Color = Color::TRANSPARENT;
const SCROLLBAR_THUMB: Color = rgba8(0x79, 0x79, 0x79, 0.45);
const SCROLLBAR_THUMB_HOVER: Color = rgba8(0x79, 0x79, 0x79, 0.75);
const MIN_THUMB_HEIGHT: f32 = 20.0;

const fn rgb8(red: u8, green: u8, blue: u8) -> Color {
    rgba8(red, green, blue, 1.0)
}

const fn rgba8(red: u8, green: u8, blue: u8, alpha: f32) -> Color {
    Color {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: alpha,
    }
}

fn committed_text_input(text: Option<&str>, is_command_shortcut: bool) -> Option<&str> {
    if is_command_shortcut {
        return None;
    }

    let text = text?;
    if text.is_empty() || !text.chars().any(|character| !character.is_control()) {
        return None;
    }

    Some(text)
}

/// Convert syntect RGBA color to Iced Color.
fn syntect_to_iced(c: syntect::highlighting::Color) -> Color {
    let (red, green, blue) = match (c.r, c.g, c.b) {
        // Translate the base16-ocean theme to the bDS2/VS Dark editor palette.
        (0x65, 0x73, 0x7E) => (0x6A, 0x99, 0x55), // comments
        (0xB4, 0x8E, 0xAD) => (0xC5, 0x86, 0xC0), // keywords
        (0xA3, 0xBE, 0x8C) => (0xCE, 0x91, 0x78), // strings
        (0xD0, 0x87, 0x70) => (0xB5, 0xCE, 0xA8), // numbers
        (0xEB, 0xCB, 0x8B) => (0xDC, 0xDC, 0xAA), // functions
        (0x96, 0xB5, 0xB4) => (0x4E, 0xC9, 0xB0), // types
        (0xBF, 0x61, 0x6A) => (0x56, 0x9C, 0xD6), // tags
        (0x8F, 0xA1, 0xB3) => (0x9C, 0xDC, 0xFE), // attributes and links
        (0xAB, 0x79, 0x67) => (0xCE, 0x91, 0x78), // attribute values
        (0xC0, 0xC5, 0xCE) | (0xA7, 0xAD, 0xBA) | (0xDF, 0xE1, 0xE8) | (0xEF, 0xF1, 0xF5) => {
            (0xD4, 0xD4, 0xD4)
        }
        _ => (c.r, c.g, c.b),
    };
    rgba8(red, green, blue, c.a as f32 / 255.0)
}

/// Compute word-wrap break offsets for a line. Returns the char offsets where
/// each visual line starts, e.g. [0, 42, 80]. Always starts with 0.
/// Breaks at the last space before `max_chars`; falls back to hard break if
/// there is no space (a single word longer than `max_chars`).
fn word_wrap_breaks(line_text: &str, max_chars: usize) -> Vec<usize> {
    if max_chars == 0 {
        return vec![0];
    }
    let chars: Vec<char> = line_text.chars().filter(|c| *c != '\n').collect();
    let total = chars.len();
    if total <= max_chars {
        return vec![0];
    }
    let mut breaks = vec![0usize];
    let mut pos = 0usize;
    while pos < total {
        let remaining = total - pos;
        if remaining <= max_chars {
            break;
        }
        // Look for the last space within [pos..pos+max_chars]
        let window_end = pos + max_chars;
        let mut break_at = None;
        for i in (pos..window_end).rev() {
            if chars[i] == ' ' || chars[i] == '\t' {
                break_at = Some(i + 1); // break after the space
                break;
            }
        }
        let next = break_at.unwrap_or(window_end); // hard break if no space
        breaks.push(next);
        pos = next;
    }
    breaks
}

/// Extract the line text (without trailing newline) for a logical line.
fn line_text_for(buf: &EditorBuffer, line_idx: usize) -> String {
    buf.line(line_idx)
        .map(|l| {
            let s: String = l.chars().collect();
            if s.ends_with('\n') {
                s[..s.len() - 1].to_string()
            } else {
                s
            }
        })
        .unwrap_or_default()
}

/// Count total visual lines across all logical lines, accounting for word wrap.
fn total_visual_lines(buf: &EditorBuffer, max_chars: usize) -> usize {
    if max_chars == 0 {
        return buf.line_count();
    }
    let mut total = 0usize;
    for line_idx in 0..buf.line_count() {
        let text = line_text_for(buf, line_idx);
        total += word_wrap_breaks(&text, max_chars).len();
    }
    total
}

/// Convert a logical (line, col) to a visual line index, accounting for word wrap.
fn logical_to_visual(buf: &EditorBuffer, line: usize, col: usize, max_chars: usize) -> usize {
    if max_chars == 0 {
        return line;
    }
    let mut vis = 0usize;
    for idx in 0..buf.line_count() {
        let text = line_text_for(buf, idx);
        let breaks = word_wrap_breaks(&text, max_chars);
        if idx == line {
            // Find which sub-line the column falls on
            for (i, &start) in breaks.iter().enumerate() {
                let end = if i + 1 < breaks.len() {
                    breaks[i + 1]
                } else {
                    usize::MAX
                };
                if (col >= start && col < end) || i + 1 == breaks.len() {
                    return vis + i;
                }
            }
            return vis;
        }
        vis += breaks.len();
    }
    vis
}

/// Map a visual row index (from scroll top) to (logical_line, char_offset).
/// Returns None if beyond end of buffer.
fn visual_to_logical(
    buf: &EditorBuffer,
    scroll: usize,
    visual_row: usize,
    max_chars: usize,
) -> Option<(usize, usize)> {
    if max_chars == 0 {
        // No wrapping — direct 1:1 mapping
        let line = scroll + visual_row;
        return if line < buf.line_count() {
            Some((line, 0))
        } else {
            None
        };
    }
    let target = scroll + visual_row;
    let mut vis_count = 0usize;
    for line_idx in 0..buf.line_count() {
        let text = line_text_for(buf, line_idx);
        let breaks = word_wrap_breaks(&text, max_chars);
        let wrap_count = breaks.len();
        if vis_count + wrap_count > target {
            let sub = target - vis_count;
            return Some((line_idx, breaks[sub]));
        }
        vis_count += wrap_count;
    }
    None
}

/// A syntax-highlighting code editor widget for Iced.
pub struct CodeEditor<'a, Message> {
    buffer: &'a RefCell<EditorBuffer>,
    highlighter: &'a Highlighter,
    extension: &'a str,
    on_change: Option<Box<dyn Fn(EditorMessage) -> Message + 'a>>,
    word_wrap: bool,
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
            word_wrap: false,
        }
    }

    pub fn on_change(mut self, f: impl Fn(EditorMessage) -> Message + 'a) -> Self {
        self.on_change = Some(Box::new(f));
        self
    }

    pub fn word_wrap(mut self, enabled: bool) -> Self {
        self.word_wrap = enabled;
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
        theme: &Theme,
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
                border: iced::Border {
                    color: Color::from_rgb8(0x35, 0x35, 0x35),
                    width: 1.0,
                    radius: 8.0.into(),
                },
                shadow: Shadow {
                    color: Color::from_rgba(0.0, 0.0, 0.0, 0.32),
                    offset: Vector::new(0.0, 3.0),
                    blur_radius: 14.0,
                },
            },
            theme.palette().background,
        );

        // Subtle gutter divider; the shared background keeps the editor visually light.
        renderer.fill_quad(
            renderer::Quad {
                bounds: Rectangle {
                    x: bounds.x + GUTTER_WIDTH,
                    y: bounds.y,
                    width: 1.0,
                    height: bounds.height,
                },
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            },
            Color::from_rgb8(0x2B, 0x2B, 0x2B),
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
        let text_area_width = bounds.width - GUTTER_WIDTH - TEXT_PADDING;
        let max_chars = if self.word_wrap && text_area_width > metrics.char_width {
            (text_area_width / metrics.char_width).floor() as usize
        } else {
            0 // 0 = no wrapping
        };

        // Build visual line map: skip 'scroll' visual lines, then render 'visible_lines' visual lines.
        // Each entry: (logical_line, char_start, char_end, is_first_visual_line)
        let mut visual_rows: Vec<(usize, usize, usize, bool)> = Vec::new();
        let mut vis_skip = 0usize;
        let mut line_idx = 0usize;

        while visual_rows.len() < visible_lines && line_idx < buf.line_count() {
            let text = line_text_for(&buf, line_idx);
            let breaks = word_wrap_breaks(&text, max_chars);
            let line_len = text.chars().count();

            for (w, &start) in breaks.iter().enumerate() {
                let end = if w + 1 < breaks.len() {
                    breaks[w + 1]
                } else {
                    line_len
                };
                if vis_skip < scroll {
                    vis_skip += 1;
                } else if visual_rows.len() < visible_lines {
                    visual_rows.push((line_idx, start, end, w == 0));
                }
            }
            line_idx += 1;
        }

        let text_x = bounds.x + GUTTER_WIDTH + TEXT_PADDING;

        // Render visual rows
        for (vis_idx, &(line_idx, char_start, char_end, is_first)) in visual_rows.iter().enumerate()
        {
            let y = bounds.y + vis_idx as f32 * metrics.line_height;
            if y + metrics.line_height < bounds.y || y > bounds.y + bounds.height {
                continue;
            }

            // Draw selection highlight for this visual line
            if let Some(sel) = selection
                && !sel.is_empty()
            {
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

                    // Clip selection to current visual line range
                    let s = sel_start_col.max(char_start);
                    let e = sel_end_col.min(char_end);
                    if s < e {
                        let sel_x = text_x + (s - char_start) as f32 * metrics.char_width;
                        let sel_w = (e - s) as f32 * metrics.char_width;
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

            // Line number (only on first visual line of each logical line)
            if is_first {
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
            }

            // Line content with syntax highlighting
            if line_idx < highlighted.len() {
                let spans = &highlighted[line_idx];
                // Flatten spans into characters with colors for correct wrapping
                let mut chars_with_color: Vec<(char, Color)> = Vec::new();
                for (style, span_text) in spans {
                    let color = syntect_to_iced(style.foreground);
                    for ch in span_text.chars() {
                        if ch == '\n' {
                            continue;
                        }
                        chars_with_color.push((ch, color));
                    }
                }

                // Extract the slice for this visual line
                let end = char_end.min(chars_with_color.len());
                let slice = if char_start < chars_with_color.len() {
                    &chars_with_color[char_start..end]
                } else {
                    &[]
                };

                // Group consecutive chars with same color into spans
                let mut x_off = text_x;
                let mut span_start = 0;
                while span_start < slice.len() {
                    let color = slice[span_start].1;
                    let mut span_end = span_start + 1;
                    while span_end < slice.len() && slice[span_end].1 == color {
                        span_end += 1;
                    }
                    let display_text: String = slice[span_start..span_end]
                        .iter()
                        .map(|(c, _)| *c)
                        .collect();
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
                    span_start = span_end;
                }
            } else if let Some(line) = buf.line(line_idx) {
                // Fallback: plain text rendering
                let line_text: String = line.chars().filter(|c| *c != '\n').collect();
                let end = char_end.min(line_text.len());
                let display_text = if char_start < line_text.len() {
                    &line_text[char_start..end]
                } else {
                    ""
                };
                if !display_text.is_empty() {
                    renderer.fill_text(
                        text::Text {
                            content: display_text.to_string(),
                            bounds: Size::new(text_area_width, metrics.line_height),
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
            }

            // Draw cursor on this visual line if it contains the cursor
            if line_idx == cursor_line && state.is_focused {
                if cursor_col >= char_start && cursor_col < char_end {
                    let cursor_x = text_x + (cursor_col - char_start) as f32 * metrics.char_width;
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
                // Also draw cursor at end of last visual line segment
                if cursor_col == char_end
                    && char_end == line_text_for(&buf, line_idx).chars().count()
                {
                    let cursor_x = text_x + (char_end - char_start) as f32 * metrics.char_width;
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

        // ── Scrollbar ──
        let total_vis = total_visual_lines(&buf, max_chars);
        let viewport_lines = (bounds.height / metrics.line_height) as usize;
        if total_vis > viewport_lines {
            let track_height = bounds.height;
            let thumb_ratio = viewport_lines as f32 / total_vis as f32;
            let thumb_height = (track_height * thumb_ratio).max(MIN_THUMB_HEIGHT);
            let max_scroll = total_vis.saturating_sub(viewport_lines);
            let scroll_ratio = if max_scroll > 0 {
                scroll as f32 / max_scroll as f32
            } else {
                0.0
            };
            let thumb_y = bounds.y + scroll_ratio * (track_height - thumb_height);
            let track_x = bounds.x + bounds.width - SCROLLBAR_WIDTH;

            // Track
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: track_x,
                        y: bounds.y,
                        width: SCROLLBAR_WIDTH,
                        height: track_height,
                    },
                    border: iced::Border::default(),
                    shadow: iced::Shadow::default(),
                },
                SCROLLBAR_TRACK,
            );
            // Thumb
            let is_hover = state.scrollbar_drag.is_some();
            renderer.fill_quad(
                renderer::Quad {
                    bounds: Rectangle {
                        x: track_x + 1.0,
                        y: thumb_y,
                        width: SCROLLBAR_WIDTH - 2.0,
                        height: thumb_height,
                    },
                    border: iced::Border {
                        radius: (4.0).into(),
                        ..iced::Border::default()
                    },
                    shadow: iced::Shadow::default(),
                },
                if is_hover {
                    SCROLLBAR_THUMB_HOVER
                } else {
                    SCROLLBAR_THUMB
                },
            );
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
        let text_area_width = bounds.width - GUTTER_WIDTH - TEXT_PADDING;
        let cpl = if self.word_wrap && text_area_width > metrics.char_width {
            (text_area_width / metrics.char_width).floor() as usize
        } else {
            0
        };

        // ── Scrollbar interaction ──
        let track_x = bounds.x + bounds.width - SCROLLBAR_WIDTH;
        let buf_ref = self.buffer.borrow();
        let total_vis = total_visual_lines(&buf_ref, cpl);
        let viewport_lines = (bounds.height / metrics.line_height) as usize;
        drop(buf_ref);
        let has_scrollbar = total_vis > viewport_lines;

        if has_scrollbar {
            let track_height = bounds.height;
            let thumb_ratio = viewport_lines as f32 / total_vis as f32;
            let thumb_height = (track_height * thumb_ratio).max(MIN_THUMB_HEIGHT);
            let max_scroll = total_vis.saturating_sub(viewport_lines);

            // Handle scrollbar drag in progress
            if let Some(grab_offset) = state.scrollbar_drag {
                match event {
                    Event::Mouse(mouse::Event::CursorMoved { .. }) => {
                        if let Some(pos) = cursor.position() {
                            let local_y = pos.y - bounds.y - grab_offset;
                            let scroll_range = track_height - thumb_height;
                            let ratio = if scroll_range > 0.0 {
                                (local_y / scroll_range).clamp(0.0, 1.0)
                            } else {
                                0.0
                            };
                            let new_scroll = (ratio * max_scroll as f32).round() as usize;
                            self.buffer.borrow_mut().set_scroll(new_scroll, max_scroll);
                        }
                        return Status::Captured;
                    }
                    Event::Mouse(mouse::Event::ButtonReleased(mouse::Button::Left)) => {
                        state.scrollbar_drag = None;
                        return Status::Captured;
                    }
                    _ => {}
                }
            }

            // Start scrollbar drag
            if let Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) = event
                && let Some(pos) = cursor.position()
                && pos.x >= track_x
                && pos.x <= bounds.x + bounds.width
                && pos.y >= bounds.y
                && pos.y <= bounds.y + bounds.height
            {
                let current_scroll = self.buffer.borrow().scroll_offset();
                let scroll_ratio = if max_scroll > 0 {
                    current_scroll as f32 / max_scroll as f32
                } else {
                    0.0
                };
                let thumb_y = bounds.y + scroll_ratio * (track_height - thumb_height);

                if pos.y >= thumb_y && pos.y <= thumb_y + thumb_height {
                    // Clicked on thumb — start dragging
                    state.scrollbar_drag = Some(pos.y - thumb_y);
                } else {
                    // Clicked on track — jump to position
                    let ratio = ((pos.y - bounds.y) / track_height).clamp(0.0, 1.0);
                    let new_scroll = (ratio * max_scroll as f32).round() as usize;
                    self.buffer.borrow_mut().set_scroll(new_scroll, max_scroll);
                    state.scrollbar_drag = Some(thumb_height / 2.0);
                }
                state.is_focused = true;
                return Status::Captured;
            }
        }

        match event {
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left)) => {
                if cursor.is_over(bounds) {
                    state.is_focused = true;
                    state.is_dragging = true;

                    if let Some(pos) = cursor.position_in(bounds) {
                        let mut buf = self.buffer.borrow_mut();
                        let vis_row = (pos.y / metrics.line_height) as usize;
                        let raw_col = ((pos.x - GUTTER_WIDTH - TEXT_PADDING).max(0.0)
                            / metrics.char_width) as usize;
                        let (line, char_off) =
                            visual_to_logical(&buf, buf.scroll_offset(), vis_row, cpl)
                                .unwrap_or((buf.line_count().saturating_sub(1), 0));
                        let col = char_off + raw_col;

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
                        let vis_row = (pos.y / metrics.line_height) as usize;
                        let raw_col = ((pos.x - GUTTER_WIDTH - TEXT_PADDING).max(0.0)
                            / metrics.char_width) as usize;
                        let (line, char_off) =
                            visual_to_logical(&buf, buf.scroll_offset(), vis_row, cpl)
                                .unwrap_or((buf.line_count().saturating_sub(1), 0));
                        let col = char_off + raw_col;
                        // Extend selection by simulating shift+movement
                        let clamped_line = line.min(buf.line_count().saturating_sub(1));
                        let clamped_col = if clamped_line < buf.line_count() {
                            col.min(
                                buf.line(clamped_line)
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
                        buf.set_selection(anchor_line, anchor_col, clamped_line, clamped_col);
                        buf.set_cursor(clamped_line, clamped_col);
                    }
                    return Status::Captured;
                }
            }
            Event::Mouse(mouse::Event::WheelScrolled { delta }) if cursor.is_over(bounds) => {
                let lines = match delta {
                    mouse::ScrollDelta::Lines { y, .. } => -(y * 3.0) as isize,
                    mouse::ScrollDelta::Pixels { y, .. } => -(y / metrics.line_height) as isize,
                };
                let mut buf = self.buffer.borrow_mut();
                let vis = (bounds.height / metrics.line_height) as usize;
                let total = total_visual_lines(&buf, cpl);
                let max_scroll = total.saturating_sub(vis);
                buf.scroll_by_clamped(lines, max_scroll);
                return Status::Captured;
            }
            Event::Keyboard(keyboard::Event::KeyPressed {
                key,
                modifiers,
                text,
                ..
            }) if state.is_focused => {
                let vis = (bounds.height / metrics.line_height) as usize;
                let is_cmd = modifiers.command();
                let is_shift = modifiers.shift();
                let is_alt = modifiers.alt();
                let committed_text = committed_text_input(text.as_deref(), is_cmd);

                // Helper: ensure cursor visible accounting for word wrap.
                // Must be called while buf is still borrowed mutably.
                macro_rules! ensure_vis {
                    ($buf:expr) => {{
                        let (cl, cc) = $buf.cursor();
                        let vl = logical_to_visual(&$buf, cl, cc, cpl);
                        let total = total_visual_lines(&$buf, cpl);
                        let max_s = total.saturating_sub(vis);
                        $buf.ensure_visual_line_visible(vl, vis, max_s);
                    }};
                }

                match key {
                    // Cmd+Z = undo, Cmd+Shift+Z = redo
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "z" => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            if is_shift {
                                buf.redo();
                            } else {
                                buf.undo();
                            }
                            ensure_vis!(buf);
                        }
                        self.emit_change(shell);
                    }
                    // Cmd+Y = redo (Windows convention)
                    keyboard::Key::Character(ref c) if is_cmd && c.as_str() == "y" => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            buf.redo();
                            ensure_vis!(buf);
                        }
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
                            let mut buf = self.buffer.borrow_mut();
                            buf.insert(&text);
                            ensure_vis!(buf);
                            drop(buf);
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
                        ensure_vis!(buf);
                    }
                    keyboard::Key::Named(keyboard::key::Named::ArrowDown) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_down();
                        } else {
                            buf.move_down();
                        }
                        ensure_vis!(buf);
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
                        ensure_vis!(buf);
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
                        ensure_vis!(buf);
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
                        ensure_vis!(buf);
                    }
                    keyboard::Key::Named(keyboard::key::Named::PageDown) => {
                        let mut buf = self.buffer.borrow_mut();
                        if is_shift {
                            buf.select_page_down(vis);
                        } else {
                            buf.move_page_down(vis);
                        }
                        ensure_vis!(buf);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Backspace) => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            if is_alt {
                                buf.select_word_left();
                                buf.delete_selection();
                            } else {
                                buf.backspace();
                            }
                            ensure_vis!(buf);
                        }
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Delete) => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            buf.delete_forward();
                            ensure_vis!(buf);
                        }
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Enter) => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            buf.insert("\n");
                            ensure_vis!(buf);
                        }
                        self.emit_change(shell);
                    }
                    keyboard::Key::Named(keyboard::key::Named::Tab) => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            buf.insert("    ");
                            ensure_vis!(buf);
                        }
                        self.emit_change(shell);
                    }
                    _ if committed_text.is_some() => {
                        {
                            let mut buf = self.buffer.borrow_mut();
                            buf.insert(committed_text.expect("committed text already checked"));
                            ensure_vis!(buf);
                        }
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

#[cfg(test)]
mod tests {
    use super::{committed_text_input, rgb8, syntect_to_iced};
    use syntect::highlighting::Color as SyntectColor;

    #[test]
    fn committed_text_input_accepts_regular_and_ime_text() {
        assert_eq!(committed_text_input(Some("a"), false), Some("a"));
        assert_eq!(committed_text_input(Some("é"), false), Some("é"));
        assert_eq!(committed_text_input(Some("にほん"), false), Some("にほん"));
    }

    #[test]
    fn committed_text_input_ignores_shortcuts_and_control_only_text() {
        assert_eq!(committed_text_input(Some("s"), true), None);
        assert_eq!(committed_text_input(Some("\n"), false), None);
        assert_eq!(committed_text_input(Some("\u{7f}"), false), None);
        assert_eq!(committed_text_input(None, false), None);
    }

    #[test]
    fn syntax_palette_uses_the_bds2_editor_colors() {
        assert_eq!(
            syntect_to_iced(SyntectColor {
                r: 101,
                g: 115,
                b: 126,
                a: 255,
            }),
            rgb8(0x6A, 0x99, 0x55)
        );
        assert_eq!(
            syntect_to_iced(SyntectColor {
                r: 180,
                g: 142,
                b: 173,
                a: 255,
            }),
            rgb8(0xC5, 0x86, 0xC0)
        );
        assert_eq!(
            syntect_to_iced(SyntectColor {
                r: 163,
                g: 190,
                b: 140,
                a: 255,
            }),
            rgb8(0xCE, 0x91, 0x78)
        );
    }
}
