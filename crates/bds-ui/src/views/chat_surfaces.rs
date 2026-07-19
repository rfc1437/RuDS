use std::collections::{HashMap, HashSet};
use std::f32::consts::{FRAC_PI_2, TAU};

use bds_core::engine::chat_surfaces::{
    ChartSeries, ChartType, ChatSurfaceState, FormInputType, InlineSurface, MindmapNode,
    SurfaceKind,
};
use bds_core::i18n::UiLocale;
use iced::widget::canvas::{self, Path, Stroke, path};
use iced::widget::{
    Space, button, checkbox, column, container, row, scrollable, text, text_editor,
};
use iced::{
    Alignment, Color, Element, Length, Point, Radians, Rectangle, Renderer, Size, Theme, mouse,
};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::t;
use crate::views::chat_view::textarea_key;

const CHART_COLORS: [Color; 7] = [
    rgb8(0x4E, 0xA1, 0xE0),
    rgb8(0x7B, 0xC9, 0x6F),
    rgb8(0xF0, 0xB4, 0x4D),
    rgb8(0xD9, 0x78, 0xAE),
    rgb8(0x9B, 0x8A, 0xE6),
    rgb8(0x54, 0xC6, 0xC0),
    rgb8(0xE3, 0x73, 0x63),
];

const fn rgb8(red: u8, green: u8, blue: u8) -> Color {
    Color {
        r: red as f32 / 255.0,
        g: green as f32 / 255.0,
        b: blue as f32 / 255.0,
        a: 1.0,
    }
}

pub fn view<'a>(
    surface: &'a InlineSurface,
    state: &'a ChatSurfaceState,
    textareas: &'a HashMap<String, text_editor::Content>,
    locale: UiLocale,
) -> Element<'a, Message> {
    surface_view(surface, state, textareas, locale, true)
}

fn surface_view<'a>(
    surface: &'a InlineSurface,
    state: &'a ChatSurfaceState,
    textareas: &'a HashMap<String, text_editor::Content>,
    locale: UiLocale,
    dismissible: bool,
) -> Element<'a, Message> {
    let has_title = surface.title.is_some();
    let title = surface.title.clone().unwrap_or_else(|| {
        t(
            locale,
            &format!("chat.surface.type.{}", surface.kind.as_str()),
        )
    });
    let mut header = row![text(title).size(14), Space::with_width(Length::Fill),]
        .spacing(8)
        .align_y(Alignment::Center);
    if has_title {
        header = header.push(
            text(t(
                locale,
                &format!("chat.surface.type.{}", surface.kind.as_str()),
            ))
            .size(10)
            .color(inputs::SECTION_COLOR),
        );
    }
    if dismissible {
        header = header.push(
            button(text(t(locale, "chat.surface.dismiss")))
                .on_press(Message::ChatSurfaceDismissed(surface.id.clone()))
                .padding([4, 8])
                .style(inputs::secondary_button),
        );
    }

    let content = surface_content(surface, state, textareas, locale);
    inputs::card(column![header, content].spacing(10)).into()
}

