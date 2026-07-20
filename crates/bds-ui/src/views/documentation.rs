use std::collections::{HashMap, hash_map::DefaultHasher};
use std::fs;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, UNIX_EPOCH};

use bds_core::i18n::UiLocale;
use iced::widget::{button, column, container, markdown, row, scrollable, text};
use iced::{Element, Length};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;

const API_REFERENCE: &str = include_str!("../../../../docs/scripting/API_REFERENCE.md");
const API_TYPES: &str = include_str!("../../../../docs/scripting/TYPES.md");
const USER_GUIDE: &str = include_str!("../../../../DOCUMENTATION.md");
const CLI_GUIDE: &str = include_str!("../../../../CLI.md");
const MCP_GUIDE: &str = include_str!("../../../../MCP.md");
const UTILITY_EXAMPLE: &str = include_str!("../../../../docs/scripting/examples/utility.lua");
const MACRO_EXAMPLE: &str = include_str!("../../../../docs/scripting/examples/macro.lua");
const TRANSFORM_EXAMPLE: &str = include_str!("../../../../docs/scripting/examples/transform.lua");
pub const API_DOCUMENTATION_URL: &str = "https://ruds.invalid/api-documentation";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentationKind {
    Guide,
    Api,
    Cli,
    Mcp,
}

#[derive(Debug, Clone)]
pub enum DocumentLoad {
    Ready { source: String, signature: u64 },
    Missing { signature: u64 },
    Malformed { signature: u64, error: String },
}

impl DocumentLoad {
    pub fn signature(&self) -> u64 {
        match self {
            Self::Ready { signature, .. }
            | Self::Missing { signature }
            | Self::Malformed { signature, .. } => *signature,
        }
    }
}

#[derive(Debug, Clone)]
pub enum DocumentBlock {
    Markdown(Vec<markdown::Item>),
    Table {
        headers: Vec<String>,
        rows: Vec<Vec<String>>,
    },
}

#[derive(Debug, Clone, Default)]
pub struct ParsedDocument {
    pub blocks: Vec<DocumentBlock>,
    pub anchors: HashMap<String, f32>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentStatus {
    NotLoaded,
    Loading,
    Ready,
    Missing,
    Malformed,
}

#[derive(Debug, Clone)]
pub struct DocumentationState {
    pub kind: DocumentationKind,
    pub status: DocumentStatus,
    pub parsed: ParsedDocument,
    pub signature: u64,
    pub error: Option<String>,
    pub load_generation: u64,
    last_checked: Option<Instant>,
}

impl DocumentationState {
    pub fn new(kind: DocumentationKind) -> Self {
        Self {
            kind,
            status: DocumentStatus::NotLoaded,
            parsed: ParsedDocument::default(),
            signature: 0,
            error: None,
            load_generation: 0,
            last_checked: None,
        }
    }

    pub fn start_loading(&mut self) -> u64 {
        self.load_generation = self.load_generation.saturating_add(1);
        self.status = DocumentStatus::Loading;
        self.error = None;
        self.load_generation
    }

    pub fn apply(&mut self, load: DocumentLoad) {
        self.signature = load.signature();
        self.last_checked = Some(Instant::now());
        match load {
            DocumentLoad::Ready { source, .. } => {
                self.parsed = parse_document(&source);
                self.status = DocumentStatus::Ready;
                self.error = None;
            }
            DocumentLoad::Missing { .. } => {
                self.parsed = ParsedDocument::default();
                self.status = DocumentStatus::Missing;
                self.error = None;
            }
            DocumentLoad::Malformed { error, .. } => {
                self.parsed = ParsedDocument::default();
                self.status = DocumentStatus::Malformed;
                self.error = Some(error);
            }
        }
    }

    pub fn should_check(&self) -> bool {
        self.status != DocumentStatus::Loading
            && self
                .last_checked
                .is_none_or(|checked| checked.elapsed() >= Duration::from_secs(1))
    }

