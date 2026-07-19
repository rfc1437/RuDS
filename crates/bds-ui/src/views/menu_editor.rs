use std::collections::HashSet;
use std::time::{Duration, Instant};

use bds_core::engine::menu::{MenuItem, MenuItemKind};
use bds_core::i18n::UiLocale;
use iced::widget::{
    Space, button, column, container, mouse_area, row, scrollable, svg, text, text_input, tooltip,
};
use iced::{Alignment, Background, Border, Color, Element, Length, Padding, Point, Theme};
use uuid::Uuid;

use crate::app::Message;
use crate::components::inputs::{
    self, danger_button, field_style, primary_button, secondary_button,
};
use crate::i18n::t;

pub const HOME_ID: &str = "menu-home";
pub const DRAG_EXPAND_DELAY: Duration = Duration::from_millis(450);
const HOME_ICON: &[u8] = include_bytes!("../../assets/icons/menu-home.svg");
const PAGE_ICON: &[u8] = include_bytes!("../../assets/icons/menu-page.svg");
const SUBMENU_ICON: &[u8] = include_bytes!("../../assets/icons/menu-submenu.svg");
const CATEGORY_ICON: &[u8] = include_bytes!("../../assets/icons/menu-category.svg");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DraftKind {
    Page,
    Category,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveDirection {
    Up,
    Down,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropPosition {
    Before,
    Inside,
    After,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEditError {
    MissingDraft,
    MissingChoice,
    CategoryRequired,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PageOption {
    pub id: String,
    pub title: String,
    pub slug: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MenuNode {
    pub id: String,
    pub kind: MenuItemKind,
    pub label: String,
    pub slug: Option<String>,
    pub children: Vec<MenuNode>,
}

impl MenuNode {
    fn from_item(item: MenuItem) -> Self {
        let id = if item.kind == MenuItemKind::Home {
            HOME_ID.to_string()
        } else {
            Uuid::new_v4().to_string()
        };
        Self {
            id,
            kind: item.kind,
            label: item.label,
            slug: item.slug,
            children: item.children.into_iter().map(Self::from_item).collect(),
        }
    }

    fn to_item(&self) -> MenuItem {
        MenuItem {
            kind: self.kind.clone(),
            label: self.label.clone(),
            slug: self.slug.clone(),
            children: self.children.iter().map(Self::to_item).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuDraft {
    pub item_id: String,
    pub kind: DraftKind,
    pub query: String,
    pub validation_failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuEditorStatus {
    NotLoaded,
    Ready,
    Saving,
    LoadFailed,
}

#[derive(Debug, Clone)]
pub struct MenuEditorState {
    pub project_id: Option<String>,
    pub items: Vec<MenuNode>,
    pub selected_id: Option<String>,
    pub draft: Option<MenuDraft>,
    pub pages: Vec<PageOption>,
    pub categories: Vec<String>,
    pub dirty: bool,
    pub status: MenuEditorStatus,
    pub error: Option<String>,
    pub collapsed: HashSet<String>,
    pub dragging_id: Option<String>,
    pub drop_target: Option<(String, DropPosition)>,
    pub hover_expand: Option<(String, Instant)>,
}

pub fn load(
    db: &bds_core::db::Database,
    project_id: &str,
    data_dir: &std::path::Path,
) -> Result<MenuEditorState, String> {
    let items = bds_core::engine::menu::read_menu(data_dir).map_err(|error| error.to_string())?;
    let mut pages = bds_core::db::queries::post::list_posts_by_project(db.conn(), project_id)
        .map_err(|error| error.to_string())?
        .into_iter()
        .filter(|post| {
            post.categories
                .iter()
                .any(|category| category.eq_ignore_ascii_case("page"))
        })
        .map(|post| PageOption {
            id: post.id,
            title: post.title,
            slug: post.slug,
        })
        .collect::<Vec<_>>();
    pages.sort_by(|left, right| {
        left.title
            .to_lowercase()
            .cmp(&right.title.to_lowercase())
            .then_with(|| left.slug.cmp(&right.slug))
    });
    let mut categories = bds_core::engine::meta::read_categories_json(data_dir)
        .map_err(|error| error.to_string())?;
    categories.sort_by_key(|name| name.to_lowercase());
    Ok(MenuEditorState::from_persisted(
        project_id.to_string(),
        items,
        pages,
        categories,
    ))
}

impl Default for MenuEditorState {
    fn default() -> Self {
        Self {
            project_id: None,
            items: Vec::new(),
            selected_id: None,
            draft: None,
            pages: Vec::new(),
            categories: Vec::new(),
            dirty: false,
            status: MenuEditorStatus::NotLoaded,
            error: None,
            collapsed: HashSet::new(),
            dragging_id: None,
            drop_target: None,
            hover_expand: None,
        }
    }
}

impl MenuEditorState {
    pub fn from_persisted(
        project_id: String,
        items: Vec<MenuItem>,
        pages: Vec<PageOption>,
        categories: Vec<String>,
    ) -> Self {
        Self::ready(
            project_id,
            items.into_iter().map(MenuNode::from_item).collect(),
            pages,
            categories,
        )
    }

    pub fn ready(
        project_id: String,
        items: Vec<MenuNode>,
        pages: Vec<PageOption>,
        categories: Vec<String>,
    ) -> Self {
        let selected_id = items.first().map(|item| item.id.clone());
        Self {
            project_id: Some(project_id),
            items,
            selected_id,
            pages,
            categories,
            status: MenuEditorStatus::Ready,
            ..Self::default()
        }
    }

    pub fn persisted_items(&self) -> Vec<MenuItem> {
        self.items.iter().map(MenuNode::to_item).collect()
    }

    pub fn start_draft(&mut self, kind: DraftKind) -> bool {
        if self.draft.is_some() || self.status != MenuEditorStatus::Ready {
            return false;
        }
        let id = Uuid::new_v4().to_string();
        let node = MenuNode {
            id: id.clone(),
            kind: match kind {
                DraftKind::Page => MenuItemKind::Page,
                DraftKind::Category => MenuItemKind::CategoryArchive,
            },
            label: String::new(),
            slug: None,
            children: Vec::new(),
        };
        let (parent, index) = insertion_target(&self.items, self.selected_id.as_deref());
        insert_at(&mut self.items, &parent, index, node);
        if let Some(selected) = self.selected_id.as_deref() {
            self.collapsed.remove(selected);
        }
        self.selected_id = Some(id.clone());
        self.draft = Some(MenuDraft {
            item_id: id,
            kind,
            query: String::new(),
            validation_failed: false,
        });
        true
    }

    pub fn draft_changed(&mut self, query: String) {
        if let Some(draft) = &mut self.draft {
            draft.query = query;
            draft.validation_failed = false;
        }
    }

    pub fn choose_page(&mut self, page_id: &str) -> Result<(), MenuEditError> {
        let page = self
            .pages
            .iter()
            .find(|page| page.id == page_id)
            .cloned()
            .ok_or(MenuEditError::MissingChoice)?;
        let draft = self
            .draft
            .as_ref()
            .filter(|draft| draft.kind == DraftKind::Page)
            .ok_or(MenuEditError::MissingDraft)?;
        let path = find_path(&self.items, &draft.item_id).ok_or(MenuEditError::MissingDraft)?;
        let item = item_at_mut(&mut self.items, &path).ok_or(MenuEditError::MissingDraft)?;
        item.kind = MenuItemKind::Page;
        item.label = page.title;
        item.slug = Some(page.slug);
        self.draft = None;
        self.dirty = true;
        Ok(())
    }

    pub fn submit_submenu(&mut self, default_label: &str) -> Result<(), MenuEditError> {
        let draft = self
            .draft
            .as_ref()
            .filter(|draft| draft.kind == DraftKind::Page)
            .ok_or(MenuEditError::MissingDraft)?;
        let label = if draft.query.trim().is_empty() {
            default_label.to_string()
        } else {
            draft.query.trim().to_string()
        };
        let path = find_path(&self.items, &draft.item_id).ok_or(MenuEditError::MissingDraft)?;
        let item = item_at_mut(&mut self.items, &path).ok_or(MenuEditError::MissingDraft)?;
        item.kind = MenuItemKind::Submenu;
        item.label = label;
        item.slug = None;
        self.draft = None;
        self.dirty = true;
        Ok(())
    }

    /// Resolve an existing or new category and finalize its draft item.
    /// Returns the canonical category name and whether metadata must be created.
    pub fn submit_category(&mut self) -> Result<(String, bool), MenuEditError> {
        let Some(draft) = self
            .draft
            .as_mut()
            .filter(|draft| draft.kind == DraftKind::Category)
        else {
            return Err(MenuEditError::MissingDraft);
        };
        let query = draft.query.trim();
        if query.is_empty() {
            draft.validation_failed = true;
            return Err(MenuEditError::CategoryRequired);
        }
        let existing = self
            .categories
            .iter()
            .find(|name| name.eq_ignore_ascii_case(query))
            .cloned();
        let name = existing.clone().unwrap_or_else(|| query.to_string());
        let is_new = existing.is_none();
        let item_id = draft.item_id.clone();
        let path = find_path(&self.items, &item_id).ok_or(MenuEditError::MissingDraft)?;
        let item = item_at_mut(&mut self.items, &path).ok_or(MenuEditError::MissingDraft)?;
        item.kind = MenuItemKind::CategoryArchive;
        item.label = name.clone();
        item.slug = Some(name.clone());
        if is_new {
            self.categories.push(name.clone());
            self.categories.sort_by_key(|name| name.to_lowercase());
        }
        self.draft = None;
        self.dirty = true;
        Ok((name, is_new))
    }

    pub fn choose_category(&mut self, name: &str) -> Result<(), MenuEditError> {
        if !self.categories.iter().any(|category| category == name) {
            return Err(MenuEditError::MissingChoice);
        }
        self.draft_changed(name.to_string());
        self.submit_category().map(|_| ())
    }

    pub fn cancel_draft(&mut self) -> bool {
        let Some(draft) = self.draft.take() else {
            return false;
        };
        let removed = remove_by_id(&mut self.items, &draft.item_id).is_some();
        self.selected_id = self.items.first().map(|item| item.id.clone());
        removed
    }

    pub fn move_selected(&mut self, direction: MoveDirection) -> bool {
        let Some(id) = self.selected_id.clone() else {
            return false;
        };
        if id == HOME_ID || self.draft.is_some() {
            return false;
        }
        let Some(path) = find_path(&self.items, &id) else {
            return false;
        };
        let Some((&index, parent_path)) = path.split_last() else {
            return false;
        };
        let siblings = children_at_mut(&mut self.items, parent_path);
        let Some(siblings) = siblings else {
            return false;
        };
        let target = match direction {
            MoveDirection::Up => {
                let minimum = usize::from(parent_path.is_empty());
                if index <= minimum {
                    return false;
                }
                index - 1
            }
            MoveDirection::Down => {
                if index + 1 >= siblings.len() {
                    return false;
                }
                index + 1
            }
        };
        siblings.swap(index, target);
        self.dirty = true;
        true
    }

    pub fn indent_selected(&mut self) -> bool {
        let Some(id) = self.selected_id.clone() else {
            return false;
        };
        if id == HOME_ID || self.draft.is_some() {
            return false;
        }
        let Some(path) = find_path(&self.items, &id) else {
            return false;
        };
        let Some((&index, parent_path)) = path.split_last() else {
            return false;
        };
        if index == 0 {
            return false;
        }
        let mut preceding = parent_path.to_vec();
        preceding.push(index - 1);
        if item_at(&self.items, &preceding).is_none_or(|item| item.kind != MenuItemKind::Submenu) {
            return false;
        }
        let Some(item) = remove_at(&mut self.items, &path) else {
            return false;
        };
        let Some(parent) = item_at_mut(&mut self.items, &preceding) else {
            return false;
        };
        parent.children.push(item);
        self.collapsed.remove(&parent.id);
        self.dirty = true;
        true
    }

    pub fn unindent_selected(&mut self) -> bool {
        let Some(id) = self.selected_id.clone() else {
            return false;
        };
        if id == HOME_ID || self.draft.is_some() {
            return false;
        }
        let Some(path) = find_path(&self.items, &id) else {
            return false;
        };
        if path.len() < 2 {
            return false;
        }
        let parent_path = &path[..path.len() - 1];
        let parent_id = item_at(&self.items, parent_path).map(|item| item.id.clone());
        if parent_id.as_deref() == Some(HOME_ID) {
            return false;
        }
        let Some(item) = remove_at(&mut self.items, &path) else {
            return false;
        };
        let grandparent = &parent_path[..parent_path.len() - 1];
        let parent_index = parent_path[parent_path.len() - 1];
        insert_at(&mut self.items, grandparent, parent_index + 1, item);
        self.dirty = true;
        true
    }

    pub fn delete_selected(&mut self) -> bool {
        let Some(id) = self.selected_id.clone() else {
            return false;
        };
        if id == HOME_ID {
            return false;
        }
        let removed = remove_by_id(&mut self.items, &id).is_some();
        if removed {
            self.draft = None;
            self.selected_id = self.items.first().map(|item| item.id.clone());
            self.dirty = true;
        }
        removed
    }

    pub fn drop_item(&mut self, dragged_id: &str, target_id: &str, position: DropPosition) -> bool {
        if dragged_id.is_empty()
            || target_id.is_empty()
            || dragged_id == target_id
            || dragged_id == HOME_ID
            || self.draft.is_some()
        {
            return false;
        }
        let Some(drag_path) = find_path(&self.items, dragged_id) else {
            return false;
        };
        let Some(target_path) = find_path(&self.items, target_id) else {
            return false;
        };
        if target_path.starts_with(&drag_path) {
            return false;
        }
        if position == DropPosition::Inside
            && item_at(&self.items, &target_path)
                .is_none_or(|item| item.kind != MenuItemKind::Submenu)
        {
            return false;
        }
        if position == DropPosition::Before && target_id == HOME_ID {
            return false;
        }
        let Some(item) = remove_at(&mut self.items, &drag_path) else {
            return false;
        };
        let Some(next_target) = find_path(&self.items, target_id) else {
            return false;
        };
        match position {
            DropPosition::Inside => insert_at(&mut self.items, &next_target, 0, item),
            DropPosition::Before | DropPosition::After => {
                let Some((&index, parent)) = next_target.split_last() else {
                    return false;
                };
                let index = index + usize::from(position == DropPosition::After);
                insert_at(&mut self.items, parent, index, item);
            }
        }
        self.selected_id = Some(dragged_id.to_string());
        self.dirty = true;
        true
    }

    pub fn drag_over(&mut self, target: String, position: DropPosition, now: Instant) {
        if self.dragging_id.is_none() {
            return;
        }
        self.drop_target = Some((target.clone(), position));
        let should_expand = position == DropPosition::Inside
            && self.collapsed.contains(&target)
            && find_path(&self.items, &target)
                .and_then(|path| item_at(&self.items, &path))
                .is_some_and(|item| item.kind == MenuItemKind::Submenu);
        if should_expand {
            if self
                .hover_expand
                .as_ref()
                .is_none_or(|(id, _)| id != &target)
            {
                self.hover_expand = Some((target, now));
            }
        } else {
            self.hover_expand = None;
        }
    }

    pub fn expand_hovered(&mut self, now: Instant) -> bool {
        let Some((id, started)) = self.hover_expand.clone() else {
            return false;
        };
        if now.duration_since(started) < DRAG_EXPAND_DELAY {
            return false;
        }
        self.collapsed.remove(&id);
        self.hover_expand = None;
        true
    }
}

#[derive(Debug, Clone)]
pub enum MenuEditorMsg {
    Reload,
    Select(String),
    StartDraft(DraftKind),
    DraftChanged(String),
    ChoosePage(String),
    ChooseCategory(String),
    SubmitDraft,
    CancelDraft,
    Move(MoveDirection),
    Indent,
    Unindent,
    Delete,
    Save,
    ToggleExpanded(String),
    DragStart(String),
    DragOver(String, DropPosition),
    DragLeave(String),
    Drop,
    DragCancel,
    ExpandTick,
}

fn insertion_target(items: &[MenuNode], selected_id: Option<&str>) -> (Vec<usize>, usize) {
    let Some(id) = selected_id else {
        return (Vec::new(), items.len());
    };
    let Some(path) = find_path(items, id) else {
        return (Vec::new(), items.len());
    };
    if item_at(items, &path).is_some_and(|item| item.kind == MenuItemKind::Submenu) {
        return (path, 0);
    }
    let Some((&index, parent)) = path.split_last() else {
        return (Vec::new(), items.len());
    };
    (parent.to_vec(), index + 1)
}

fn find_path(items: &[MenuNode], id: &str) -> Option<Vec<usize>> {
    fn search(items: &[MenuNode], id: &str, path: &mut Vec<usize>) -> bool {
        for (index, item) in items.iter().enumerate() {
            path.push(index);
            if item.id == id || search(&item.children, id, path) {
                return true;
            }
            path.pop();
        }
        false
    }
    let mut path = Vec::new();
    search(items, id, &mut path).then_some(path)
}

fn item_at<'a>(items: &'a [MenuNode], path: &[usize]) -> Option<&'a MenuNode> {
    let (&first, rest) = path.split_first()?;
    let item = items.get(first)?;
    if rest.is_empty() {
        Some(item)
    } else {
        item_at(&item.children, rest)
    }
}

fn item_at_mut<'a>(items: &'a mut [MenuNode], path: &[usize]) -> Option<&'a mut MenuNode> {
    let (&first, rest) = path.split_first()?;
    let item = items.get_mut(first)?;
    if rest.is_empty() {
        Some(item)
    } else {
        item_at_mut(&mut item.children, rest)
    }
}

fn children_at_mut<'a>(
    items: &'a mut Vec<MenuNode>,
    path: &[usize],
) -> Option<&'a mut Vec<MenuNode>> {
    if path.is_empty() {
        return Some(items);
    }
    item_at_mut(items, path).map(|item| &mut item.children)
}

fn insert_at(items: &mut Vec<MenuNode>, parent: &[usize], index: usize, item: MenuNode) {
    if let Some(children) = children_at_mut(items, parent) {
        children.insert(index.min(children.len()), item);
    }
}

fn remove_at(items: &mut Vec<MenuNode>, path: &[usize]) -> Option<MenuNode> {
    let (&index, parent) = path.split_last()?;
    let children = children_at_mut(items, parent)?;
    (index < children.len()).then(|| children.remove(index))
}

fn remove_by_id(items: &mut Vec<MenuNode>, id: &str) -> Option<MenuNode> {
    let path = find_path(items, id)?;
    remove_at(items, &path)
}

fn kind_label(locale: UiLocale, kind: &MenuItemKind) -> String {
    t(
        locale,
        match kind {
            MenuItemKind::Home => "menuEditor.kindHome",
            MenuItemKind::Page => "menuEditor.kindPage",
            MenuItemKind::Submenu => "menuEditor.kindSubmenu",
            MenuItemKind::CategoryArchive => "menuEditor.kindCategory",
        },
    )
}

fn kind_icon(kind: &MenuItemKind) -> iced::widget::Svg<'static> {
    let bytes = match kind {
        MenuItemKind::Home => HOME_ICON,
        MenuItemKind::Page => PAGE_ICON,
        MenuItemKind::Submenu => SUBMENU_ICON,
        MenuItemKind::CategoryArchive => CATEGORY_ICON,
    };
    svg(svg::Handle::from_memory(bytes))
        .width(Length::Fixed(18.0))
        .height(Length::Fixed(18.0))
}

pub fn view(state: &MenuEditorState, locale: UiLocale) -> Element<'_, Message> {
    let title = text(t(locale, "menuEditor.title")).size(24);
    let description = text(t(locale, "menuEditor.description"))
        .size(13)
        .color(Color::from_rgb8(0xA8, 0xA8, 0xA8));
    let header = inputs::card(column![title, description].spacing(6));
    let add_disabled = state.draft.is_some() || state.status != MenuEditorStatus::Ready;
    let selected_is_home = state.selected_id.as_deref() == Some(HOME_ID);
    let actions_disabled = selected_is_home
        || state.selected_id.is_none()
        || state.draft.is_some()
        || state.status != MenuEditorStatus::Ready;
    let toolbar = inputs::toolbar(
        vec![
            toolbar_icon(
                locale,
                "menuEditor.addEntry",
                "+",
                MenuEditorMsg::StartDraft(DraftKind::Page),
                !add_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "menuEditor.addCategory",
                "▣",
                MenuEditorMsg::StartDraft(DraftKind::Category),
                !add_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "menuEditor.moveUp",
                "↑",
                MenuEditorMsg::Move(MoveDirection::Up),
                !actions_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "menuEditor.moveDown",
                "↓",
                MenuEditorMsg::Move(MoveDirection::Down),
                !actions_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "menuEditor.indent",
                "→",
                MenuEditorMsg::Indent,
                !actions_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "menuEditor.unindent",
                "←",
                MenuEditorMsg::Unindent,
                !actions_disabled,
                false,
            ),
            toolbar_icon(
                locale,
                "common.delete",
                "×",
                MenuEditorMsg::Delete,
                !actions_disabled,
                true,
            ),
        ],
        vec![toolbar_icon(
            locale,
            "common.save",
            "◆",
            MenuEditorMsg::Save,
            state.dirty && state.draft.is_none() && state.status == MenuEditorStatus::Ready,
            false,
        )],
    );

    let content: Element<'_, Message> = match state.status {
        MenuEditorStatus::NotLoaded => centered_status(locale, "menuEditor.loading"),
        MenuEditorStatus::LoadFailed => {
            let detail = state.error.clone().unwrap_or_default();
            container(
                column![
                    text(t(locale, "menuEditor.loadFailed")),
                    text(detail).size(12)
                ]
                .spacing(8)
                .align_x(Alignment::Center),
            )
            .center_x(Length::Fill)
            .center_y(Length::Fill)
            .into()
        }
        MenuEditorStatus::Ready | MenuEditorStatus::Saving => {
            if state.items.is_empty() {
                centered_status(locale, "menuEditor.empty")
            } else {
                scrollable(tree_view(state, locale))
                    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                    .style(inputs::scrollable_style)
                    .height(Length::Fill)
                    .into()
            }
        }
    };

    let content = inputs::card(content).height(Length::Fill);
    container(column![header, toolbar, content].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn toolbar_icon(
    locale: UiLocale,
    key: &str,
    glyph: &'static str,
    message: MenuEditorMsg,
    enabled: bool,
    destructive: bool,
) -> Element<'static, Message> {
    let style = if !enabled {
        secondary_button
    } else if destructive {
        danger_button
    } else if key == "common.save" {
        primary_button
    } else {
        secondary_button
    };
    let mut control = button(text(glyph).size(18))
        .width(Length::Fixed(38.0))
        .height(Length::Fixed(34.0))
        .style(style);
    if enabled {
        control = control.on_press(Message::MenuEditor(message));
    }
    tooltip(
        control,
        text(t(locale, key)).size(12),
        tooltip::Position::Bottom,
    )
    .gap(4)
    .into()
}

fn centered_status(locale: UiLocale, key: &str) -> Element<'_, Message> {
    container(
        text(t(locale, key))
            .size(14)
            .color(Color::from_rgb8(0xA8, 0xA8, 0xA8)),
    )
    .center_x(Length::Fill)
    .center_y(Length::Fill)
    .into()
}

