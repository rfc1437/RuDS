use std::collections::HashMap;

use iced::Subscription;
use muda::accelerator::{Accelerator, Code, Modifiers, CMD_OR_CTRL};
use muda::{Menu, MenuEvent, MenuItem, MenuId, PredefinedMenuItem, Submenu};

use bds_core::i18n::{translate, UiLocale};
use crate::app::Message;

/// Every custom menu item that the application handles.
///
/// Predefined OS items (Undo, Redo, Cut, Copy, Paste, SelectAll, etc.)
/// are handled by the platform and do not appear here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MenuAction {
    // File
    NewPost,
    ImportMedia,
    Save,
    OpenInBrowser,
    OpenDataFolder,
    // Edit (custom items only)
    Find,
    Replace,
    EditPreferences,
    // View
    ViewPosts,
    ViewMedia,
    ToggleSidebar,
    TogglePanel,
    // Blog
    PublishSelected,
    PreviewPost,
    EditMenu,
    RebuildDatabase,
    ReindexText,
    MetadataDiff,
    RegenerateCalendar,
    ValidateTranslations,
    FillMissingTranslations,
    GenerateSitemap,
    ValidateSite,
    UploadSite,
    // Help
    About,
    OpenDocumentation,
    ViewOnGitHub,
    ReportIssue,
}

impl MenuAction {
    /// All variants in declaration order.
    pub const ALL: &'static [MenuAction] = &[
        MenuAction::NewPost,
        MenuAction::ImportMedia,
        MenuAction::Save,
        MenuAction::OpenInBrowser,
        MenuAction::OpenDataFolder,
        MenuAction::Find,
        MenuAction::Replace,
        MenuAction::EditPreferences,
        MenuAction::ViewPosts,
        MenuAction::ViewMedia,
        MenuAction::ToggleSidebar,
        MenuAction::TogglePanel,
        MenuAction::PublishSelected,
        MenuAction::PreviewPost,
        MenuAction::EditMenu,
        MenuAction::RebuildDatabase,
        MenuAction::ReindexText,
        MenuAction::MetadataDiff,
        MenuAction::RegenerateCalendar,
        MenuAction::ValidateTranslations,
        MenuAction::FillMissingTranslations,
        MenuAction::GenerateSitemap,
        MenuAction::ValidateSite,
        MenuAction::UploadSite,
        MenuAction::About,
        MenuAction::OpenDocumentation,
        MenuAction::ViewOnGitHub,
        MenuAction::ReportIssue,
    ];

    /// Return the i18n key for this action's menu label.
    pub fn i18n_key(self) -> &'static str {
        match self {
            Self::NewPost => "menu.item.newPost",
            Self::ImportMedia => "menu.item.importMedia",
            Self::Save => "menu.item.save",
            Self::OpenInBrowser => "menu.item.openInBrowser",
            Self::OpenDataFolder => "menu.item.openDataFolder",
            Self::Find => "menu.item.find",
            Self::Replace => "menu.item.replace",
            Self::EditPreferences => "menu.item.editPreferences",
            Self::ViewPosts => "menu.item.viewPosts",
            Self::ViewMedia => "menu.item.viewMedia",
            Self::ToggleSidebar => "menu.item.toggleSidebar",
            Self::TogglePanel => "menu.item.togglePanel",
            Self::PublishSelected => "menu.item.publishSelected",
            Self::PreviewPost => "menu.item.previewPost",
            Self::EditMenu => "menu.item.editMenu",
            Self::RebuildDatabase => "menu.item.rebuildDatabase",
            Self::ReindexText => "menu.item.reindexText",
            Self::MetadataDiff => "menu.item.metadataDiff",
            Self::RegenerateCalendar => "menu.item.regenerateCalendar",
            Self::ValidateTranslations => "menu.item.validateTranslations",
            Self::FillMissingTranslations => "menu.item.fillMissingTranslations",
            Self::GenerateSitemap => "menu.item.generateSitemap",
            Self::ValidateSite => "menu.item.validateSite",
            Self::UploadSite => "menu.item.uploadSite",
            Self::About => "menu.item.about",
            Self::OpenDocumentation => "menu.item.openDocumentation",
            Self::ViewOnGitHub => "menu.item.viewOnGitHub",
            Self::ReportIssue => "menu.item.reportIssue",
        }
    }
}

/// Maps between muda `MenuId`s and application `MenuAction`s.
///
/// Also holds clones of the `MenuItem` handles so that labels and
/// enabled state can be changed at runtime (e.g. on locale switch).
pub struct MenuRegistry {
    action_map: HashMap<MenuId, MenuAction>,
    id_map: HashMap<MenuAction, MenuId>,
    items: HashMap<MenuAction, MenuItem>,
}

