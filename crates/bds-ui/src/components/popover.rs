use iced::advanced::layout;
use iced::advanced::overlay;
use iced::advanced::renderer;
use iced::advanced::widget::{Operation, Tree};
use iced::advanced::{Clipboard, Layout, Shell, Widget};
use iced::event;
use iced::keyboard::{self, Key, key};
use iced::mouse;
use iced::{Element, Event, Length, Point, Rectangle, Size, Vector};

/// An interactive popup anchored below the right edge of its trigger.
pub struct Popover<'a, Message, Theme = iced::Theme, Renderer = iced::Renderer> {
    trigger: Element<'a, Message, Theme, Renderer>,
    popup: Element<'a, Message, Theme, Renderer>,
    is_open: bool,
    on_dismiss: Message,
    gap: f32,
}

impl<'a, Message, Theme, Renderer> Popover<'a, Message, Theme, Renderer> {
    pub fn new(
        trigger: impl Into<Element<'a, Message, Theme, Renderer>>,
        popup: impl Into<Element<'a, Message, Theme, Renderer>>,
        is_open: bool,
        on_dismiss: Message,
    ) -> Self {
        Self {
            trigger: trigger.into(),
            popup: popup.into(),
            is_open,
            on_dismiss,
            gap: 8.0,
        }
    }
}

fn popup_position(anchor: Rectangle, popup: Size, viewport: Size, gap: f32) -> Point {
    let max_x = (viewport.width - popup.width).max(0.0);
    let x = (anchor.x + anchor.width - popup.width).clamp(0.0, max_x);
    let below = anchor.y + anchor.height + gap;
    let y = if below + popup.height <= viewport.height {
        below
    } else {
        (anchor.y - gap - popup.height).max(0.0)
    };
    Point::new(x, y)
}