fn surface_content<'a>(
    surface: &'a InlineSurface,
    state: &'a ChatSurfaceState,
    textareas: &'a HashMap<String, text_editor::Content>,
    locale: UiLocale,
) -> Element<'a, Message> {
    match surface.kind {
        SurfaceKind::Card => {
            let mut children = Vec::new();
            if let Some(subtitle) = &surface.subtitle {
                children.push(
                    text(subtitle.clone())
                        .size(12)
                        .color(inputs::SECTION_COLOR)
                        .into(),
                );
            }
            if let Some(body) = &surface.body {
                children.push(text(body.clone()).size(13).into());
            }
            if !surface.actions.is_empty() {
                let actions = surface
                    .actions
                    .iter()
                    .fold(row![].spacing(8), |actions, item| {
                        actions.push(
                            button(text(item.label.clone()))
                                .on_press(Message::ChatSurfaceAction {
                                    surface_id: surface.id.clone(),
                                    action: item.action.clone(),
                                    payload: item.payload.clone(),
                                })
                                .padding([7, 10])
                                .style(inputs::secondary_button),
                        )
                    });
                children.push(actions.into());
            }
            empty_or_column(children, locale)
        }
        SurfaceKind::Chart => chart(surface),
        SurfaceKind::Form => form(surface, textareas, locale),
        SurfaceKind::List => {
            let children = surface
                .items
                .iter()
                .map(|item| text(format!("• {item}")).size(13).into())
                .collect();
            empty_or_column(children, locale)
        }
        SurfaceKind::Metric => column![
            text(surface.label.clone().unwrap_or_default())
                .size(12)
                .color(inputs::SECTION_COLOR),
            text(surface.value.clone().unwrap_or_default()).size(28),
        ]
        .spacing(4)
        .into(),
        SurfaceKind::Mindmap => mindmap(surface, locale),
        SurfaceKind::Table => table(surface, locale),
        SurfaceKind::Tabs => tabs(surface, state, textareas, locale),
        SurfaceKind::Text => text(surface.body.clone().unwrap_or_default())
            .size(13)
            .into(),
        SurfaceKind::Json => text(
            surface
                .raw
                .as_ref()
                .and_then(|value| serde_json::to_string_pretty(value).ok())
                .unwrap_or_default(),
        )
        .size(12)
        .shaping(iced::widget::text::Shaping::Advanced)
        .into(),
    }
}

fn empty_or_column<'a>(
    mut children: Vec<Element<'a, Message>>,
    locale: UiLocale,
) -> Element<'a, Message> {
    if children.is_empty() {
        children.push(
            text(t(locale, "chat.surface.empty"))
                .size(12)
                .color(inputs::SECTION_COLOR)
                .into(),
        );
    }
    iced::widget::Column::with_children(children)
        .spacing(7)
        .into()
}

fn form<'a>(
    surface: &'a InlineSurface,
    textareas: &'a HashMap<String, text_editor::Content>,
    locale: UiLocale,
) -> Element<'a, Message> {
    let mut children: Vec<Element<'a, Message>> = Vec::new();
    for field in &surface.fields {
        let label = if field.required {
            format!("{} *", field.label)
        } else {
            field.label.clone()
        };
        let surface_id = surface.id.clone();
        let key = field.key.clone();
        let control: Element<'a, Message> = match field.input_type {
            FormInputType::Checkbox => checkbox(label, field.value.as_bool().unwrap_or(false))
                .on_toggle(move |value| Message::ChatSurfaceFieldChanged {
                    surface_id: surface_id.clone(),
                    field: key.clone(),
                    value: value.into(),
                })
                .size(16)
                .text_size(13)
                .into(),
            FormInputType::Select => {
                let selected = field.options.iter().find(|option| {
                    field
                        .value
                        .as_str()
                        .is_some_and(|value| value == option.value)
                });
                inputs::labeled_select(&label, &field.options, selected, move |option| {
                    Message::ChatSurfaceFieldChanged {
                        surface_id: surface_id.clone(),
                        field: key.clone(),
                        value: option.value.into(),
                    }
                })
            }
            FormInputType::Textarea => {
                let content_key = textarea_key(&surface.id, &field.key);
                if let Some(content) = textareas.get(&content_key) {
                    column![
                        text(label).size(12).color(inputs::LABEL_COLOR),
                        text_editor(content)
                            .placeholder(field.placeholder.clone().unwrap_or_default())
                            .on_action(move |action| Message::ChatSurfaceTextareaAction {
                                surface_id: surface_id.clone(),
                                field: key.clone(),
                                action,
                            })
                            .height(Length::Fixed(96.0))
                            .style(inputs::text_editor_style),
                    ]
                    .spacing(6)
                    .into()
                } else {
                    text(t(locale, "chat.surface.empty")).into()
                }
            }
            FormInputType::Text | FormInputType::Date | FormInputType::Number => {
                let value = match &field.value {
                    serde_json::Value::String(value) => value.clone(),
                    serde_json::Value::Number(value) => value.to_string(),
                    _ => String::new(),
                };
                let placeholder = field.placeholder.clone().unwrap_or_else(|| {
                    t(
                        locale,
                        match field.input_type {
                            FormInputType::Date => "chat.surface.form.datePlaceholder",
                            FormInputType::Number => "chat.surface.form.numberPlaceholder",
                            _ => "chat.surface.form.textPlaceholder",
                        },
                    )
                });
                inputs::labeled_input(&label, &placeholder, &value, move |value| {
                    let value = if field.input_type == FormInputType::Number {
                        value
                            .parse::<serde_json::Number>()
                            .map(serde_json::Value::Number)
                            .unwrap_or_else(|_| value.into())
                    } else {
                        value.into()
                    };
                    Message::ChatSurfaceFieldChanged {
                        surface_id: surface_id.clone(),
                        field: key.clone(),
                        value,
                    }
                })
            }
        };
        children.push(control);
    }
    if let Some(action) = &surface.submit_action {
        children.push(
            button(text(
                surface
                    .submit_label
                    .clone()
                    .unwrap_or_else(|| t(locale, "chat.surface.form.submit")),
            ))
            .on_press(Message::ChatSurfaceAction {
                surface_id: surface.id.clone(),
                action: action.clone(),
                payload: serde_json::json!({}),
            })
            .padding([8, 12])
            .style(inputs::primary_button)
            .into(),
        );
    }
    empty_or_column(children, locale)
}