impl MenuRegistry {
    fn new() -> Self {
        Self {
            action_map: HashMap::new(),
            id_map: HashMap::new(),
            items: HashMap::new(),
        }
    }

    fn register(&mut self, action: MenuAction, item: &MenuItem) {
        self.action_map.insert(item.id().clone(), action);
        self.id_map.insert(action, item.id().clone());
        self.items.insert(action, item.clone());
    }

    /// Look up the `MenuAction` for a raw muda event id.
    pub fn lookup(&self, id: &MenuId) -> Option<MenuAction> {
        self.action_map.get(id).copied()
    }

    /// Enable or disable the menu item for a given action.
    pub fn set_enabled(&self, action: MenuAction, enabled: bool) {
        if let Some(item) = self.items.get(&action) {
            item.set_enabled(enabled);
        }
    }

    /// Change the displayed text for a given action.
    pub fn set_text(&self, action: MenuAction, text: &str) {
        if let Some(item) = self.items.get(&action) {
            item.set_text(text);
        }
    }

    /// Number of registered (action, MenuId) pairs.
    pub fn action_count(&self) -> usize {
        self.action_map.len()
    }
}

// ---------------------------------------------------------------------------
// Menu construction helpers
// ---------------------------------------------------------------------------

/// Helper: create a `MenuItem`, register it, and return a reference for
/// appending to a `Submenu`.
fn item(
    registry: &mut MenuRegistry,
    action: MenuAction,
    locale: UiLocale,
    accel: Option<Accelerator>,
) -> MenuItem {
    let label = translate(locale, action.i18n_key());
    let mi = MenuItem::new(label, true, accel);
    registry.register(action, &mi);
    mi
}

/// Build the full native menu bar and a registry that maps ids to actions.
///
/// On macOS this also calls `init_for_nsapp()` to attach the menu.
pub fn build_menu_bar(locale: UiLocale) -> (Menu, MenuRegistry) {
    let menu = Menu::new();
    let mut reg = MenuRegistry::new();

    // -- macOS app menu --
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

    // -- File --
    let file_menu = Submenu::new(translate(locale, "menu.group.file"), true);
    let _ = file_menu.append(&item(&mut reg, MenuAction::NewPost, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyN))));
    let _ = file_menu.append(&item(&mut reg, MenuAction::ImportMedia, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyI))));
    let _ = file_menu.append(&item(&mut reg, MenuAction::Save, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyS))));
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let _ = file_menu.append(&item(&mut reg, MenuAction::OpenInBrowser, locale, None));
    let _ = file_menu.append(&item(&mut reg, MenuAction::OpenDataFolder, locale, None));
    let _ = file_menu.append(&PredefinedMenuItem::separator());
    let _ = file_menu.append(&PredefinedMenuItem::close_window(None));

    // -- Edit --
    let edit_menu = Submenu::new(translate(locale, "menu.group.edit"), true);
    let _ = edit_menu.append(&PredefinedMenuItem::undo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::redo(None));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&PredefinedMenuItem::cut(None));
    let _ = edit_menu.append(&PredefinedMenuItem::copy(None));
    let _ = edit_menu.append(&PredefinedMenuItem::paste(None));
    let _ = edit_menu.append(&PredefinedMenuItem::select_all(None));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&item(&mut reg, MenuAction::Find, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyF))));
    let _ = edit_menu.append(&item(&mut reg, MenuAction::Replace, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyH))));
    let _ = edit_menu.append(&PredefinedMenuItem::separator());
    let _ = edit_menu.append(&item(&mut reg, MenuAction::EditPreferences, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::Comma))));

    // -- View --
    let view_menu = Submenu::new(translate(locale, "menu.group.view"), true);
    let _ = view_menu.append(&item(&mut reg, MenuAction::ViewPosts, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::Digit1))));
    let _ = view_menu.append(&item(&mut reg, MenuAction::ViewMedia, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::Digit2))));
    let _ = view_menu.append(&PredefinedMenuItem::separator());
    let _ = view_menu.append(&item(&mut reg, MenuAction::ToggleSidebar, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyB))));
    let _ = view_menu.append(&item(&mut reg, MenuAction::TogglePanel, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyJ))));
    let _ = view_menu.append(&PredefinedMenuItem::separator());
    let _ = view_menu.append(&PredefinedMenuItem::fullscreen(None));

    // -- Blog --
    let blog_menu = Submenu::new(translate(locale, "menu.group.blog"), true);
    let _ = blog_menu.append(&item(&mut reg, MenuAction::PublishSelected, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::SHIFT), Code::KeyP))));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::PreviewPost, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::SHIFT), Code::KeyV))));
    let _ = blog_menu.append(&PredefinedMenuItem::separator());
    let _ = blog_menu.append(&item(&mut reg, MenuAction::EditMenu, locale, None));
    let _ = blog_menu.append(&PredefinedMenuItem::separator());
    let _ = blog_menu.append(&item(&mut reg, MenuAction::RebuildDatabase, locale, None));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::ReindexText, locale, None));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::MetadataDiff, locale, None));
    let _ = blog_menu.append(&PredefinedMenuItem::separator());
    let _ = blog_menu.append(&item(&mut reg, MenuAction::RegenerateCalendar, locale, None));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::ValidateTranslations, locale, None));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::FillMissingTranslations, locale, None));
    let _ = blog_menu.append(&PredefinedMenuItem::separator());
    let _ = blog_menu.append(&item(&mut reg, MenuAction::GenerateSitemap, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL), Code::KeyR))));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::ValidateSite, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::SHIFT), Code::KeyL))));
    let _ = blog_menu.append(&item(&mut reg, MenuAction::UploadSite, locale,
        Some(Accelerator::new(Some(CMD_OR_CTRL | Modifiers::SHIFT), Code::KeyU))));

    // -- Help --
    let help_menu = Submenu::new(translate(locale, "menu.group.help"), true);
    let _ = help_menu.append(&item(&mut reg, MenuAction::About, locale, None));
    let _ = help_menu.append(&PredefinedMenuItem::separator());
    let _ = help_menu.append(&item(&mut reg, MenuAction::OpenDocumentation, locale, None));
    let _ = help_menu.append(&item(&mut reg, MenuAction::ViewOnGitHub, locale, None));
    let _ = help_menu.append(&item(&mut reg, MenuAction::ReportIssue, locale, None));

    // Assemble the menu bar
    let _ = menu.append(&app_menu);
    let _ = menu.append(&file_menu);
    let _ = menu.append(&edit_menu);
    let _ = menu.append(&view_menu);
    let _ = menu.append(&blog_menu);
    let _ = menu.append(&help_menu);

    (menu, reg)
}