    pub fn mark_checked(&mut self) {
        self.last_checked = Some(Instant::now());
    }
}

pub fn scroll_id(kind: DocumentationKind) -> scrollable::Id {
    scrollable::Id::new(match kind {
        DocumentationKind::Guide => "user-documentation",
        DocumentationKind::Api => "api-documentation",
        DocumentationKind::Cli => "cli-documentation",
        DocumentationKind::Mcp => "mcp-documentation",
    })
}

pub fn view(state: &DocumentationState, locale: UiLocale) -> Element<'_, Message> {
    let title_key = match state.kind {
        DocumentationKind::Guide => "documentation.title",
        DocumentationKind::Api => "documentation.apiTitle",
        DocumentationKind::Cli => "documentation.cliTitle",
        DocumentationKind::Mcp => "documentation.mcpTitle",
    };
    let subtitle_key = match state.kind {
        DocumentationKind::Guide => "documentation.subtitle",
        DocumentationKind::Api => "documentation.apiSubtitle",
        DocumentationKind::Cli => "documentation.cliSubtitle",
        DocumentationKind::Mcp => "documentation.mcpSubtitle",
    };
    let toolbar = inputs::toolbar(
        vec![
            column![
                text(t(locale, title_key)).size(20),
                text(t(locale, subtitle_key))
                    .size(12)
                    .color(inputs::LABEL_COLOR)
            ]
            .spacing(3)
            .into(),
        ],
        vec![
            button(text(t(locale, "common.refresh")).size(13))
                .on_press(Message::DocumentationRefresh(state.kind))
                .padding([6, 16])
                .style(inputs::secondary_button)
                .into(),
        ],
    );

    let body: Element<'_, Message> = match state.status {
        DocumentStatus::NotLoaded | DocumentStatus::Loading => {
            status_card(locale, "documentation.loading", inputs::LABEL_COLOR)
        }
        DocumentStatus::Missing => {
            status_card(locale, "documentation.missing", inputs::LABEL_COLOR)
        }
        DocumentStatus::Malformed => status_card(
            locale,
            "documentation.malformed",
            iced::Color::from_rgb(0.90, 0.38, 0.38),
        ),
        DocumentStatus::Ready if state.parsed.blocks.is_empty() => {
            status_card(locale, "documentation.empty", inputs::LABEL_COLOR)
        }
        DocumentStatus::Ready => {
            let mut content = column![].spacing(12).width(Length::Fill);
            for block in &state.parsed.blocks {
                content = content.push(match block {
                    DocumentBlock::Markdown(items) => markdown::view(
                        items,
                        markdown::Settings::with_text_size(14),
                        markdown::Style::from_palette(inputs::app_theme().palette()),
                    )
                    .map({
                        let kind = state.kind;
                        move |url| Message::DocumentationLinkClicked(kind, url.to_string())
                    }),
                    DocumentBlock::Table { headers, rows } => table_view(headers, rows),
                });
            }
            scrollable(container(content).padding(2))
                .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
                .style(inputs::scrollable_style)
                .id(scroll_id(state.kind))
                .height(Length::Fill)
                .into()
        }
    };

    container(column![toolbar, body].spacing(12))
        .padding(16)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn status_card<'a>(locale: UiLocale, key: &str, color: iced::Color) -> Element<'a, Message> {
    inputs::card(text(t(locale, key)).size(14).color(color)).into()
}

fn table_view<'a>(headers: &'a [String], rows: &'a [Vec<String>]) -> Element<'a, Message> {
    let mut table = column![table_row(headers, true)].spacing(5);
    for values in rows {
        table = table.push(table_row(values, false));
    }
    inputs::card(table).into()
}

fn table_row<'a>(values: &'a [String], header: bool) -> Element<'a, Message> {
    let mut cells = row![].spacing(8).width(Length::Fill);
    for value in values {
        let color = if header {
            inputs::SECTION_COLOR
        } else {
            inputs::LABEL_COLOR
        };
        cells = cells.push(
            container(text(value).size(if header { 12 } else { 11 }).color(color))
                .width(Length::FillPortion(1)),
        );
    }
    cells.into()
}

pub fn load_user_guide() -> DocumentLoad {
    load_embedded_document(&user_guide_path(), USER_GUIDE)
}

pub fn load_cli_guide() -> DocumentLoad {
    load_embedded_document(&root_document_path("CLI.md"), CLI_GUIDE)
}

pub fn load_mcp_guide() -> DocumentLoad {
    load_embedded_document(&root_document_path("MCP.md"), MCP_GUIDE)
}

fn load_embedded_document(path: &Path, embedded: &str) -> DocumentLoad {
    match load_document_file(path) {
        DocumentLoad::Missing { signature } => DocumentLoad::Ready {
            source: embedded.to_string(),
            signature,
        },
        load => load,
    }
}

fn load_document_file(path: &Path) -> DocumentLoad {
    let signature = file_signature(path);
    match fs::read_to_string(path) {
        Ok(source) => DocumentLoad::Ready { source, signature },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            DocumentLoad::Missing { signature }
        }
        Err(error) => DocumentLoad::Malformed {
            signature,
            error: error.to_string(),
        },
    }
}

