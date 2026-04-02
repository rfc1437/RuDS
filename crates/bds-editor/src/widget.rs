use iced::advanced::layout::{self, Layout};
use iced::advanced::renderer;
use iced::advanced::widget::{self, Widget};
use iced::advanced::{Clipboard, Shell};
use iced::event::Status;
use iced::mouse;
use iced::{Color, Element, Event, Length, Rectangle, Size, Theme};

use crate::buffer::EditorBuffer;
use crate::highlight::Highlighter;

/// Messages emitted by the CodeEditor widget.
#[derive(Debug, Clone)]
pub enum EditorMessage {
    ContentChanged(String),
}

/// A syntax-highlighting code editor widget for Iced.
///
/// M0 proof of concept: renders highlighted text with line numbers.
/// Full editing (cursor, input, selection) follows in M3.
pub struct CodeEditor<'a> {
    buffer: &'a EditorBuffer,
    highlighter: &'a Highlighter,
    extension: &'a str,
    line_height: f32,
    char_width: f32,
    gutter_width: f32,
}

impl<'a> CodeEditor<'a> {
    pub fn new(
        buffer: &'a EditorBuffer,
        highlighter: &'a Highlighter,
        extension: &'a str,
    ) -> Self {
        Self {
            buffer,
            highlighter,
            extension,
            line_height: 20.0,
            char_width: 8.4,
            gutter_width: 50.0,
        }
    }
}

impl<'a, Message, Renderer> Widget<Message, Theme, Renderer> for CodeEditor<'a>
where
    Renderer: renderer::Renderer,
{
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
        _tree: &widget::Tree,
        renderer: &mut Renderer,
        _theme: &Theme,
        _style: &renderer::Style,
        layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _viewport: &Rectangle,
    ) {
        let bounds = layout.bounds();

        // Draw background
        renderer.fill_quad(
            renderer::Quad {
                bounds,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            },
            Color::from_rgb(0.18, 0.20, 0.25),
        );

        // Draw gutter background
        let gutter_bounds = Rectangle {
            width: self.gutter_width,
            ..bounds
        };
        renderer.fill_quad(
            renderer::Quad {
                bounds: gutter_bounds,
                border: iced::Border::default(),
                shadow: iced::Shadow::default(),
            },
            Color::from_rgb(0.15, 0.17, 0.21),
        );

        // Note: Full text rendering with cosmic-text will be added when
        // we integrate cosmic-text's Buffer for font shaping and layout.
        // For M0 PoC, we verify the widget mounts and draws backgrounds.
    }

    fn on_event(
        &mut self,
        _state: &mut widget::Tree,
        _event: Event,
        _layout: Layout<'_>,
        _cursor: mouse::Cursor,
        _renderer: &Renderer,
        _clipboard: &mut dyn Clipboard,
        _shell: &mut Shell<'_, Message>,
        _viewport: &Rectangle,
    ) -> Status {
        Status::Ignored
    }
}

impl<'a, Message, Renderer> From<CodeEditor<'a>> for Element<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer + 'a,
    Message: 'a,
{
    fn from(editor: CodeEditor<'a>) -> Self {
        Self::new(editor)
    }
}