fn table<'a>(surface: &'a InlineSurface, locale: UiLocale) -> Element<'a, Message> {
    if surface.columns.is_empty() && surface.rows.is_empty() {
        return text(t(locale, "chat.surface.empty"))
            .size(12)
            .color(inputs::SECTION_COLOR)
            .into();
    }
    let header = iced::widget::Row::with_children(
        surface
            .columns
            .iter()
            .map(|value| {
                container(text(value.clone()).size(12))
                    .width(Length::Fixed(140.0))
                    .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(8);
    let mut body: Vec<Element<'a, Message>> = vec![header.into()];
    for values in &surface.rows {
        body.push(
            iced::widget::Row::with_children(
                values
                    .iter()
                    .map(|value| {
                        container(text(value.clone()).size(12))
                            .width(Length::Fixed(140.0))
                            .into()
                    })
                    .collect::<Vec<_>>(),
            )
            .spacing(8)
            .into(),
        );
    }
    scrollable(iced::widget::Column::with_children(body).spacing(6))
        .direction(scrollable::Direction::Horizontal(
            inputs::compact_scrollbar(),
        ))
        .style(inputs::scrollable_style)
        .into()
}

fn mindmap<'a>(surface: &'a InlineSurface, locale: UiLocale) -> Element<'a, Message> {
    let rows = mindmap_rows(&surface.nodes);
    if rows.is_empty() {
        return text(t(locale, "chat.surface.empty"))
            .size(12)
            .color(inputs::SECTION_COLOR)
            .into();
    }
    iced::widget::Column::with_children(
        rows.into_iter()
            .map(|(depth, label)| {
                row![
                    Space::with_width(Length::Fixed(depth as f32 * 18.0)),
                    text(if depth == 0 { "◆" } else { "└" })
                        .size(11)
                        .color(inputs::SECTION_COLOR),
                    text(label).size(13),
                ]
                .spacing(6)
                .into()
            })
            .collect::<Vec<_>>(),
    )
    .spacing(6)
    .into()
}