fn tree_view<'a>(state: &'a MenuEditorState, locale: UiLocale) -> Element<'a, Message> {
    let mut rows = column![].spacing(4).width(Length::Fill);
    for item in &state.items {
        rows = rows.push(tree_item(state, item, 0, locale));
    }
    rows.into()
}

fn tree_item<'a>(
    state: &'a MenuEditorState,
    item: &'a MenuNode,
    depth: usize,
    locale: UiLocale,
) -> Element<'a, Message> {
    let selected = state.selected_id.as_deref() == Some(&item.id);
    let is_submenu = item.kind == MenuItemKind::Submenu;
    let collapsed = state.collapsed.contains(&item.id);
    let id = item.id.clone();
    let toggle: Element<'_, Message> = if is_submenu {
        button(text(if collapsed { "▸" } else { "▾" }).size(14))
            .on_press(Message::MenuEditor(MenuEditorMsg::ToggleExpanded(
                id.clone(),
            )))
            .padding(4)
            .style(secondary_button)
            .into()
    } else {
        Space::new(Length::Fixed(28.0), 0).into()
    };
    let drag: Element<'_, Message> = if item.id == HOME_ID {
        Space::new(Length::Fixed(24.0), 0).into()
    } else {
        tooltip(
            mouse_area(text("⠿").width(Length::Fixed(24.0)))
                .on_press(Message::MenuEditor(MenuEditorMsg::DragStart(id.clone())))
                .interaction(iced::mouse::Interaction::Grab),
            text(t(locale, "menuEditor.drag")).size(12),
            tooltip::Position::Top,
        )
        .gap(4)
        .into()
    };
    let label = if item.label.is_empty() {
        t(locale, "menuEditor.draft")
    } else {
        item.label.clone()
    };
    let label_button = button(
        row![
            kind_icon(&item.kind),
            text(label).size(14),
            Space::new(Length::Fill, 0),
            text(kind_label(locale, &item.kind))
                .size(11)
                .color(Color::from_rgb8(0x9D, 0xA5, 0xB4))
        ]
        .align_y(Alignment::Center),
    )
    .on_press(Message::MenuEditor(MenuEditorMsg::Select(id.clone())))
    .padding([8, 10])
    .width(Length::Fill)
    .style(move |_theme: &Theme, _status| button::Style {
        background: selected.then_some(Background::Color(Color::from_rgb8(0x26, 0x4F, 0x78))),
        text_color: Color::from_rgb8(0xE4, 0xE4, 0xE4),
        border: Border {
            radius: 5.0.into(),
            ..Border::default()
        },
        ..button::Style::default()
    });
    let row_content = row![
        Space::new(Length::Fixed(depth as f32 * 22.0), 0),
        toggle,
        drag,
        label_button
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .height(Length::Fixed(46.0));
    let target = state
        .drop_target
        .as_ref()
        .filter(|(target, _)| target == &item.id)
        .map(|(_, position)| *position);
    let target_id = id.clone();
    let drop_row = mouse_area(
        container(row_content)
            .padding(Padding::from([1, 4]))
            .width(Length::Fill)
            .style(move |_theme| {
                let color = Color::from_rgb8(0x00, 0x7F, 0xD4);
                let mut border = Border {
                    radius: 5.0.into(),
                    ..Border::default()
                };
                if target == Some(DropPosition::Inside) {
                    border.color = color;
                    border.width = 1.0;
                }
                container::Style {
                    border,
                    ..container::Style::default()
                }
            }),
    )
    .on_move(move |point: Point| {
        let position = if point.y < 14.0 {
            DropPosition::Before
        } else if point.y > 32.0 {
            DropPosition::After
        } else {
            DropPosition::Inside
        };
        Message::MenuEditor(MenuEditorMsg::DragOver(target_id.clone(), position))
    })
    .on_exit(Message::MenuEditor(MenuEditorMsg::DragLeave(id.clone())))
    .on_release(Message::MenuEditor(MenuEditorMsg::Drop));

    let mut result = column![].spacing(3).width(Length::Fill);
    if target == Some(DropPosition::Before) {
        result = result.push(drop_indicator(depth));
    }
    result = result.push(drop_row);
    if state
        .draft
        .as_ref()
        .is_some_and(|draft| draft.item_id == item.id)
    {
        result = result.push(draft_editor(state, depth + 1, locale));
    }
    if is_submenu && !collapsed {
        for child in &item.children {
            result = result.push(tree_item(state, child, depth + 1, locale));
        }
    }
    if target == Some(DropPosition::After) {
        result = result.push(drop_indicator(depth));
    }
    result.into()
}

