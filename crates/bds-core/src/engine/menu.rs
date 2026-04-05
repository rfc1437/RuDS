use std::fs;
use std::path::Path;

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
            Self::CategoryArchive => "category_archive",
        }
    }

    fn from_str(s: &str) -> Self {
        match s {
            "home" => Self::Home,
            "submenu" => Self::Submenu,
            "category_archive" => Self::CategoryArchive,
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
    Ok(parse_opml(&content))
}

/// Write the navigation menu to meta/menu.opml.
/// Per menu.allium UpdateMenu rule: Home entry is always extracted and prepended.
pub fn write_menu(data_dir: &Path, items: &[MenuItem]) -> EngineResult<()> {
    let normalized = normalize_menu(items);
    let opml = serialize_opml(&normalized);
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
    serialize_opml(&items)
}

/// Per menu.allium HomeAlwaysPresent: ensure Home is always first.
fn normalize_menu(items: &[MenuItem]) -> Vec<MenuItem> {
    let without_home: Vec<MenuItem> = items
        .iter()
        .filter(|i| i.kind != MenuItemKind::Home)
        .cloned()
        .collect();
    let mut result = vec![MenuItem {
        kind: MenuItemKind::Home,
        label: "Home".to_string(),
        slug: None,
        children: Vec::new(),
    }];
    result.extend(without_home);
    result
}

/// Parse OPML 2.0 XML into menu items.
fn parse_opml(content: &str) -> Vec<MenuItem> {
    // Simple XML parsing using quick-xml-style manual parsing.
    // OPML structure: <opml><body><outline .../></body></opml>
    let mut items = Vec::new();
    parse_outlines(content, &mut items);

    // Ensure HomeAlwaysPresent
    if items.is_empty() || items[0].kind != MenuItemKind::Home {
        let without_home: Vec<MenuItem> = items
            .into_iter()
            .filter(|i| i.kind != MenuItemKind::Home)
            .collect();
        let mut normalized = vec![MenuItem {
            kind: MenuItemKind::Home,
            label: "Home".to_string(),
            slug: None,
            children: Vec::new(),
        }];
        normalized.extend(without_home);
        return normalized;
    }
    items
}

/// Simple OPML outline parser.
/// Parses <outline text="..." type="..." htmlUrl="..."> elements.
fn parse_outlines(xml: &str, items: &mut Vec<MenuItem>) {
    // Find all outline elements at the body level
    let body_start = xml.find("<body>");
    let body_end = xml.find("</body>");
    if body_start.is_none() || body_end.is_none() {
        return;
    }
    let body = &xml[body_start.unwrap() + 6..body_end.unwrap()];
    parse_outline_children(body, items);
}

fn parse_outline_children(content: &str, items: &mut Vec<MenuItem>) {
    let mut pos = 0;
    while pos < content.len() {
        // Find next <outline
        let Some(start) = content[pos..].find("<outline") else {
            break;
        };
        let abs_start = pos + start;
        let tag_start = abs_start;

        // Find the end of the opening tag
        let rest = &content[tag_start..];
        let is_self_closing;
        let tag_end;

        if let Some(sc) = rest.find("/>") {
            if let Some(gt) = rest.find('>') {
                if sc < gt {
                    is_self_closing = true;
                    tag_end = tag_start + sc + 2;
                } else {
                    is_self_closing = false;
                    tag_end = tag_start + gt + 1;
                }
            } else {
                is_self_closing = true;
                tag_end = tag_start + sc + 2;
            }
        } else if let Some(gt) = rest.find('>') {
            is_self_closing = false;
            tag_end = tag_start + gt + 1;
        } else {
            break;
        }

        let tag_content = &content[tag_start..tag_end];

        // Extract attributes
        let label = extract_attr(tag_content, "text")
            .unwrap_or_else(|| "Untitled".to_string());
        let kind_str = extract_attr(tag_content, "type")
            .unwrap_or_else(|| "page".to_string());
        let slug = extract_attr(tag_content, "htmlUrl");
        let kind = MenuItemKind::from_str(&kind_str);

        let mut children = Vec::new();
        let after_tag;

        if is_self_closing {
            after_tag = tag_end;
        } else {
            // Find matching </outline>
            let inner = &content[tag_end..];
            if let Some(close_idx) = find_closing_outline(inner) {
                let inner_content = &inner[..close_idx];
                parse_outline_children(inner_content, &mut children);
                after_tag = tag_end + close_idx + "</outline>".len();
            } else {
                after_tag = tag_end;
            }
        }

        items.push(MenuItem {
            kind,
            label,
            slug,
            children,
        });
        pos = after_tag;
    }
}

fn find_closing_outline(content: &str) -> Option<usize> {
    let mut depth = 0i32;
    let mut pos = 0;
    while pos < content.len() {
        if content[pos..].starts_with("<outline") {
            if let Some(sc) = content[pos..].find("/>") {
                if let Some(gt) = content[pos..].find('>') {
                    if sc < gt {
                        pos += sc + 2;
                        continue;
                    }
                }
            }
            depth += 1;
            pos += 8; // skip past "<outline"
        } else if content[pos..].starts_with("</outline>") {
            if depth == 0 {
                return Some(pos);
            }
            depth -= 1;
            pos += 10;
        } else {
            pos += 1;
        }
    }
    None
}

fn extract_attr(tag: &str, name: &str) -> Option<String> {
    let pattern = format!("{name}=\"");
    let start = tag.find(&pattern)?;
    let value_start = start + pattern.len();
    let rest = &tag[value_start..];
    let end = rest.find('"')?;
    let value = &rest[..end];
    // Unescape XML entities
    Some(
        value
            .replace("&amp;", "&")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&quot;", "\"")
            .replace("&apos;", "'"),
    )
}