fn mindmap_rows(nodes: &[MindmapNode]) -> Vec<(usize, String)> {
    let by_id = nodes
        .iter()
        .filter_map(|node| node.id.as_deref().map(|id| (id, node)))
        .collect::<HashMap<_, _>>();
    let children = nodes
        .iter()
        .flat_map(|node| node.children.iter().map(String::as_str))
        .collect::<HashSet<_>>();
    let roots = nodes
        .iter()
        .filter(|node| node.id.as_deref().is_none_or(|id| !children.contains(id)))
        .collect::<Vec<_>>();
    let mut rows = Vec::new();
    let mut visited = HashSet::new();
    fn walk(
        node: &MindmapNode,
        depth: usize,
        by_id: &HashMap<&str, &MindmapNode>,
        visited: &mut HashSet<String>,
        rows: &mut Vec<(usize, String)>,
    ) {
        if let Some(id) = &node.id
            && !visited.insert(id.clone())
        {
            return;
        }
        rows.push((depth, node.label.clone()));
        for child in &node.children {
            if let Some(node) = by_id.get(child.as_str()) {
                walk(node, depth + 1, by_id, visited, rows);
            }
        }
    }
    for root in roots {
        walk(root, 0, &by_id, &mut visited, &mut rows);
    }
    for node in nodes {
        let missing = node.id.as_ref().is_some_and(|id| !visited.contains(id));
        if missing {
            walk(node, 0, &by_id, &mut visited, &mut rows);
        }
    }
    rows
}

fn tabs<'a>(
    surface: &'a InlineSurface,
    state: &'a ChatSurfaceState,
    textareas: &'a HashMap<String, text_editor::Content>,
    locale: UiLocale,
) -> Element<'a, Message> {
    if surface.tabs.is_empty() {
        return text(t(locale, "chat.surface.empty")).into();
    }
    let selected = state
        .surface_tabs
        .get(&surface.id)
        .copied()
        .or(surface.selected_index)
        .unwrap_or(0)
        .min(surface.tabs.len() - 1);
    let controls =
        surface
            .tabs
            .iter()
            .enumerate()
            .fold(row![].spacing(6), |controls, (index, tab)| {
                controls.push(
                    button(text(tab.label.clone()))
                        .on_press(Message::ChatSurfaceTabSelected {
                            surface_id: surface.id.clone(),
                            index,
                        })
                        .padding([6, 10])
                        .style(if index == selected {
                            inputs::primary_button
                        } else {
                            inputs::secondary_button
                        }),
                )
            });
    let content = iced::widget::Column::with_children(
        surface.tabs[selected]
            .content
            .iter()
            .map(|child| surface_view(child, state, textareas, locale, false))
            .collect::<Vec<_>>(),
    )
    .spacing(8);
    column![controls, content].spacing(8).into()
}

fn chart<'a>(surface: &'a InlineSurface) -> Element<'a, Message> {
    let chart_type = surface.chart_type.unwrap_or(ChartType::Bar);
    let canvas = canvas::Canvas::new(NativeChart {
        chart_type,
        series: surface.series.clone(),
    })
    .width(Length::Fill)
    .height(Length::Fixed(180.0));
    let mut legend_items = Vec::new();
    if chart_type == ChartType::Heatmap {
        let (columns, rows) = heatmap_layout(&surface.series);
        for (row_label, values) in rows {
            for (column, value) in columns.iter().zip(values) {
                legend_items.push(format!(
                    "{row_label} / {column} — {}",
                    display_number(value)
                ));
            }
        }
    } else if chart_type == ChartType::StackedBar {
        for item in &surface.series {
            if item.segments.is_empty() {
                legend_items.push(format!("{} — {}", item.label, display_number(item.value)));
            } else {
                for segment in &item.segments {
                    legend_items.push(format!(
                        "{} / {} — {}",
                        item.label,
                        segment.label,
                        display_number(segment.value)
                    ));
                }
            }
        }
    } else {
        legend_items.extend(surface.series.iter().map(|item| {
            format!(
                "{} — {}",
                item.label,
                display_number(chart_series_total(chart_type, item))
            )
        }));
    }
    let legend = legend_items.into_iter().enumerate().fold(
        iced::widget::Column::new().spacing(4),
        |legend, (index, item)| {
            legend.push(
                text(item)
                    .size(11)
                    .color(CHART_COLORS[index % CHART_COLORS.len()]),
            )
        },
    );
    column![canvas, legend].spacing(8).into()
}

fn display_number(value: f64) -> String {
    if value.fract() == 0.0 {
        format!("{value:.0}")
    } else {
        format!("{value:.2}")
    }
}

fn chart_series_total(chart_type: ChartType, series: &ChartSeries) -> f64 {
    if chart_type == ChartType::StackedBar && !series.segments.is_empty() {
        series.segments.iter().map(|segment| segment.value).sum()
    } else {
        series.value
    }
}