pub fn load_api_document() -> DocumentLoad {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/scripting");
    let sources = [
        (source_root.join("API_REFERENCE.md"), API_REFERENCE),
        (source_root.join("TYPES.md"), API_TYPES),
        (source_root.join("examples/utility.lua"), UTILITY_EXAMPLE),
        (source_root.join("examples/macro.lua"), MACRO_EXAMPLE),
        (
            source_root.join("examples/transform.lua"),
            TRANSFORM_EXAMPLE,
        ),
    ];
    let signature = paths_signature(sources.iter().map(|(path, _)| path));
    let mut loaded = Vec::with_capacity(sources.len());
    for (path, embedded) in sources {
        match fs::read_to_string(&path) {
            Ok(source) => loaded.push(source),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                loaded.push(embedded.to_string());
            }
            Err(error) => {
                return DocumentLoad::Malformed {
                    signature,
                    error: error.to_string(),
                };
            }
        }
    }
    let [reference, types, utility, macro_example, transform] = loaded
        .try_into()
        .expect("five generated documentation sources");
    let source = format!(
        "{reference}\n\n{types}\n\n# Lua Examples\n\n## Utility Lua example\n\n```lua\n{utility}\n```\n\n## Macro Lua example\n\n```lua\n{macro_example}\n```\n\n## Transform Lua example\n\n```lua\n{transform}\n```\n"
    );
    DocumentLoad::Ready { source, signature }
}

pub fn current_signature(kind: DocumentationKind) -> u64 {
    match kind {
        DocumentationKind::Guide => file_signature(&user_guide_path()),
        DocumentationKind::Cli => file_signature(&root_document_path("CLI.md")),
        DocumentationKind::Mcp => file_signature(&root_document_path("MCP.md")),
        DocumentationKind::Api => {
            let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/scripting");
            paths_signature(
                [
                    "API_REFERENCE.md",
                    "TYPES.md",
                    "examples/utility.lua",
                    "examples/macro.lua",
                    "examples/transform.lua",
                ]
                .map(|relative| root.join(relative)),
            )
        }
    }
}

fn user_guide_path() -> PathBuf {
    root_document_path("DOCUMENTATION.md")
}

fn root_document_path(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..").join(name)
}

pub fn parse_document(source: &str) -> ParsedDocument {
    let safe = rewrite_links(&external_images_as_links(source));
    let anchors = heading_anchors(&safe);
    let lines = safe.lines().collect::<Vec<_>>();
    let mut blocks = Vec::new();
    let mut markdown_lines = Vec::new();
    let mut index = 0;
    let mut in_fence = false;
    while index < lines.len() {
        if lines[index].trim_start().starts_with("```") {
            in_fence = !in_fence;
            markdown_lines.push(lines[index]);
            index += 1;
        } else if !in_fence && let Some((headers, rows, consumed)) = parse_table(&lines[index..]) {
            push_markdown(&mut blocks, &mut markdown_lines);
            blocks.push(DocumentBlock::Table { headers, rows });
            index += consumed;
        } else {
            markdown_lines.push(lines[index]);
            index += 1;
        }
    }
    push_markdown(&mut blocks, &mut markdown_lines);
    ParsedDocument { blocks, anchors }
}

fn push_markdown(blocks: &mut Vec<DocumentBlock>, lines: &mut Vec<&str>) {
    if lines.is_empty() {
        return;
    }
    let items = markdown::parse(&lines.join("\n")).collect::<Vec<_>>();
    if !items.is_empty() {
        blocks.push(DocumentBlock::Markdown(items));
    }
    lines.clear();
}

fn parse_table(lines: &[&str]) -> Option<(Vec<String>, Vec<Vec<String>>, usize)> {
    if lines.len() < 2 || !lines[0].contains('|') || !is_table_delimiter(lines[1]) {
        return None;
    }
    let headers = table_cells(lines[0]);
    if headers.is_empty() || table_cells(lines[1]).len() != headers.len() {
        return None;
    }
    let mut rows = Vec::new();
    let mut consumed = 2;
    while consumed < lines.len() && lines[consumed].contains('|') {
        let cells = table_cells(lines[consumed]);
        if cells.len() != headers.len() {
            break;
        }
        rows.push(cells);
        consumed += 1;
    }
    Some((headers, rows, consumed))
}

fn is_table_delimiter(line: &str) -> bool {
    let cells = table_cells(line);
    !cells.is_empty()
        && cells.iter().all(|cell| {
            let trimmed = cell.trim_matches(':');
            trimmed.len() >= 3 && trimmed.chars().all(|character| character == '-')
        })
}

