use std::collections::HashSet;

use bds_core::i18n::UiLocale;
use bds_core::model::{
    ImportCandidate, ImportDefinition, ImportExecutionResult, ImportItemKind, ImportItemStatus,
    ImportPhase, ImportProgress, ImportReport, ImportResolution, TaxonomyKind,
};
use iced::widget::{Space, button, column, container, progress_bar, row, scrollable, text};
use iced::{Alignment, Color, Element, Length};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};

#[derive(Debug, Clone)]
pub struct ImportEditorState {
    pub definition: ImportDefinition,
    pub report: Option<ImportReport>,
    pub category_options: Vec<String>,
    pub tag_options: Vec<String>,
    pub expanded: HashSet<ImportSection>,
    pub is_analyzing: bool,
    pub is_executing: bool,
    pub progress: Option<ImportProgress>,
    pub result: Option<ImportExecutionResult>,
    pub error: Option<String>,
}

impl ImportEditorState {
    pub fn new(
        definition: ImportDefinition,
        category_options: Vec<String>,
        tag_options: Vec<String>,
    ) -> Self {
        let report = definition.analysis().ok().flatten();
        Self {
            definition,
            report,
            category_options,
            tag_options,
            expanded: HashSet::from([
                ImportSection::Conflicts,
                ImportSection::Taxonomy,
                ImportSection::Posts,
            ]),
            is_analyzing: false,
            is_executing: false,
            progress: None,
            result: None,
            error: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportSection {
    Conflicts,
    Posts,
    Pages,
    Media,
    Taxonomy,
    Macros,
}

#[derive(Debug, Clone)]
pub enum ImportEditorMsg {
    NameChanged(String),
    PickUploads,
    PickWxr,
    Analyze,
    Execute,
    AutoMapTaxonomy,
    DeleteRequested,
    SetResolution {
        kind: ImportItemKind,
        identity: String,
        resolution: ImportResolution,
    },
    SetTaxonomyMapping {
        kind: TaxonomyKind,
        source: String,
        target: Option<String>,
    },
    ToggleSection(ImportSection),
}

#[derive(Debug, Clone)]
pub enum ImportAnalysisEvent {
    Progress(ImportProgress),
    Finished(Box<Result<(ImportDefinition, ImportReport), String>>),
}

#[derive(Debug, Clone)]
pub enum ImportExecutionEvent {
    Progress(ImportProgress),
    Finished(Result<ImportExecutionResult, String>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ResolutionOption {
    value: ImportResolution,
    label: String,
}

impl std::fmt::Display for ResolutionOption {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.label)
    }
}

pub fn view<'a>(state: &'a ImportEditorState, locale: UiLocale) -> Element<'a, Message> {
    let busy = state.is_analyzing || state.is_executing;
    let name = inputs::labeled_input(
        &t(locale, "import.name"),
        &t(locale, "import.namePlaceholder"),
        &state.definition.name,
        |value| Message::ImportEditor(ImportEditorMsg::NameChanged(value)),
    );
    let header = inputs::card(
        column![
            row![
                name,
                button(text(t(locale, "modal.confirmDelete.delete")))
                    .on_press_maybe(
                        (!busy).then_some(Message::ImportEditor(ImportEditorMsg::DeleteRequested,))
                    )
                    .padding([8, 12])
                    .style(inputs::danger_button),
                button(text(t(locale, "import.analyze")))
                    .on_press_maybe(
                        (!busy && state.definition.wxr_file_path.is_some())
                            .then_some(Message::ImportEditor(ImportEditorMsg::Analyze),)
                    )
                    .padding([8, 12])
                    .style(inputs::primary_button),
            ]
            .spacing(12)
            .align_y(Alignment::End),
            text(t(locale, "import.description"))
                .size(12)
                .color(Color::from_rgb(0.60, 0.60, 0.65)),
        ]
        .spacing(10),
    );

    let uploads = path_row(
        locale,
        "import.uploadsFolder",
        state.definition.uploads_folder_path.as_deref(),
        "import.noFolder",
        ImportEditorMsg::PickUploads,
    );
    let wxr = path_row(
        locale,
        "import.wxrFile",
        state.definition.wxr_file_path.as_deref(),
        "import.noWxr",
        ImportEditorMsg::PickWxr,
    );
    let files = inputs::card(column![uploads, wxr].spacing(12));

    let mut content: Vec<Element<'a, Message>> = vec![header.into(), files.into()];
    if busy || state.progress.is_some() {
        content.push(progress_view(state, locale));
    }
    if let Some(error) = &state.error {
        content.push(
            inputs::card(
                text(tw(locale, "import.failed", &[("error", error)]))
                    .size(13)
                    .color(Color::from_rgb(0.95, 0.38, 0.36)),
            )
            .into(),
        );
    }
    if let Some(result) = &state.result {
        content.push(result_view(result, locale));
    }

    if let Some(report) = &state.report {
        content.push(summary_view(report, locale));
        content.push(execute_toolbar(report, state, locale));

        let conflicts = report
            .posts
            .iter()
            .chain(&report.pages)
            .chain(&report.media)
            .filter(|item| item.status == ImportItemStatus::Conflict)
            .collect::<Vec<_>>();
        if !conflicts.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Conflicts,
                "import.conflicts",
                conflict_rows(&conflicts, locale),
            ));
        }
        if !report.posts.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Posts,
                "import.posts",
                candidate_rows(&report.posts, locale),
            ));
        }
        if !report.pages.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Pages,
                "import.pages",
                candidate_rows(&report.pages, locale),
            ));
        }
        if !report.media.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Media,
                "import.media",
                candidate_rows(&report.media, locale),
            ));
        }
        if !report.taxonomies.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Taxonomy,
                "import.taxonomy",
                taxonomy_rows(state, report, locale),
            ));
        }
        if !report.macros.is_empty() {
            content.extend(section(
                state,
                locale,
                ImportSection::Macros,
                "import.macros",
                macro_rows(report, locale),
            ));
        }
    } else if !state.is_analyzing {
        content.push(
            inputs::card(
                text(t(locale, "import.empty"))
                    .size(13)
                    .color(Color::from_rgb(0.60, 0.60, 0.65)),
            )
            .into(),
        );
    }

    scrollable(
        iced::widget::Column::with_children(content)
            .spacing(10)
            .padding(16)
            .width(Length::Fill),
    )
    .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
    .style(inputs::scrollable_style)
    .height(Length::Fill)
    .into()
}