#[derive(Debug, Clone)]
struct NativeChart {
    chart_type: ChartType,
    series: Vec<ChartSeries>,
}

impl<Message> canvas::Program<Message> for NativeChart {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<canvas::Geometry> {
        let mut frame = canvas::Frame::new(renderer, bounds.size());
        match self.chart_type {
            ChartType::Bar => draw_bars(&mut frame, &self.series, false),
            ChartType::StackedBar => draw_bars(&mut frame, &self.series, true),
            ChartType::Line => draw_line(&mut frame, &self.series, false),
            ChartType::Area => draw_line(&mut frame, &self.series, true),
            ChartType::Pie => draw_pie(&mut frame, &self.series, false),
            ChartType::Donut => draw_pie(&mut frame, &self.series, true),
            ChartType::Heatmap => draw_heatmap(&mut frame, &self.series),
        }
        vec![frame.into_geometry()]
    }
}

fn positive_max(series: &[ChartSeries], stacked: bool) -> f32 {
    series
        .iter()
        .map(|item| {
            if stacked && !item.segments.is_empty() {
                item.segments.iter().map(|part| part.value.max(0.0)).sum()
            } else {
                item.value.max(0.0)
            }
        })
        .fold(0.0_f64, f64::max)
        .max(1.0) as f32
}

fn draw_bars(frame: &mut canvas::Frame, series: &[ChartSeries], stacked: bool) {
    if series.is_empty() {
        return;
    }
    let size = frame.size();
    let gap = 8.0;
    let width = ((size.width - gap * (series.len() as f32 + 1.0)) / series.len() as f32).max(3.0);
    let max = positive_max(series, stacked);
    for (index, item) in series.iter().enumerate() {
        let x = gap + index as f32 * (width + gap);
        if stacked && !item.segments.is_empty() {
            let mut y = size.height - 8.0;
            for (part_index, part) in item.segments.iter().enumerate() {
                let height = (part.value.max(0.0) as f32 / max) * (size.height - 16.0);
                y -= height;
                frame.fill(
                    &Path::rectangle(Point::new(x, y), Size::new(width, height)),
                    CHART_COLORS[part_index % CHART_COLORS.len()],
                );
            }
        } else {
            let height = (item.value.max(0.0) as f32 / max) * (size.height - 16.0);
            frame.fill(
                &Path::rectangle(
                    Point::new(x, size.height - height - 8.0),
                    Size::new(width, height),
                ),
                CHART_COLORS[index % CHART_COLORS.len()],
            );
        }
    }
}

fn chart_points(size: Size, series: &[ChartSeries]) -> Vec<Point> {
    let max = positive_max(series, false);
    let denominator = series.len().saturating_sub(1).max(1) as f32;
    series
        .iter()
        .enumerate()
        .map(|(index, item)| {
            Point::new(
                8.0 + index as f32 / denominator * (size.width - 16.0),
                size.height - 8.0 - item.value.max(0.0) as f32 / max * (size.height - 16.0),
            )
        })
        .collect()
}

fn draw_line(frame: &mut canvas::Frame, series: &[ChartSeries], area: bool) {
    let points = chart_points(frame.size(), series);
    let Some(first) = points.first().copied() else {
        return;
    };
    let line = Path::new(|builder| {
        builder.move_to(first);
        for point in points.iter().skip(1) {
            builder.line_to(*point);
        }
    });
    if area {
        let size = frame.size();
        let fill = Path::new(|builder| {
            builder.move_to(Point::new(first.x, size.height - 8.0));
            builder.line_to(first);
            for point in points.iter().skip(1) {
                builder.line_to(*point);
            }
            builder.line_to(Point::new(
                points.last().map_or(first.x, |point| point.x),
                size.height - 8.0,
            ));
            builder.close();
        });
        frame.fill(&fill, CHART_COLORS[0].scale_alpha(0.28));
    }
    frame.stroke(
        &line,
        Stroke::default()
            .with_color(CHART_COLORS[0])
            .with_width(2.0)
            .with_line_cap(canvas::LineCap::Round),
    );
    for point in points {
        frame.fill(&Path::circle(point, 3.0), CHART_COLORS[0]);
    }
}