fn table_cells(line: &str) -> Vec<String> {
    let mut cells = Vec::new();
    let mut cell = String::new();
    let mut in_code = false;
    let mut escaped = false;
    for character in line.trim().chars() {
        if escaped {
            cell.push(character);
            escaped = false;
        } else if character == '\\' {
            escaped = true;
        } else if character == '`' {
            in_code = !in_code;
        } else if character == '|' && !in_code {
            cells.push(clean_table_cell(&cell));
            cell.clear();
        } else {
            cell.push(character);
        }
    }
    cells.push(clean_table_cell(&cell));
    if cells.first().is_some_and(String::is_empty) {
        cells.remove(0);
    }
    if cells.last().is_some_and(String::is_empty) {
        cells.pop();
    }
    cells
}

fn clean_table_cell(cell: &str) -> String {
    cell.trim().replace("**", "").replace("__", "")
}

fn external_images_as_links(source: &str) -> String {
    let pattern = regex::Regex::new(r"!\[([^\]]*)\]\((https?://[^)\s]+)\)")
        .expect("external image Markdown regex");
    pattern.replace_all(source, "[🖼 $1]($2)").into_owned()
}

fn rewrite_links(source: &str) -> String {
    let pattern =
        regex::Regex::new(r"(\[[^\]]*\]\()([^\s)]+)([^)]*\))").expect("Markdown link regex");
    pattern
        .replace_all(source, |captures: &regex::Captures<'_>| {
            let target = &captures[2];
            if target == "API.md" {
                return format!("{}{}{}", &captures[1], API_DOCUMENTATION_URL, &captures[3]);
            }
            let Some(anchor) = internal_anchor(target) else {
                return captures[0].to_string();
            };
            format!(
                "{}https://ruds.invalid/document#{}{}",
                &captures[1], anchor, &captures[3]
            )
        })
        .into_owned()
}

fn internal_anchor(target: &str) -> Option<String> {
    if let Some(anchor) = target.strip_prefix('#') {
        return Some(anchor.to_string());
    }
    let (path, fragment) = target.split_once('#').unwrap_or((target, ""));
    let default = match path {
        "API_REFERENCE.md" => "ruds-lua-api-reference",
        "TYPES.md" => "lua-api-types",
        "examples" | "examples/" => "lua-examples",
        "examples/utility.lua" => "utility-lua-example",
        "examples/macro.lua" => "macro-lua-example",
        "examples/transform.lua" => "transform-lua-example",
        _ => return None,
    };
    Some(
        if fragment.is_empty() {
            default
        } else {
            fragment
        }
        .to_string(),
    )
}

fn heading_anchors(source: &str) -> HashMap<String, f32> {
    let total = source.lines().count().max(1) as f32;
    let mut in_fence = false;
    let mut counts = HashMap::<String, usize>::new();
    let mut anchors = HashMap::new();
    for (line_index, line) in source.lines().enumerate() {
        if line.trim_start().starts_with("```") {
            in_fence = !in_fence;
        } else if !in_fence {
            let hashes = line
                .chars()
                .take_while(|character| *character == '#')
                .count();
            if (1..=6).contains(&hashes) && line.chars().nth(hashes) == Some(' ') {
                let base = github_slug(line[hashes + 1..].trim());
                if !base.is_empty() {
                    let count = counts.entry(base.clone()).or_default();
                    let slug = if *count == 0 {
                        base.clone()
                    } else {
                        format!("{base}-{count}")
                    };
                    *count += 1;
                    anchors.insert(slug, line_index as f32 / total);
                }
            }
        }
    }
    anchors
}

fn github_slug(heading: &str) -> String {
    let mut slug = String::new();
    for character in heading.to_lowercase().chars() {
        if character.is_alphanumeric() || character == '-' || character == '_' {
            slug.push(character);
        } else if character.is_whitespace() {
            slug.push('-');
        }
    }
    slug.trim_matches('-').to_string()
}

fn file_signature(path: &Path) -> u64 {
    let Ok(metadata) = fs::metadata(path) else {
        return 0;
    };
    let modified = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_nanos())
        .unwrap_or_default();
    let mut hasher = DefaultHasher::new();
    path.hash(&mut hasher);
    metadata.len().hash(&mut hasher);
    modified.hash(&mut hasher);
    hasher.finish()
}