fn path_row<'a>(
    locale: UiLocale,
    label_key: &str,
    path: Option<&str>,
    empty_key: &str,
    action: ImportEditorMsg,
) -> Element<'a, Message> {
    row![
        column![
            text(t(locale, label_key))
                .size(12)
                .color(inputs::LABEL_COLOR),
            text(
                path.map(str::to_string)
                    .unwrap_or_else(|| t(locale, empty_key))
            )
            .size(13)
            .color(if path.is_some() {
                Color::from_rgb(0.85, 0.85, 0.88)
            } else {
                Color::from_rgb(0.50, 0.50, 0.55)
            }),
        ]
        .spacing(5)
        .width(Length::Fill),
        button(text(t(locale, "common.open")))
            .on_press(Message::ImportEditor(action))
            .padding([7, 12])
            .style(inputs::secondary_button),
    ]
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn progress_view<'a>(state: &ImportEditorState, locale: UiLocale) -> Element<'a, Message> {
    let (phase, current, total, detail, eta) = state.progress.as_ref().map_or_else(
        || {
            (
                if state.is_analyzing {
                    t(locale, "import.phase.analysis")
                } else {
                    t(locale, "import.phase.starting")
                },
                0,
                1,
                String::new(),
                None,
            )
        },
        |progress| {
            (
                phase_label(progress.phase, locale),
                progress.current,
                progress.total.max(1),
                progress.detail.clone(),
                progress.eta_ms,
            )
        },
    );
    let ratio = current as f32 / total as f32;
    let eta = eta
        .map(|milliseconds| {
            tw(
                locale,
                "import.eta",
                &[("seconds", &(milliseconds / 1_000).to_string())],
            )
        })
        .unwrap_or_default();
    inputs::card(
        column![
            row![
                text(phase).size(13),
                Space::with_width(Length::Fill),
                text(format!("{current} / {total}"))
                    .size(12)
                    .color(inputs::LABEL_COLOR),
            ],
            progress_bar(0.0..=1.0, ratio.clamp(0.0, 1.0)),
            text(format!("{detail} {eta}"))
                .size(12)
                .color(Color::from_rgb(0.60, 0.60, 0.65)),
        ]
        .spacing(8),
    )
    .into()
}