fn drop_indicator(depth: usize) -> Element<'static, Message> {
    row![
        Space::new(Length::Fixed(depth as f32 * 22.0 + 60.0), 0),
        container(Space::new(Length::Fill, Length::Fixed(2.0))).style(|_theme| {
            container::Style {
                background: Some(Background::Color(Color::from_rgb8(0x00, 0x7F, 0xD4))),
                ..container::Style::default()
            }
        })
    ]
    .into()
}

fn draft_editor<'a>(
    state: &'a MenuEditorState,
    depth: usize,
    locale: UiLocale,
) -> Element<'a, Message> {
    let draft = state.draft.as_ref().expect("draft editor requires a draft");
    let placeholder = match draft.kind {
        DraftKind::Page => "menuEditor.pagePlaceholder",
        DraftKind::Category => "menuEditor.categoryPlaceholder",
    };
    let input = text_input(&t(locale, placeholder), &draft.query)
        .on_input(|value| Message::MenuEditor(MenuEditorMsg::DraftChanged(value)))
        .on_submit(Message::MenuEditor(MenuEditorMsg::SubmitDraft))
        .padding([8, 10])
        .style(field_style)
        .width(Length::Fill);
    let mut choices = column![].spacing(4);
    match draft.kind {
        DraftKind::Page => {
            for page in state
                .pages
                .iter()
                .filter(|page| page_matches_query(page, &draft.query))
            {
                choices = choices.push(
                    button(text(page.title.clone()))
                        .on_press(Message::MenuEditor(MenuEditorMsg::ChoosePage(
                            page.id.clone(),
                        )))
                        .style(secondary_button)
                        .width(Length::Fill),
                );
            }
        }
        DraftKind::Category => {
            for category in state
                .categories
                .iter()
                .filter(|name| category_matches_query(name, &draft.query))
            {
                choices = choices.push(
                    button(text(category.clone()))
                        .on_press(Message::MenuEditor(MenuEditorMsg::ChooseCategory(
                            category.clone(),
                        )))
                        .style(secondary_button)
                        .width(Length::Fill),
                );
            }
        }
    }
    let submit_label = match draft.kind {
        DraftKind::Page => "menuEditor.createSubmenu",
        DraftKind::Category => "menuEditor.useCategory",
    };
    let actions = row![
        button(text(t(locale, submit_label)))
            .on_press(Message::MenuEditor(MenuEditorMsg::SubmitDraft))
            .style(primary_button),
        button(text(t(locale, "common.cancel")))
            .on_press(Message::MenuEditor(MenuEditorMsg::CancelDraft))
            .style(secondary_button),
    ]
    .spacing(8);
    let mut editor = column![input, choices, actions].spacing(8);
    if draft.validation_failed {
        editor = editor.push(
            text(t(locale, "menuEditor.categoryRequired"))
                .size(12)
                .color(Color::from_rgb8(0xF4, 0x87, 0x71)),
        );
    }
    container(row![
        Space::new(Length::Fixed(depth as f32 * 22.0 + 60.0), 0),
        editor.width(Length::Fill)
    ])
    .padding([8, 8])
    .into()
}