impl<'a, Message, Theme, Renderer> Widget<Message, Theme, Renderer>
    for Popover<'a, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn children(&self) -> Vec<Tree> {
        vec![Tree::new(&self.trigger), Tree::new(&self.popup)]
    }

    fn diff(&self, tree: &mut Tree) {
        tree.diff_children(&[self.trigger.as_widget(), self.popup.as_widget()]);
    }

    fn size(&self) -> Size<Length> {
        self.trigger.as_widget().size()
    }

    fn layout(
        &self,
        tree: &mut Tree,
        renderer: &Renderer,
        limits: &layout::Limits,
    ) -> layout::Node {
        self.trigger
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
        self.trigger
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
        self.trigger.as_widget_mut().on_event(
            &mut tree.children[0],
            event,
            layout,
            cursor,
            renderer,
            clipboard,
            shell,
            viewport,
        )
    }

    fn mouse_interaction(
        &self,
        tree: &Tree,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.trigger.as_widget().mouse_interaction(
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
        self.trigger.as_widget().draw(
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
        let (trigger_tree, popup_tree) = tree.children.split_at_mut(1);
        let trigger_overlay = self.trigger.as_widget_mut().overlay(
            &mut trigger_tree[0],
            layout,
            renderer,
            translation,
        );
        let popup_overlay = self.is_open.then(|| {
            overlay::Element::new(Box::new(PopupOverlay {
                anchor: layout.bounds() + translation,
                popup: &mut self.popup,
                state: &mut popup_tree[0],
                on_dismiss: self.on_dismiss.clone(),
                gap: self.gap,
            }))
        });

        if trigger_overlay.is_some() || popup_overlay.is_some() {
            Some(
                overlay::Group::with_children(
                    trigger_overlay.into_iter().chain(popup_overlay).collect(),
                )
                .overlay(),
            )
        } else {
            None
        }
    }
}

impl<'a, Message, Theme, Renderer> From<Popover<'a, Message, Theme, Renderer>>
    for Element<'a, Message, Theme, Renderer>
where
    Message: Clone + 'a,
    Theme: 'a,
    Renderer: renderer::Renderer + 'a,
{
    fn from(popover: Popover<'a, Message, Theme, Renderer>) -> Self {
        Element::new(popover)
    }
}

struct PopupOverlay<'a, 'b, Message, Theme, Renderer> {
    anchor: Rectangle,
    popup: &'b mut Element<'a, Message, Theme, Renderer>,
    state: &'b mut Tree,
    on_dismiss: Message,
    gap: f32,
}

impl<Message, Theme, Renderer> overlay::Overlay<Message, Theme, Renderer>
    for PopupOverlay<'_, '_, Message, Theme, Renderer>
where
    Message: Clone,
    Renderer: renderer::Renderer,
{
    fn layout(&mut self, renderer: &Renderer, bounds: Size) -> layout::Node {
        let popup = self.popup.as_widget().layout(
            self.state,
            renderer,
            &layout::Limits::new(Size::ZERO, bounds),
        );
        let position = popup_position(self.anchor, popup.size(), bounds, self.gap);
        layout::Node::with_children(popup.size(), vec![popup])
            .translate(Vector::new(position.x, position.y))
    }

    fn draw(
        &self,
        renderer: &mut Renderer,
        theme: &Theme,
        style: &renderer::Style,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
    ) {
        self.popup.as_widget().draw(
            self.state,
            renderer,
            theme,
            style,
            layout.children().next().expect("popover child layout"),
            cursor,
            &Rectangle::with_size(Size::INFINITY),
        );
    }

    fn operate(&mut self, layout: Layout<'_>, renderer: &Renderer, operation: &mut dyn Operation) {
        self.popup.as_widget().operate(
            self.state,
            layout.children().next().expect("popover child layout"),
            renderer,
            operation,
        );
    }

    fn on_event(
        &mut self,
        event: Event,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        renderer: &Renderer,
        clipboard: &mut dyn Clipboard,
        shell: &mut Shell<'_, Message>,
    ) -> event::Status {
        let popup_layout = layout.children().next().expect("popover child layout");
        let status = self.popup.as_widget_mut().on_event(
            self.state,
            event.clone(),
            popup_layout,
            cursor,
            renderer,
            clipboard,
            shell,
            &Rectangle::with_size(Size::INFINITY),
        );
        if status == event::Status::Captured {
            return status;
        }

        let dismiss = matches!(
            event,
            Event::Mouse(mouse::Event::ButtonPressed(mouse::Button::Left))
                | Event::Touch(iced::touch::Event::FingerPressed { .. })
                | Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Escape),
                    ..
                })
        );
        if dismiss && !cursor.is_over(layout.bounds()) {
            shell.publish(self.on_dismiss.clone());
            event::Status::Captured
        } else if dismiss
            && matches!(
                event,
                Event::Keyboard(keyboard::Event::KeyPressed {
                    key: Key::Named(key::Named::Escape),
                    ..
                })
            )
        {
            shell.publish(self.on_dismiss.clone());
            event::Status::Captured
        } else {
            status
        }
    }

    fn mouse_interaction(
        &self,
        layout: Layout<'_>,
        cursor: mouse::Cursor,
        viewport: &Rectangle,
        renderer: &Renderer,
    ) -> mouse::Interaction {
        self.popup.as_widget().mouse_interaction(
            self.state,
            layout.children().next().expect("popover child layout"),
            cursor,
            viewport,
            renderer,
        )
    }

    fn overlay<'b>(
        &'b mut self,
        layout: Layout<'_>,
        renderer: &Renderer,
    ) -> Option<overlay::Element<'b, Message, Theme, Renderer>> {
        self.popup.as_widget_mut().overlay(
            self.state,
            layout.children().next().expect("popover child layout"),
            renderer,
            Vector::ZERO,
        )
    }
}

pub fn popover<'a, Message: Clone + 'a>(
    trigger: impl Into<Element<'a, Message>>,
    popup: impl Into<Element<'a, Message>>,
    is_open: bool,
    on_dismiss: Message,
) -> Popover<'a, Message> {
    Popover::new(trigger, popup, is_open, on_dismiss)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn popup_is_right_aligned_below_its_trigger() {
        assert_eq!(
            popup_position(
                Rectangle::new(Point::new(400.0, 20.0), Size::new(100.0, 30.0)),
                Size::new(220.0, 180.0),
                Size::new(800.0, 600.0),
                8.0,
            ),
            Point::new(280.0, 58.0)
        );
    }

    #[test]
    fn popup_flips_above_and_stays_inside_the_viewport() {
        assert_eq!(
            popup_position(
                Rectangle::new(Point::new(10.0, 570.0), Size::new(40.0, 24.0)),
                Size::new(220.0, 180.0),
                Size::new(800.0, 600.0),
                8.0,
            ),
            Point::new(0.0, 382.0)
        );
    }
}