fn summary_view<'a>(report: &'a ImportReport, locale: UiLocale) -> Element<'a, Message> {
    let source = std::path::Path::new(&report.source_file)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(&report.source_file);
    let counts = row![
        stat(locale, "import.posts", &report.post_counts),
        stat(locale, "import.pages", &report.page_counts),
        stat(locale, "import.media", &report.media_counts),
        column![
            text(t(locale, "import.taxonomy"))
                .size(12)
                .color(inputs::LABEL_COLOR),
            text(report.taxonomies.len().to_string()).size(22),
        ]
        .spacing(4)
        .width(Length::Fill),
    ]
    .spacing(14);
    let dates = report
        .date_distribution
        .iter()
        .map(|bucket| {
            format!(
                "{}: {} / {}",
                bucket.year, bucket.post_count, bucket.media_count
            )
        })
        .collect::<Vec<_>>()
        .join(" · ");
    inputs::card(
        column![
            text(&report.site.title).size(20),
            text(format!(
                "{} · {} · {}",
                report.site.url.as_deref().unwrap_or("—"),
                report.site.language.as_deref().unwrap_or("—"),
                source
            ))
            .size(12)
            .color(Color::from_rgb(0.60, 0.60, 0.65)),
            counts,
            text(tw(locale, "import.dateDistribution", &[("dates", &dates)]))
                .size(12)
                .color(inputs::LABEL_COLOR),
        ]
        .spacing(10),
    )
    .into()
}

fn stat<'a>(
    locale: UiLocale,
    label_key: &str,
    counts: &bds_core::model::ImportCounts,
) -> Element<'a, Message> {
    column![
        text(t(locale, label_key))
            .size(12)
            .color(inputs::LABEL_COLOR),
        text(
            (counts.new_count
                + counts.update_count
                + counts.conflict_count
                + counts.duplicate_count
                + counts.missing_count)
                .to_string()
        )
        .size(22),
        text(format!(
            "{} {} · {} {} · {} {} · {} {} · {} {}",
            counts.new_count,
            t(locale, "import.status.new"),
            counts.update_count,
            t(locale, "import.status.update"),
            counts.conflict_count,
            t(locale, "import.status.conflict"),
            counts.duplicate_count,
            t(locale, "import.status.duplicate"),
            counts.missing_count,
            t(locale, "import.status.missing"),
        ))
        .size(10)
        .color(Color::from_rgb(0.58, 0.58, 0.62)),
    ]
    .spacing(4)
    .width(Length::Fill)
    .into()
}

fn execute_toolbar<'a>(
    report: &ImportReport,
    state: &ImportEditorState,
    locale: UiLocale,
) -> Element<'a, Message> {
    let count = report.importable_count();
    inputs::toolbar(
        vec![
            text(tw(locale, "import.ready", &[("count", &count.to_string())]))
                .size(13)
                .into(),
        ],
        vec![
            button(text(t(locale, "import.autoMap")))
                .on_press_maybe(
                    (!state.is_analyzing && !state.is_executing)
                        .then_some(Message::ImportEditor(ImportEditorMsg::AutoMapTaxonomy)),
                )
                .padding([8, 12])
                .style(inputs::secondary_button)
                .into(),
            button(text(tw(
                locale,
                "import.execute",
                &[("count", &count.to_string())],
            )))
            .on_press_maybe(
                (count > 0 && !state.is_analyzing && !state.is_executing)
                    .then_some(Message::ImportEditor(ImportEditorMsg::Execute)),
            )
            .padding([8, 12])
            .style(inputs::primary_button)
            .into(),
        ],
    )
}

