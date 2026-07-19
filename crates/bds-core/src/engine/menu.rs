use std::fs;
use std::path::Path;

use quick_xml::events::{BytesDecl, BytesEnd, BytesStart, BytesText, Event};
use quick_xml::{Reader, Writer, XmlVersion, escape::escape};

use crate::engine::EngineError;
use crate::engine::EngineResult;
use crate::util::atomic_write_str;

/// A navigation menu item per menu.allium.
#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub kind: MenuItemKind,
    pub label: String,
    pub slug: Option<String>,
    pub children: Vec<MenuItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum MenuItemKind {
    Home,
    Page,
    Submenu,
    CategoryArchive,
}

impl MenuItemKind {
    fn as_str(&self) -> &'static str {
        match self {
            Self::Home => "home",
            Self::Page => "page",
            Self::Submenu => "submenu",
            Self::CategoryArchive => "category-archive",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "home" => Self::Home,
            "submenu" => Self::Submenu,
            "category-archive" | "category_archive" => Self::CategoryArchive,
            _ => Self::Page,
        }
    }
}

/// Read the navigation menu from meta/menu.opml.
pub fn read_menu(data_dir: &Path) -> EngineResult<Vec<MenuItem>> {
    let path = data_dir.join("meta").join("menu.opml");
    if !path.exists() {
        return Ok(vec![MenuItem {
            kind: MenuItemKind::Home,
            label: "Home".to_string(),
            slug: None,
            children: Vec::new(),
        }]);
    }
    let content = fs::read_to_string(&path)?;
    parse_opml(&content)
}

/// Write the navigation menu to meta/menu.opml.
/// Per menu.allium UpdateMenu rule: Home entry is always extracted and prepended.
pub fn write_menu(data_dir: &Path, items: &[MenuItem]) -> EngineResult<()> {
    let normalized = normalize_menu(items);
    let opml = serialize_opml(&normalized)?;
    let path = data_dir.join("meta").join("menu.opml");
    atomic_write_str(&path, &opml)?;
    Ok(())
}

/// Return the default menu OPML for new projects.
pub fn default_menu_opml() -> String {
    let items = vec![MenuItem {
        kind: MenuItemKind::Home,
        label: "Home".to_string(),
        slug: None,
        children: Vec::new(),
    }];
    serialize_opml(&items).expect("writing menu XML to memory cannot fail")
}

/// Per menu.allium HomeAlwaysPresent: ensure Home is always first.
fn normalize_menu(items: &[MenuItem]) -> Vec<MenuItem> {
    let without_home: Vec<_> = items.iter().filter_map(normalize_non_home).collect();
    let mut result = vec![MenuItem {
        kind: MenuItemKind::Home,
        label: "Home".to_string(),
        slug: None,
        children: Vec::new(),
    }];
    result.extend(without_home);
    result
}

fn normalize_non_home(item: &MenuItem) -> Option<MenuItem> {
    if item.kind == MenuItemKind::Home {
        return None;
    }
    Some(MenuItem {
        kind: item.kind.clone(),
        label: item.label.clone(),
        slug: match item.kind {
            MenuItemKind::Page | MenuItemKind::CategoryArchive => item.slug.clone(),
            MenuItemKind::Home | MenuItemKind::Submenu => None,
        },
        children: if item.kind == MenuItemKind::Submenu {
            item.children
                .iter()
                .filter_map(normalize_non_home)
                .collect()
        } else {
            Vec::new()
        },
    })
}

/// Parse OPML 2.0 XML into menu items.
fn parse_opml(content: &str) -> EngineResult<Vec<MenuItem>> {
    let mut reader = Reader::from_str(content);
    reader.config_mut().trim_text(true);
    let mut items = Vec::new();
    let mut outlines: Vec<Option<MenuItem>> = Vec::new();
    let mut elements: Vec<Vec<u8>> = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Start(event)) => {
                let name = event.name().as_ref().to_vec();
                if name == b"outline" {
                    let collect = collect_outline(&elements, &outlines);
                    outlines.push(collect.then(|| parse_outline(&event)).transpose()?);
                }
                elements.push(name);
            }
            Ok(Event::Empty(event)) if event.name().as_ref() == b"outline" => {
                if collect_outline(&elements, &outlines) {
                    attach_outline(parse_outline(&event)?, &mut outlines, &mut items);
                }
            }
            Ok(Event::End(event)) => {
                let name = event.name().as_ref().to_vec();
                let open = elements
                    .pop()
                    .ok_or_else(|| EngineError::Parse("unexpected closing element".to_string()))?;
                if open != name {
                    return Err(EngineError::Parse("mismatched closing element".to_string()));
                }
                if name == b"outline" {
                    let item = outlines
                        .pop()
                        .ok_or_else(|| EngineError::Parse("unexpected </outline>".to_string()))?;
                    if let Some(mut item) = item {
                        if item.kind != MenuItemKind::Submenu {
                            item.children.clear();
                        }
                        attach_outline(item, &mut outlines, &mut items);
                    }
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => return Err(EngineError::Parse(error.to_string())),
        }
    }

    if !outlines.is_empty() || !elements.is_empty() {
        return Err(EngineError::Parse("unclosed <outline>".to_string()));
    }
    Ok(normalize_menu(&items))
}

