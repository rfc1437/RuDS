use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use iced::advanced::layout;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree, tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::event;
use iced::keyboard::{self, Key, Location, Modifiers, key};
use iced::mouse;
use iced::{Element, Event, Length, Rectangle, Size, Vector};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EditCommand {
    Undo,
    Redo,
    Cut,
    Copy,
    Paste,
    SelectAll,
}

pub type EditCommandQueue = Arc<Mutex<VecDeque<EditCommand>>>;

pub fn command_queue() -> EditCommandQueue {
    Arc::new(Mutex::new(VecDeque::new()))
}

pub fn queue_command(queue: &EditCommandQueue, command: EditCommand) {
    queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .push_back(command);
}

fn pop_command(queue: &EditCommandQueue) -> Option<EditCommand> {
    queue
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .pop_front()
}

/// Transparent bridge between native menu actions and Iced's focused widget.
///
/// Muda's predefined macOS edit items send Cocoa responder selectors. Iced's
/// canvas is not a native text responder, so those selectors swallow keyboard
/// shortcuts without reaching the focused Iced text widget. Custom menu items
/// enqueue commands here; the bridge replays the equivalent keyboard sequence
/// through the widget tree on the redraw requested after the menu event.
pub struct NativeEdit<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    content: Element<'a, Message, Theme, Renderer>,
    commands: EditCommandQueue,
}

impl<'a, Message, Theme, Renderer> NativeEdit<'a, Message, Theme, Renderer> {
    pub fn new(
        content: impl Into<Element<'a, Message, Theme, Renderer>>,
        commands: EditCommandQueue,
    ) -> Self {
        Self {
            content: content.into(),
            commands,
        }
    }
}

#[derive(Default)]
struct State;

fn modifier_sync_event(event: &Event) -> Option<Event> {
    let Event::Keyboard(keyboard::Event::KeyPressed { modifiers, .. }) = event else {
        return None;
    };

    Some(Event::Keyboard(keyboard::Event::ModifiersChanged(
        *modifiers,
    )))
}

fn command_events(command: EditCommand) -> [Event; 4] {
    let (character, modified_character, physical_key, modifiers) = match command {
        EditCommand::Undo => ("z", "z", key::Code::KeyZ, Modifiers::COMMAND),
        EditCommand::Redo => (
            "z",
            "Z",
            key::Code::KeyZ,
            Modifiers::COMMAND | Modifiers::SHIFT,
        ),
        EditCommand::Cut => ("x", "x", key::Code::KeyX, Modifiers::COMMAND),
        EditCommand::Copy => ("c", "c", key::Code::KeyC, Modifiers::COMMAND),
        EditCommand::Paste => ("v", "v", key::Code::KeyV, Modifiers::COMMAND),
        EditCommand::SelectAll => ("a", "a", key::Code::KeyA, Modifiers::COMMAND),
    };
    let key = Key::Character(character.into());

    [
        Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)),
        Event::Keyboard(keyboard::Event::KeyPressed {
            key: key.clone(),
            modified_key: Key::Character(modified_character.into()),
            physical_key: key::Physical::Code(physical_key),
            location: Location::Standard,
            modifiers,
            text: None,
        }),
        Event::Keyboard(keyboard::Event::KeyReleased {
            key,
            location: Location::Standard,
            modifiers,
        }),
        Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::default())),
    ]
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for NativeEdit<'a, Message, Theme, Renderer>
where
    Renderer: renderer::Renderer,
{
    fn tag(&self) -> tree::Tag {
        tree::Tag::of::<State>()
    }

    fn state(&self) -> tree::State {
        tree::State::new(State)
    }

    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.content)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(std::slice::from_ref(&self.content));
    }

    fn size(&self) -> Size<Length> {
        self.content.as_widget().size()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.content
            .as_widget()
            .layout(&mut tree.children[0], renderer, limits)
    }

    fn operate(
        &self,
        tree: &mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        operation: &mut dyn Operation,
    ) {
        self.content
            .as_widget()
            .operate(&mut tree.children[0], layout, renderer, operation);
    }

    fn on_event(
        &mut self,
        tree: &mut Tree,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
        viewport: &Rectangle,
    ) -> event::Status {
        let mut status = event::Status::Ignored;

        if matches!(
            event,
            Event::Window(iced::window::Event::RedrawRequested(_))
        ) {
            while let Some(command) = pop_command(&self.commands) {
                for command_event in command_events(command) {
                    if self.content.as_widget_mut().on_event(
                        &mut tree.children[0],
                        command_event,
                        layout,
                        cursor,
                        renderer,
                        clipboard,
                        shell,
                        viewport,
                    ) == event::Status::Captured
                    {
                        status = event::Status::Captured;
                    }
                }
            }
        }

        if let Some(sync_event) = modifier_sync_event(&event) {
            let _ = self.content.as_widget_mut().on_event(
                &mut tree.children[0],
                sync_event,
                layout,
                cursor,
                renderer,
                clipboard,
                shell,
                viewport,
            );
        }

        if self.content.as_widget_mut().on_event(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        ) == event::Status::Captured
        {
            event::Status::Captured
        } else {
            status
        }
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.content.as_widget().mouse_interaction(
            &tree.children[0],
            layout,
            cursor,
            viewport,
            renderer,
        )
    }

    fn draw(
        &self,
        tree: &Tree,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
    ) {
        self.content.as_widget().draw(
            &tree.children[0],
            renderer,
            theme,
            style,
            layout,
            cursor,
            viewport,
        );
    }

    fn overlay<'b>(
        &'b mut self,
        tree: &'b mut Tree,
        layout: Layout<'_>,
        renderer: &Renderer,
        translation: Vector,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.content
            .as_widget_mut()
            .overlay(&mut tree.children[0], layout, renderer, translation)
    }
}