fn result_view<'a>(result: &ImportExecutionResult, locale: UiLocale) -> Element<'a, Message> {
    let imported = result.taxonomy.imported
        + result.posts.imported
        + result.media.imported
        + result.pages.imported;
    let skipped = result.taxonomy.skipped
        + result.posts.skipped
        + result.media.skipped
        + result.pages.skipped;
    inputs::card(
        text(tw(
            locale,
            "import.complete",
            &[
                ("imported", &imported.to_string()),
                ("skipped", &skipped.to_string()),
            ],
        ))
        .size(13)
        .color(Color::from_rgb(0.45, 0.80, 0.50)),
    )
    .into()
}

fn section<'a>(
    state: &ImportEditorState,
    locale: UiLocale,
    section: ImportSection,
    title_key: &str,
    body: Element<'a, Message>,
) -> Vec<Element<'a, Message>> {
    let expanded = state.expanded.contains(&section);
    let header = inputs::card(
        button(
            row![
                text(if expanded { "▾" } else { "▸" }).size(12),
                text(t(locale, title_key)).size(13),
            ]
            .spacing(8),
        )
        .on_press(Message::ImportEditor(ImportEditorMsg::ToggleSection(
            section,
        )))
        .padding([6, 8])
        .width(Length::Fill)
        .style(inputs::disclosure_button),
    )
    .padding(6)
    .into();
    if expanded {
        vec![header, inputs::card(body).into()]
    } else {
        vec![header]
    }
}

fn conflict_rows<'a>(conflicts: &[&'a ImportCandidate], locale: UiLocale) -> Element<'a, Message> {
    let options = vec![
        ResolutionOption {
            value: ImportResolution::Ignore,
            label: t(locale, "import.resolution.ignore"),
        },
        ResolutionOption {
            value: ImportResolution::Overwrite,
            label: t(locale, "import.resolution.overwrite"),
        },
        ResolutionOption {
            value: ImportResolution::Import,
            label: t(locale, "import.resolution.import"),
        },
    ];
    let rows = conflicts
        .iter()
        .map(|item| {
            let identity = item
                .slug
                .as_deref()
                .or(item.filename.as_deref())
                .unwrap_or("")
                .to_string();
            let identity_label = identity.clone();
            let selected = options
                .iter()
                .find(|option| Some(option.value) == item.resolution);
            let kind = item.kind;
            row![
                column![
                    text(identity_label).size(13),
                    text(&item.title).size(11).color(inputs::LABEL_COLOR)
                ]
                .spacing(3)
                .width(Length::Fill),
                container(inputs::labeled_select(
                    &t(locale, "import.resolution"),
                    &options,
                    selected,
                    move |option| Message::ImportEditor(ImportEditorMsg::SetResolution {
                        kind,
                        identity: identity.clone(),
                        resolution: option.value,
                    }),
                ))
                .width(Length::Fixed(240.0)),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        })
        .collect::<Vec<_>>();
    iced::widget::Column::with_children(rows).spacing(10).into()
}

fn candidate_rows<'a>(items: &'a [ImportCandidate], locale: UiLocale) -> Element<'a, Message> {
    let rows = items
        .iter()
        .map(|item| {
            let identity = item
                .slug
                .as_deref()
                .or(item.filename.as_deref())
                .unwrap_or("—");
            let detail = match item.kind {
                ImportItemKind::Media => item.relative_path.as_deref().unwrap_or("—").to_string(),
                _ => format!(
                    "{} · {} · {}",
                    item.source_status.as_deref().unwrap_or("—"),
                    item.author.as_deref().unwrap_or("—"),
                    item.categories.join(", ")
                ),
            };
            row![
                column![
                    text(&item.title).size(13),
                    text(identity).size(11).color(inputs::LABEL_COLOR)
                ]
                .spacing(3)
                .width(Length::FillPortion(2)),
                text(detail)
                    .size(11)
                    .color(Color::from_rgb(0.62, 0.62, 0.66))
                    .width(Length::FillPortion(3)),
                text(status_label(item.status, locale)).size(11),
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        })
        .collect::<Vec<_>>();
    iced::widget::Column::with_children(rows).spacing(10).into()
}