/// Attach the built menu to the macOS NSApp.
///
/// Must be called **after** the event loop has started (e.g. from the
/// init task or first update), not during `build_menu_bar`.
#[cfg(target_os = "macos")]
pub fn init_menu_for_nsapp(menu: &Menu) {
    let _ = menu.init_for_nsapp();
}

/// Re-translate every registered menu item for a new locale.
pub fn update_menu_labels(registry: &MenuRegistry, locale: UiLocale) {
    for &action in MenuAction::ALL {
        registry.set_text(action, &translate(locale, action.i18n_key()));
    }
}

/// Iced subscription that polls muda `MenuEvent`s each frame.
///
/// Produces `Message::MenuEvent(MenuId)` so the app can look up the
/// `MenuAction` via its `MenuRegistry`.
pub fn menu_subscription() -> Subscription<Message> {
    iced::event::listen_with(|_event, _status, _id| {
        if let Ok(event) = MenuEvent::receiver().try_recv() {
            Some(Message::MenuEvent(event.id))
        } else {
            None
        }
    })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;
    use bds_core::i18n::UiLocale;

    #[test]
    fn all_variants_are_listed() {
        // MenuAction::ALL must contain every variant.
        // If someone adds a variant but forgets ALL, the i18n_key match
        // will produce a compile error, but this also catches ALL length.
        assert_eq!(MenuAction::ALL.len(), 28);
    }

    #[test]
    fn i18n_keys_resolve_for_english() {
        for &action in MenuAction::ALL {
            let key = action.i18n_key();
            let label = translate(UiLocale::En, key);
            assert_ne!(label, key, "missing English translation for {key}");
        }
    }

    #[test]
    fn registry_register_and_lookup_roundtrip() {
        let mut reg = MenuRegistry::new();
        let mi = MenuItem::new("Test", true, None);
        reg.register(MenuAction::Save, &mi);

        assert_eq!(reg.lookup(mi.id()), Some(MenuAction::Save));
        assert_eq!(reg.action_count(), 1);
    }

    #[test]
    fn registry_lookup_missing_returns_none() {
        let reg = MenuRegistry::new();
        let bogus = MenuId::new("nonexistent");
        assert_eq!(reg.lookup(&bogus), None);
    }

    #[test]
    fn registry_set_enabled_and_text() {
        let mut reg = MenuRegistry::new();
        let mi = MenuItem::new("Original", true, None);
        reg.register(MenuAction::NewPost, &mi);

        reg.set_enabled(MenuAction::NewPost, false);
        assert!(!mi.is_enabled());

        reg.set_text(MenuAction::NewPost, "Changed");
        assert_eq!(mi.text(), "Changed");
    }

    #[test]
    fn all_actions_have_unique_i18n_keys() {
        let mut seen = std::collections::HashSet::new();
        for &action in MenuAction::ALL {
            let key = action.i18n_key();
            assert!(seen.insert(key), "duplicate i18n key: {key}");
        }
    }
}