fn collect_outline(elements: &[Vec<u8>], outlines: &[Option<MenuItem>]) -> bool {
    elements == [b"opml".as_slice(), b"body".as_slice()]
        || (elements.last().is_some_and(|name| name == b"outline")
            && outlines.last().is_some_and(Option::is_some))
}

fn parse_outline(event: &BytesStart<'_>) -> EngineResult<MenuItem> {
    let mut label = String::new();
    let mut kind_value = None;
    let mut legacy_kind = None;
    let mut page_slug = None;
    let mut category_name = None;
    let mut legacy_slug = None;
    let mut html_url = None;

    for attribute in event.attributes() {
        let attribute = attribute.map_err(|error| EngineError::Parse(error.to_string()))?;
        let value = attribute
            .decoded_and_normalized_value(XmlVersion::Implicit1_0, event.decoder())
            .map_err(|error| EngineError::Parse(error.to_string()))?
            .into_owned();
        match attribute.key.as_ref() {
            b"text" => label = value,
            b"type" => kind_value = Some(value),
            b"kind" => legacy_kind = Some(value),
            b"pageSlug" => page_slug = Some(value),
            b"categoryName" => category_name = Some(value),
            b"slug" => legacy_slug = Some(value),
            b"htmlUrl" => html_url = Some(value),
            _ => {}
        }
    }

    let kind = kind_value
        .or(legacy_kind)
        .as_deref()
        .map(MenuItemKind::from_str)
        .unwrap_or(MenuItemKind::Page);
    let slug = match kind {
        MenuItemKind::Home | MenuItemKind::Submenu => None,
        MenuItemKind::Page => page_slug.or(legacy_slug).or(html_url),
        MenuItemKind::CategoryArchive => category_name.or(legacy_slug).or(html_url),
    }
    .filter(|value| !value.is_empty());

    Ok(MenuItem {
        kind,
        label,
        slug,
        children: Vec::new(),
    })
}

fn attach_outline(item: MenuItem, parents: &mut [Option<MenuItem>], items: &mut Vec<MenuItem>) {
    if let Some(Some(parent)) = parents.last_mut() {
        parent.children.push(item);
    } else {
        items.push(item);
    }
}