impl<'a, Message, Theme, Renderer> From<NativeEdit<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: 'a,
    Theme: 'a,
    Renderer: 'a + renderer::Renderer,
{
    fn from(bridge: NativeEdit<'a, Message, Theme, Renderer>) -> Self {
        Element::new(bridge)
    }
}

pub fn native_edit<'a, Message, Theme, Renderer>(
    content: impl Into<Element<'a, Message, Theme, Renderer>>,
    commands: EditCommandQueue,
) -> NativeEdit<'a, Message, Theme, Renderer> {
    NativeEdit::new(content, commands)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_press_produces_modifier_sync_event() {
        let modifiers = Modifiers::COMMAND | Modifiers::SHIFT;
        let event = Event::Keyboard(keyboard::Event::KeyPressed {
            key: Key::Character("v".into()),
            modified_key: Key::Character("V".into()),
            physical_key: key::Physical::Code(key::Code::KeyV),
            location: Location::Standard,
            modifiers,
            text: Some("v".into()),
        });

        assert_eq!(
            modifier_sync_event(&event),
            Some(Event::Keyboard(keyboard::Event::ModifiersChanged(
                modifiers
            )))
        );
    }

    #[test]
    fn paste_command_replays_complete_shortcut_sequence() {
        let events = command_events(EditCommand::Paste);

        assert!(matches!(
            events[0],
            Event::Keyboard(keyboard::Event::ModifiersChanged(Modifiers::COMMAND))
        ));
        assert!(matches!(
            &events[1],
            Event::Keyboard(keyboard::Event::KeyPressed {
                key: Key::Character(key),
                modifiers: Modifiers::COMMAND,
                ..
            }) if key == "v"
        ));
        assert!(matches!(
            &events[2],
            Event::Keyboard(keyboard::Event::KeyReleased {
                key: Key::Character(key),
                ..
            }) if key == "v"
        ));
        assert!(matches!(
            events[3],
            Event::Keyboard(keyboard::Event::ModifiersChanged(modifiers)) if modifiers.is_empty()
        ));
    }

    #[test]
    fn command_queue_preserves_edit_order() {
        let queue = command_queue();
        queue_command(&queue, EditCommand::Copy);
        queue_command(&queue, EditCommand::Paste);

        assert_eq!(pop_command(&queue), Some(EditCommand::Copy));
        assert_eq!(pop_command(&queue), Some(EditCommand::Paste));
        assert_eq!(pop_command(&queue), None);
    }
}