fn taxonomy_rows<'a>(
    state: &'a ImportEditorState,
    report: &'a ImportReport,
    locale: UiLocale,
) -> Element<'a, Message> {
    let rows = report
        .taxonomies
        .iter()
        .map(|item| {
            let options = match item.kind {
                TaxonomyKind::Category => &state.category_options,
                TaxonomyKind::Tag => &state.tag_options,
            };
            let source = item.name.clone();
            let kind = item.kind;
            let status = if item.exists_in_project {
                t(locale, "import.taxonomyExisting")
            } else if item.mapped_to.is_some() {
                t(locale, "import.taxonomyMapped")
            } else {
                t(locale, "import.taxonomyNew")
            };
            let select: Element<'a, Message> = if item.exists_in_project {
                text("—").size(12).into()
            } else {
                let selected = item
                    .mapped_to
                    .as_ref()
                    .and_then(|mapped| options.iter().find(|option| *option == mapped));
                row![
                    container(inputs::labeled_select(
                        &t(locale, "import.mapTo"),
                        options,
                        selected,
                        move |target| Message::ImportEditor(ImportEditorMsg::SetTaxonomyMapping {
                            kind,
                            source: source.clone(),
                            target: Some(target),
                        }),
                    ))
                    .width(Length::Fixed(260.0)),
                    button(text(t(locale, "common.clear")))
                        .on_press_maybe(item.mapped_to.is_some().then_some(Message::ImportEditor(
                            ImportEditorMsg::SetTaxonomyMapping {
                                kind,
                                source: item.name.clone(),
                                target: None,
                            },
                        )))
                        .padding([7, 10])
                        .style(inputs::secondary_button),
                ]
                .spacing(8)
                .align_y(Alignment::End)
                .into()
            };
            row![
                column![
                    text(&item.name).size(13),
                    text(match item.kind {
                        TaxonomyKind::Category => t(locale, "import.category"),
                        TaxonomyKind::Tag => t(locale, "import.tag"),
                    })
                    .size(11)
                    .color(inputs::LABEL_COLOR),
                ]
                .spacing(3)
                .width(Length::Fill),
                text(status).size(11).width(Length::Fixed(90.0)),
                select,
            ]
            .spacing(12)
            .align_y(Alignment::Center)
            .into()
        })
        .collect::<Vec<_>>();
    iced::widget::Column::with_children(rows).spacing(10).into()
}

fn macro_rows<'a>(report: &'a ImportReport, locale: UiLocale) -> Element<'a, Message> {
    let rows = report
        .macros
        .iter()
        .map(|usage| {
            let parameters = usage
                .parameters
                .iter()
                .map(|values| {
                    values
                        .iter()
                        .map(|(key, value)| format!("{key}={value}"))
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .collect::<Vec<_>>()
                .join(" · ");
            column![
                row![
                    text(format!("[[{}]]", usage.name)).size(13),
                    Space::with_width(Length::Fill),
                    text(tw(
                        locale,
                        "import.macroUses",
                        &[("count", &usage.total_count.to_string())],
                    ))
                    .size(11)
                    .color(inputs::LABEL_COLOR),
                ],
                text(parameters).size(11).color(inputs::LABEL_COLOR),
                text(usage.post_slugs.join(", "))
                    .size(11)
                    .color(Color::from_rgb(0.58, 0.58, 0.62)),
            ]
            .spacing(4)
            .into()
        })
        .collect::<Vec<_>>();
    iced::widget::Column::with_children(rows).spacing(10).into()
}

fn status_label(status: ImportItemStatus, locale: UiLocale) -> String {
    t(
        locale,
        match status {
            ImportItemStatus::New => "import.status.new",
            ImportItemStatus::Update => "import.status.update",
            ImportItemStatus::Conflict => "import.status.conflict",
            ImportItemStatus::ContentDuplicate => "import.status.duplicate",
            ImportItemStatus::Missing => "import.status.missing",
        },
    )
}

fn phase_label(phase: ImportPhase, locale: UiLocale) -> String {
    t(
        locale,
        match phase {
            ImportPhase::Taxonomy => "import.phase.taxonomy",
            ImportPhase::Posts => "import.phase.posts",
            ImportPhase::Media => "import.phase.media",
            ImportPhase::Pages => "import.phase.pages",
            ImportPhase::Complete => "import.phase.complete",
        },
    )
}