fn draw_pie(frame: &mut canvas::Frame, series: &[ChartSeries], donut: bool) {
    let values = series
        .iter()
        .map(|item| item.value.max(0.0) as f32)
        .collect::<Vec<_>>();
    let total = values.iter().sum::<f32>();
    if total <= 0.0 {
        return;
    }
    let center = frame.center();
    let radius = (frame.width().min(frame.height()) / 2.0 - 8.0).max(1.0);
    let mut start = -FRAC_PI_2;
    for (index, value) in values.into_iter().enumerate() {
        let end = start + value / total * TAU;
        let wedge = Path::new(|builder| {
            builder.move_to(center);
            builder.line_to(Point::new(
                center.x + radius * start.cos(),
                center.y + radius * start.sin(),
            ));
            builder.arc(path::Arc {
                center,
                radius,
                start_angle: Radians(start),
                end_angle: Radians(end),
            });
            builder.close();
        });
        frame.fill(&wedge, CHART_COLORS[index % CHART_COLORS.len()]);
        start = end;
    }
    if donut {
        frame.fill(
            &Path::circle(center, radius * 0.54),
            Color::from_rgb8(0x25, 0x25, 0x26),
        );
    }
}

fn draw_heatmap(frame: &mut canvas::Frame, series: &[ChartSeries]) {
    let (columns, rows) = heatmap_layout(series);
    if columns.is_empty() || rows.is_empty() {
        return;
    }
    let cell_width = frame.width() / columns.len() as f32;
    let cell_height = frame.height() / rows.len() as f32;
    let max = rows
        .iter()
        .flat_map(|(_, values)| values)
        .copied()
        .fold(0.0_f64, f64::max)
        .max(1.0);
    for (row_index, (_, values)) in rows.iter().enumerate() {
        for (column_index, value) in values.iter().enumerate() {
            let alpha = if *value <= 0.0 {
                0.08
            } else {
                0.18 + 0.82 * *value as f32 / max as f32
            };
            frame.fill(
                &Path::rectangle(
                    Point::new(
                        column_index as f32 * cell_width + 2.0,
                        row_index as f32 * cell_height + 2.0,
                    ),
                    Size::new(cell_width - 4.0, cell_height - 4.0),
                ),
                CHART_COLORS[0].scale_alpha(alpha),
            );
        }
    }
}

fn heatmap_layout(series: &[ChartSeries]) -> (Vec<String>, Vec<(String, Vec<f64>)>) {
    let mut columns = Vec::new();
    for item in series {
        for segment in &item.segments {
            if !columns.contains(&segment.label) {
                columns.push(segment.label.clone());
            }
        }
    }
    let rows = series
        .iter()
        .filter(|item| !item.segments.is_empty())
        .map(|item| {
            let values = columns
                .iter()
                .map(|column| {
                    item.segments
                        .iter()
                        .find(|segment| segment.label == *column)
                        .map_or(0.0, |segment| segment.value)
                })
                .collect();
            (item.label.clone(), values)
        })
        .collect();
    (columns, rows)
}

#[cfg(test)]
mod tests {
    use super::*;
    use bds_core::engine::chat_surfaces::build_render_surface;

    #[test]
    fn chart_math_handles_every_fixed_chart_type() {
        let series = vec![ChartSeries {
            label: "A".into(),
            value: 3.0,
            segments: vec![],
        }];
        for chart_type in [
            ChartType::Bar,
            ChartType::StackedBar,
            ChartType::Line,
            ChartType::Area,
            ChartType::Pie,
            ChartType::Donut,
            ChartType::Heatmap,
        ] {
            assert_eq!(chart_series_total(chart_type, &series[0]), 3.0);
        }
        assert_eq!(chart_points(Size::new(100.0, 100.0), &series).len(), 1);
    }