/// Serialize menu items to OPML 2.0 format.
fn serialize_opml(items: &[MenuItem]) -> EngineResult<String> {
    let mut writer = Writer::new_with_indent(Vec::new(), b' ', 2);
    writer
        .write_event(Event::Decl(BytesDecl::new("1.0", Some("UTF-8"), None)))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    let mut opml = BytesStart::new("opml");
    opml.push_attribute(("version", "2.0"));
    writer
        .write_event(Event::Start(opml))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("head")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("title")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::Text(BytesText::new("Blog Menu")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("title")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("head")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::Start(BytesStart::new("body")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    for item in items {
        write_outline(&mut writer, item).map_err(|error| EngineError::Parse(error.to_string()))?;
    }
    writer
        .write_event(Event::End(BytesEnd::new("body")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    writer
        .write_event(Event::End(BytesEnd::new("opml")))
        .map_err(|error| EngineError::Parse(error.to_string()))?;
    String::from_utf8(writer.into_inner()).map_err(|error| EngineError::Parse(error.to_string()))
}

fn write_outline(writer: &mut Writer<Vec<u8>>, item: &MenuItem) -> quick_xml::Result<()> {
    let label = escape(&item.label);
    let mut outline = BytesStart::new("outline");
    outline.push_attribute(("text", label.as_ref()));
    outline.push_attribute(("type", item.kind.as_str()));
    match item.kind {
        MenuItemKind::Home => outline.push_attribute(("pageSlug", "home")),
        MenuItemKind::Page => {
            if let Some(slug) = item.slug.as_deref().map(escape) {
                outline.push_attribute(("pageSlug", slug.as_ref()));
            }
        }
        MenuItemKind::CategoryArchive => {
            if let Some(slug) = item.slug.as_deref().map(escape) {
                outline.push_attribute(("categoryName", slug.as_ref()));
            }
        }
        MenuItemKind::Submenu => {}
    }
    if item.children.is_empty() {
        writer.write_event(Event::Empty(outline))?;
    } else {
        writer.write_event(Event::Start(outline))?;
        for child in &item.children {
            write_outline(writer, child)?;
        }
        writer.write_event(Event::End(BytesEnd::new("outline")))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_opml_has_home() {
        let opml = default_menu_opml();
        assert!(opml.contains("type=\"home\""));
        assert!(opml.contains("text=\"Home\""));
    }

    #[test]
    fn roundtrip_menu() {
        let items = vec![
            MenuItem {
                kind: MenuItemKind::Home,
                label: "Home".into(),
                slug: None,
                children: Vec::new(),
            },
            MenuItem {
                kind: MenuItemKind::Page,
                label: "About".into(),
                slug: Some("/about".into()),
                children: Vec::new(),
            },
            MenuItem {
                kind: MenuItemKind::Submenu,
                label: "Categories".into(),
                slug: None,
                children: vec![MenuItem {
                    kind: MenuItemKind::CategoryArchive,
                    label: "Tech".into(),
                    slug: Some("/category/tech/".into()),
                    children: Vec::new(),
                }],
            },
        ];
        let opml = serialize_opml(&items).unwrap();
        let parsed = parse_opml(&opml).unwrap();
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].kind, MenuItemKind::Home);
        assert_eq!(parsed[1].label, "About");
        assert_eq!(parsed[1].slug.as_deref(), Some("/about"));
        assert_eq!(parsed[2].children.len(), 1);
        assert_eq!(parsed[2].children[0].label, "Tech");
    }

    #[test]
    fn normalize_prepends_home() {
        let items = vec![MenuItem {
            kind: MenuItemKind::Page,
            label: "About".into(),
            slug: Some("/about".into()),
            children: Vec::new(),
        }];
        let normalized = normalize_menu(&items);
        assert_eq!(normalized.len(), 2);
        assert_eq!(normalized[0].kind, MenuItemKind::Home);
        assert_eq!(normalized[1].label, "About");
    }

    #[test]
    fn read_menu_missing_file_returns_home() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        let items = read_menu(dir.path()).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].kind, MenuItemKind::Home);
    }

    #[test]
    fn write_and_read_menu() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        let items = vec![MenuItem {
            kind: MenuItemKind::Page,
            label: "Blog".into(),
            slug: Some("/blog".into()),
            children: Vec::new(),
        }];
        write_menu(dir.path(), &items).unwrap();
        let read = read_menu(dir.path()).unwrap();
        // Home is always prepended
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].kind, MenuItemKind::Home);
        assert_eq!(read[1].label, "Blog");
    }

    #[test]
    fn reads_single_quoted_xml_attributes() {
        let parsed = parse_opml(
            "<opml version='2.0'><body><outline text='About' type='page' htmlUrl='/about'/></body></opml>",
        )
        .unwrap();
        assert_eq!(parsed[1].label, "About");
        assert_eq!(parsed[1].slug.as_deref(), Some("/about"));
    }

    #[test]
    fn canonical_bds2_attributes_round_trip_without_legacy_output() {
        let parsed = parse_opml(
            "<opml version='2.0'><body><outline text='Home' type='home' pageSlug='home'/><outline text='Sections' type='submenu'><outline text='About' type='page' pageSlug='about'/><outline text='Notes' type='category-archive' categoryName='notes'/></outline></body></opml>",
        )
        .unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[1].children[0].slug.as_deref(), Some("about"));
        assert_eq!(parsed[1].children[1].kind, MenuItemKind::CategoryArchive);
        assert_eq!(parsed[1].children[1].slug.as_deref(), Some("notes"));

        let serialized = serialize_opml(&parsed).unwrap();
        assert!(serialized.contains("type=\"home\" pageSlug=\"home\""));
        assert!(serialized.contains("type=\"page\" pageSlug=\"about\""));
        assert!(serialized.contains("type=\"category-archive\" categoryName=\"notes\""));
        assert!(!serialized.contains("htmlUrl"));
        assert!(!serialized.contains("category_archive"));
    }

    #[test]
    fn parser_ignores_foreign_outlines_and_drops_children_of_non_submenus() {
        let parsed = parse_opml(
            "<opml><head><outline text='Head'/></head><body><section><outline text='Foreign'/></section><outline text='Page' type='page' pageSlug='page'><outline text='Dropped' type='page' pageSlug='dropped'/></outline><outline text='Kept' type='submenu'><outline text='Child' type='page' pageSlug='child'/></outline></body></opml>",
        )
        .unwrap();

        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[1].label, "Page");
        assert!(parsed[1].children.is_empty());
        assert_eq!(parsed[2].label, "Kept");
        assert_eq!(parsed[2].children[0].label, "Child");
    }

    #[test]
    fn normalization_removes_home_entries_at_every_depth() {
        let normalized = normalize_menu(&[
            MenuItem {
                kind: MenuItemKind::Home,
                label: "Duplicate".into(),
                slug: None,
                children: Vec::new(),
            },
            MenuItem {
                kind: MenuItemKind::Submenu,
                label: "Sections".into(),
                slug: None,
                children: vec![MenuItem {
                    kind: MenuItemKind::Home,
                    label: "Nested Home".into(),
                    slug: None,
                    children: Vec::new(),
                }],
            },
        ]);

        assert_eq!(normalized[0].kind, MenuItemKind::Home);
        assert_eq!(normalized[0].label, "Home");
        assert!(normalized[1].children.is_empty());
    }

    #[test]
    fn read_menu_rejects_malformed_xml() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("meta")).unwrap();
        std::fs::write(
            dir.path().join("meta/menu.opml"),
            "<opml><body><outline></body></opml>",
        )
        .unwrap();

        assert!(read_menu(dir.path()).is_err());
    }
}