fn paths_signature(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> u64 {
    let mut hasher = DefaultHasher::new();
    for path in paths {
        let path: PathBuf = path.as_ref().to_path_buf();
        path.hash(&mut hasher);
        file_signature(&path).hash(&mut hasher);
    }
    hasher.finish()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_guide_states_cover_existing_missing_and_invalid_utf8() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("DOCUMENTATION.md");
        assert!(matches!(
            load_document_file(&path),
            DocumentLoad::Missing { .. }
        ));

        std::fs::write(&path, "# Project guide\n\nSafe **Markdown**.").unwrap();
        let DocumentLoad::Ready { source, .. } = load_document_file(&path) else {
            panic!("existing documentation should load");
        };
        assert!(source.contains("Project guide"));

        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(matches!(
            load_document_file(&path),
            DocumentLoad::Malformed { .. }
        ));
    }

    #[test]
    fn explicit_user_guide_reload_observes_source_changes() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("DOCUMENTATION.md");
        std::fs::write(&path, "# First").unwrap();
        let DocumentLoad::Ready {
            source: first,
            signature: first_signature,
        } = load_document_file(&path)
        else {
            panic!("first document should load");
        };
        std::fs::write(&path, "# Second and longer").unwrap();
        let DocumentLoad::Ready {
            source: second,
            signature: second_signature,
        } = load_document_file(&path)
        else {
            panic!("changed document should load");
        };
        assert_ne!(first, second);
        assert_ne!(first_signature, second_signature);
        assert_eq!(file_signature(&path), second_signature);
    }

    #[test]
    fn packaged_user_guide_is_bundled_and_global() {
        let DocumentLoad::Ready { source, .. } = load_user_guide() else {
            panic!("the bundled user guide should always load");
        };
        assert!(source.starts_with("# RuDS User Guide"));
        let rewritten = rewrite_links(&source);
        let parsed = parse_document(&source);
        let anchor_pattern =
            regex::Regex::new(r"https://ruds\.invalid/document#([^)\s]+)").unwrap();
        for captures in anchor_pattern.captures_iter(&rewritten) {
            assert!(
                parsed.anchors.contains_key(&captures[1]),
                "guide link points to missing heading anchor: {}",
                &captures[1]
            );
        }
        assert!(rewritten.contains(API_DOCUMENTATION_URL));
    }

    #[test]
    fn api_document_uses_generated_reference_types_and_examples() {
        let DocumentLoad::Ready { source, .. } = load_api_document() else {
            panic!("bundled API documentation should always load");
        };
        assert!(source.contains("# RuDS Lua API Reference"));
        assert!(source.contains("# Lua API Types"));
        assert!(source.contains("## Utility Lua example"));
        assert!(source.contains("function main(input)"));
        let types_offset = parse_document(&source).anchors["lua-api-types"];
        assert!((0.90..0.96).contains(&types_offset));
    }

    #[test]
    fn safe_document_parser_keeps_tables_and_navigation_but_never_active_content() {
        let source = "# Start\n\n[Jump](#details) [API](API.md)\n\n| Name | Value |\n| --- | --- |\n| one | `two` |\n\n## Details\n\n<script>alert(1)</script>\n<style>bad</style>";
        let rewritten = rewrite_links(source);
        assert!(rewritten.contains("https://ruds.invalid/document#details"));
        assert!(rewritten.contains(API_DOCUMENTATION_URL));
        let parsed = parse_document(source);
        assert!(parsed.blocks.iter().any(|block| matches!(
            block,
            DocumentBlock::Table { headers, rows }
                if headers == &["Name", "Value"]
                    && rows == &[vec![String::from("one"), String::from("two")]]
        )));
        assert!(parsed.anchors.contains_key("details"));
        let debug = format!("{:?}", parsed.blocks);
        assert!(debug.contains("Heading"));
        assert!(!debug.contains("<script>"));
        assert!(!debug.contains("<style>"));
    }

    #[test]
    fn tables_keep_union_type_cells_and_fenced_table_text_stays_code() {
        let parsed = parse_document(
            "| Type | Meaning |\n| --- | --- |\n| `string | nil` | optional |\n\n```md\n| not | a table |\n| --- | --- |\n```",
        );
        assert!(matches!(
            &parsed.blocks[0],
            DocumentBlock::Table { rows, .. }
                if rows == &[vec![String::from("string | nil"), String::from("optional")]]
        ));
        assert!(matches!(
            &parsed.blocks[1],
            DocumentBlock::Markdown(items)
                if format!("{items:?}").contains("CodeBlock")
                    && format!("{items:?}").contains("not | a table")
        ));
    }
}