fn page_matches_query(page: &PageOption, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || page.title.to_lowercase().contains(&query)
        || page.slug.to_lowercase().contains(&query)
}

fn category_matches_query(category: &str, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty() || category.to_lowercase().contains(&query)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn inserts_after_a_leaf_and_as_first_child_of_a_submenu() {
        let mut state = fixture();
        state.selected_id = Some("page-one".into());
        state.start_draft(DraftKind::Page);
        state.draft_changed("Nested".into());
        state.submit_submenu("New submenu").unwrap();
        assert_eq!(
            labels(&state.items),
            vec!["Home", "One", "Nested", "Section"]
        );

        state.selected_id = Some("section".into());
        state.start_draft(DraftKind::Page);
        state.choose_page("page-two").unwrap();
        assert_eq!(labels(&state.items[3].children), vec!["Two"]);
    }

    #[test]
    fn cancelling_a_draft_removes_it() {
        let mut state = fixture();
        state.selected_id = Some("page-one".into());
        state.start_draft(DraftKind::Category);
        assert!(state.draft.is_some());
        state.cancel_draft();
        assert_eq!(labels(&state.items), vec!["Home", "One", "Section"]);
    }

    #[test]
    fn moves_indents_and_unindents_with_home_protected() {
        let mut state = fixture();
        state.selected_id = Some("section".into());
        assert!(state.move_selected(MoveDirection::Up));
        assert_eq!(labels(&state.items), vec!["Home", "Section", "One"]);
        assert!(!state.move_selected(MoveDirection::Up));
        assert!(state.move_selected(MoveDirection::Down));
        assert_eq!(labels(&state.items), vec!["Home", "One", "Section"]);
        assert!(state.move_selected(MoveDirection::Up));

        state.selected_id = Some("page-one".into());
        assert!(state.indent_selected());
        assert_eq!(labels(&state.items[1].children), vec!["One"]);
        assert!(state.unindent_selected());
        assert_eq!(labels(&state.items), vec!["Home", "Section", "One"]);

        state.selected_id = Some(HOME_ID.into());
        assert!(!state.move_selected(MoveDirection::Down));
        assert!(!state.delete_selected());
    }

    #[test]
    fn drag_drop_supports_positions_and_rejects_invalid_targets() {
        let mut state = fixture();
        assert!(state.drop_item("page-one", "section", DropPosition::Inside));
        let section = find_path(&state.items, "section").unwrap();
        assert_eq!(
            labels(&item_at(&state.items, &section).unwrap().children),
            vec!["One"]
        );
        assert!(state.drop_item("page-one", "section", DropPosition::Before));
        assert_eq!(labels(&state.items), vec!["Home", "One", "Section"]);
        assert!(state.drop_item("page-one", "section", DropPosition::After));
        assert_eq!(labels(&state.items), vec!["Home", "Section", "One"]);
        assert!(state.drop_item("page-one", "section", DropPosition::Before));
        assert!(!state.drop_item("section", "page-one", DropPosition::Inside));
        assert!(!state.drop_item("section", "section", DropPosition::After));
        assert!(!state.drop_item("section", HOME_ID, DropPosition::Before));
        assert!(!state.drop_item(HOME_ID, "section", DropPosition::After));

        state.items[2]
            .children
            .push(node("child", MenuItemKind::Submenu, "Child", None));
        assert!(!state.drop_item("section", "child", DropPosition::Inside));
    }

    #[test]
    fn category_drafts_validate_and_normalize_existing_names() {
        let mut state = fixture();
        state.start_draft(DraftKind::Category);
        assert!(state.submit_category().is_err());
        assert!(state.draft.as_ref().unwrap().validation_failed);
        state.draft_changed(" ARTICLES ".into());
        assert_eq!(state.submit_category().unwrap(), ("articles".into(), false));

        state.start_draft(DraftKind::Category);
        state.draft_changed(" Long Form ".into());
        assert_eq!(state.submit_category().unwrap(), ("Long Form".into(), true));
        assert_eq!(state.categories, vec!["articles", "Long Form"]);
    }

    #[test]
    fn collapsed_submenus_expand_only_after_the_drag_delay() {
        let mut state = fixture();
        state.collapsed.insert("section".into());
        state.dragging_id = Some("page-one".into());
        let started = Instant::now();
        state.drag_over("section".into(), DropPosition::Inside, started);
        assert!(!state.expand_hovered(started + DRAG_EXPAND_DELAY - Duration::from_millis(1)));
        assert!(state.collapsed.contains("section"));
        assert!(state.expand_hovered(started + DRAG_EXPAND_DELAY));
        assert!(!state.collapsed.contains("section"));
    }

    #[test]
    fn draft_search_matches_bds2_title_slug_and_trimmed_category_rules() {
        let page = PageOption {
            id: "page".into(),
            title: "About Us".into(),
            slug: "company-profile".into(),
        };
        assert!(page_matches_query(&page, " ABOUT "));
        assert!(page_matches_query(&page, "profile"));
        assert!(!page_matches_query(&page, "contact"));
        assert!(category_matches_query("Long Form", " long "));
    }

    fn fixture() -> MenuEditorState {
        MenuEditorState::ready(
            "project".into(),
            vec![
                node(HOME_ID, MenuItemKind::Home, "Home", None),
                node("page-one", MenuItemKind::Page, "One", Some("one")),
                node("section", MenuItemKind::Submenu, "Section", None),
            ],
            vec![PageOption {
                id: "page-two".into(),
                title: "Two".into(),
                slug: "two".into(),
            }],
            vec!["articles".into()],
        )
    }

    fn node(id: &str, kind: MenuItemKind, label: &str, slug: Option<&str>) -> MenuNode {
        MenuNode {
            id: id.into(),
            kind,
            label: label.into(),
            slug: slug.map(str::to_string),
            children: Vec::new(),
        }
    }

    fn labels(items: &[MenuNode]) -> Vec<&str> {
        items.iter().map(|item| item.label.as_str()).collect()
    }
}
