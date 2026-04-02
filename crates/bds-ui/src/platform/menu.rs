use iced::Subscription;
use muda::{Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu};

use crate::app::Message;

/// Build the native menu bar with standard application menus.
pub fn build_menu_bar() -> Menu {
    let menu = Menu::new();

    // App menu (macOS only shows this as the app name)
    let app_menu = Submenu::new("bDS", true);
    let _ = app_menu.append(&PredefinedMenuItem::about(None, None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let _ = app_menu.append(&PredefinedMenuItem::services(None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let _ = app_menu.append(&PredefinedMenuItem::hide(None));
    let _ = app_menu.append(&PredefinedMenuItem::hide_others(None));
    let _ = app_menu.append(&PredefinedMenuItem::show_all(None));
    let _ = app_menu.append(&PredefinedMenuItem::separator());
    let _ = app_menu.append(&PredefinedMenuItem::quit(None));

    // File menu
    let file_menu = Submenu::new("File", true);
    let _ = file_menu.append(&MenuItem::new("Open Project...", true, None));
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let _ = file_menu.append(&MenuItem::new("Save", true, None));
    let _ = file_menu.append(&PredefinedMenuItem::close_window(None));

    // Edit menu
    let edit_menu = Submenu::new("Edit", true);
    let _ = edit_menu.append(&PredefinedMenuItem::undo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::redo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&PredefinedMenuItem::cut(None));
    let _ = edit_menu.append(&PredefinedMenuItem::copy(None));
    let _ = edit_menu.append(&PredefinedMenuItem::paste(None));
    let _ = edit_menu.append(&PredefinedMenuItem::select_all(None));

    // View menu
    let view_menu = Submenu::new("View", true);
    let _ = view_menu.append(&MenuItem::new("Toggle Sidebar", true, None));
    let _ = view_menu.append(&PredefinedMenuItem::fullscreen(None));

    // Window menu
    let window_menu = Submenu::new("Window", true);
    let _ = window_menu.append(&PredefinedMenuItem::minimize(None));
    let _ = window_menu.append(&PredefinedMenuItem::maximize(None));

    // Help menu
    let help_menu = Submenu::new("Help", true);
    let _ = help_menu.append(&MenuItem::new("bDS Help", true, None));

    let _ = menu.append(&app_menu);
    let _ = menu.append(&file_menu);
    let _ = menu.append(&edit_menu);
    let _ = menu.append(&view_menu);
    let _ = menu.append(&window_menu);
    let _ = menu.append(&help_menu);

    // Initialize the menu on macOS
    #[cfg(target_os = "macos")]
    {
        let _ = menu.init_for_nsapp();
    }

    menu
}

/// Iced subscription that polls muda menu events.
pub fn menu_subscription() -> Subscription<Message> {
    iced::event::listen_with(|_event, _status, _id| {
        // Check for pending menu events
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            Some(Message::MenuEvent(event.id))
        } else {
            None
        }
    })
}