/// Serialize menu items to OPML 2.0 format.
fn serialize_opml(items: &[MenuItem]) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n");
    out.push_str("<opml version=\"2.0\">\n");
    out.push_str("  <head><title>Blog Menu</title></head>\n");
    out.push_str("  <body>\n");
    for item in items {
        serialize_outline(&mut out, item, 2);
    }
    out.push_str("  </body>\n");
    out.push_str("</opml>\n");
    out
}

fn serialize_outline(out: &mut String, item: &MenuItem, indent: usize) {
    let pad = "  ".repeat(indent);
    out.push_str(&pad);
    out.push_str("<outline");
    out.push_str(&format!(
        " text=\"{}\"",
        xml_escape(&item.label)
    ));
    out.push_str(&format!(
        " type=\"{}\"",
        item.kind.as_str()
    ));
    if let Some(ref slug) = item.slug {
        out.push_str(&format!(
            " htmlUrl=\"{}\"",
            xml_escape(slug)
        ));
    }
    if item.children.is_empty() {
        out.push_str("/>\n");
    } else {
        out.push_str(">\n");
        for child in &item.children {
            serialize_outline(out, child, indent + 1);
        }
        out.push_str(&pad);
        out.push_str("</outline>\n");
    }
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
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
                children: vec![
                    MenuItem {
                        kind: MenuItemKind::CategoryArchive,
                        label: "Tech".into(),
                        slug: Some("/category/tech/".into()),
                        children: Vec::new(),
                    },
                ],
            },
        ];
        let opml = serialize_opml(&items);
        let parsed = parse_opml(&opml);
        assert_eq!(parsed.len(), 3);
        assert_eq!(parsed[0].kind, MenuItemKind::Home);
        assert_eq!(parsed[1].label, "About");
        assert_eq!(parsed[1].slug.as_deref(), Some("/about"));
        assert_eq!(parsed[2].children.len(), 1);
        assert_eq!(parsed[2].children[0].label, "Tech");
    }

    #[test]
    fn normalize_prepends_home() {
        let items = vec![
            MenuItem {
                kind: MenuItemKind::Page,
                label: "About".into(),
                slug: Some("/about".into()),
                children: Vec::new(),
            },
        ];
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
        let items = vec![
            MenuItem {
                kind: MenuItemKind::Page,
                label: "Blog".into(),
                slug: Some("/blog".into()),
                children: Vec::new(),
            },
        ];
        write_menu(dir.path(), &items).unwrap();
        let read = read_menu(dir.path()).unwrap();
        // Home is always prepended
        assert_eq!(read.len(), 2);
        assert_eq!(read[0].kind, MenuItemKind::Home);
        assert_eq!(read[1].label, "Blog");
    }

    #[test]
    fn xml_escape_special_chars() {
        let escaped = xml_escape("Tom & Jerry <3 \"quotes\"");
        assert_eq!(escaped, "Tom &amp; Jerry &lt;3 &quot;quotes&quot;");
    }
}
