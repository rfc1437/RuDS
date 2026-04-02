use iced::widget::{column, container, text};
use iced::{Element, Length, Subscription, Task};

use crate::platform::menu;

#[derive(Debug, Clone)]
pub enum Message {
    MenuEvent(muda::MenuId),
    Noop,
}

pub struct BdsApp {
    _menu_bar: muda::Menu,
}

impl BdsApp {
    pub fn new() -> (Self, Task<Message>) {
        let menu_bar = menu::build_menu_bar();
        (Self { _menu_bar: menu_bar }, Task::none())
    }

    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::MenuEvent(_id) => {
                // Menu routing will be expanded in M2
                Task::none()
            }
            Message::Noop => Task::none(),
        }
    }

    pub fn view(&self) -> Element<'_, Message> {
        let content = column![text("bDS — Blogging Desktop Server").size(24),]
            .padding(20)
            .spacing(10);

        container(content)
            .width(Length::Fill)
            .height(Length::Fill)
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
    }

    pub fn subscription(&self) -> Subscription<Message> {
        menu::menu_subscription()
    }
}