    #[test]
    fn heatmap_uses_labelled_segments_and_zero_fills_missing_cells() {
        let series = vec![
            ChartSeries {
                label: "First".into(),
                value: 0.0,
                segments: vec![
                    bds_core::engine::chat_surfaces::ChartSegment {
                        label: "A".into(),
                        value: 2.0,
                    },
                    bds_core::engine::chat_surfaces::ChartSegment {
                        label: "B".into(),
                        value: 4.0,
                    },
                ],
            },
            ChartSeries {
                label: "Second".into(),
                value: 0.0,
                segments: vec![bds_core::engine::chat_surfaces::ChartSegment {
                    label: "B".into(),
                    value: 3.0,
                }],
            },
        ];
        let (columns, rows) = heatmap_layout(&series);
        assert_eq!(columns, ["A", "B"]);
        assert_eq!(rows[0], ("First".into(), vec![2.0, 4.0]));
        assert_eq!(rows[1], ("Second".into(), vec![0.0, 3.0]));
    }

    #[test]
    fn mindmap_is_cycle_safe_and_keeps_orphans() {
        let nodes = vec![
            MindmapNode {
                id: Some("a".into()),
                label: "A".into(),
                children: vec!["b".into()],
            },
            MindmapNode {
                id: Some("b".into()),
                label: "B".into(),
                children: vec!["a".into()],
            },
            MindmapNode {
                id: None,
                label: "Loose".into(),
                children: vec![],
            },
        ];
        let rows = mindmap_rows(&nodes);
        assert_eq!(rows.len(), 3);
        assert!(rows.iter().any(|(_, label)| label == "Loose"));
    }

    #[test]
    fn every_surface_chart_and_form_control_builds_a_native_widget() {
        let state = ChatSurfaceState::default();
        let form = serde_json::json!({
            "fields": [
                {"key": "text", "label": "Text", "inputType": "text"},
                {"key": "notes", "label": "Notes", "inputType": "textarea"},
                {"key": "choice", "label": "Choice", "inputType": "select", "options": [{"label": "A", "value": "a"}]},
                {"key": "enabled", "label": "Enabled", "inputType": "checkbox"},
                {"key": "date", "label": "Date", "inputType": "date"},
                {"key": "count", "label": "Count", "inputType": "number"}
            ],
            "submitAction": "switchView"
        });
        let cases = [
            (
                "render_card",
                serde_json::json!({"body": "<script>plain text</script>"}),
            ),
            ("render_form", form),
            ("render_list", serde_json::json!({"items": ["One"]})),
            (
                "render_metric",
                serde_json::json!({"label": "Count", "value": 2}),
            ),
            (
                "render_mindmap",
                serde_json::json!({"nodes": [{"id": "a", "label": "A"}]}),
            ),
            (
                "render_table",
                serde_json::json!({"columns": ["A"], "rows": [["B"]]}),
            ),
            (
                "render_tabs",
                serde_json::json!({"tabs": [{"label": "Mixed", "content": ["plain", {"type": "future", "html": "<b>data</b>"}]}]}),
            ),
        ];
        for (index, (name, arguments)) in cases.into_iter().enumerate() {
            let surface =
                build_render_surface(name, &arguments, format!("surface-{index}"), &state).unwrap();
            let mut textareas = HashMap::new();
            if name == "render_form" {
                textareas.insert(
                    textarea_key(&surface.id, "notes"),
                    text_editor::Content::new(),
                );
            }
            let _: Element<'_, Message> = view(&surface, &state, &textareas, UiLocale::En);
        }
        for (index, chart_type) in [
            "bar",
            "stacked-bar",
            "line",
            "area",
            "pie",
            "donut",
            "heatmap",
        ]
        .into_iter()
        .enumerate()
        {
            let surface = build_render_surface(
                "render_chart",
                &serde_json::json!({
                    "chartType": chart_type,
                    "series": [{"label": "A", "value": 2, "segments": [{"label": "X", "value": 2}]}]
                }),
                format!("chart-{index}"),
                &state,
            )
            .unwrap();
            let textareas = HashMap::new();
            let _: Element<'_, Message> = view(&surface, &state, &textareas, UiLocale::En);
        }
    }
}
