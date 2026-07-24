use std::sync::{Arc, Mutex};

use bds_core::engine::git::{
    ChangedFile, GitCommit, GitFileDiff, GitFileStatus, GitOutput, GitRemoteState, GitRepository,
    ReconcileEvent, SyncStatus,
};
use bds_core::i18n::UiLocale;
use iced::widget::text::{Shaping, Wrapping};
use iced::widget::{Space, button, column, container, row, scrollable, text, text_input};
use iced::{Alignment, Background, Color, Element, Font, Length};

use crate::app::Message;
use crate::components::inputs;
use crate::i18n::{t, tw};
use crate::state::tabs::{Tab, TabType};

#[derive(Debug, Clone)]
pub struct GitSnapshot {
    pub repository: GitRepository,
    pub files: Vec<GitFileStatus>,
    pub history: Vec<GitCommit>,
    pub remote: GitRemoteState,
}

#[derive(Debug, Clone)]
pub struct GitNetworkCompletion {
    pub snapshot: GitSnapshot,
    pub events: Vec<ReconcileEvent>,
    pub output: String,
}

#[derive(Debug, Clone)]
pub struct GitUiState {
    pub repository: GitRepository,
    pub files: Vec<GitFileStatus>,
    pub history: Vec<GitCommit>,
    pub remote: GitRemoteState,
    pub commit_message: String,
    pub loading: bool,
    pub error: Option<String>,
    pub network_run: Option<GitNetworkRunState>,
}

impl Default for GitUiState {
    fn default() -> Self {
        Self {
            repository: GitRepository {
                is_initialized: false,
                remote_url: None,
                provider: None,
                current_branch: None,
                has_lfs: false,
            },
            files: Vec::new(),
            history: Vec::new(),
            remote: GitRemoteState {
                local_branch: None,
                upstream_branch: None,
                has_upstream: false,
                ahead: 0,
                behind: 0,
            },
            commit_message: String::new(),
            loading: false,
            error: None,
            network_run: None,
        }
    }
}

impl GitUiState {
    pub fn apply_snapshot(&mut self, snapshot: GitSnapshot) {
        self.repository = snapshot.repository;
        self.files = snapshot.files;
        self.history = snapshot.history;
        self.remote = snapshot.remote;
        self.loading = false;
        self.error = None;
    }
}

#[derive(Debug, Clone)]
pub struct GitNetworkRunState {
    pub task_id: u64,
    pub output: Arc<Mutex<Vec<GitOutput>>>,
}

#[derive(Debug, Clone)]
pub enum GitDiffKind {
    File,
    Commit { hash: String },
}

#[derive(Debug, Clone)]
pub struct GitDiffState {
    pub kind: GitDiffKind,
    pub changes: Vec<ChangedFile>,
    pub selected_path: Option<String>,
    pub diff: Option<GitFileDiff>,
    pub loading: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone)]
pub struct GitDiffLoad {
    pub changes: Vec<ChangedFile>,
    pub selected_path: Option<String>,
    pub diff: Option<GitFileDiff>,
}

impl GitDiffState {
    pub fn loading_file(path: String) -> Self {
        Self {
            kind: GitDiffKind::File,
            changes: Vec::new(),
            selected_path: Some(path),
            diff: None,
            loading: true,
            error: None,
        }
    }

    pub fn loading_commit(hash: String) -> Self {
        Self {
            kind: GitDiffKind::Commit { hash },
            changes: Vec::new(),
            selected_path: None,
            diff: None,
            loading: true,
            error: None,
        }
    }
}

pub fn sidebar_view(
    state: &GitUiState,
    offline_mode: bool,
    locale: UiLocale,
) -> Element<'static, Message> {
    if state.loading && !state.repository.is_initialized {
        return text(t(locale, "git.loading"))
            .size(12)
            .color(Color::from_rgb(0.5, 0.5, 0.55))
            .into();
    }
    if !state.repository.is_initialized {
        return column![
            text(t(locale, "git.notRepository"))
                .size(12)
                .color(Color::from_rgb(0.6, 0.6, 0.65)),
            button(text(t(locale, "git.initialize")).size(12))
                .on_press(Message::GitInitialize)
                .padding([5, 8])
                .style(inputs::primary_button),
            error_view(state.error.as_deref()),
        ]
        .spacing(8)
        .into();
    }

    let branch = state.repository.current_branch.as_deref().unwrap_or("—");
    let upstream = state.remote.upstream_branch.as_deref().unwrap_or("—");
    let header = column![
        text(format!("⎇ {branch}"))
            .size(13)
            .shaping(Shaping::Advanced),
        text(format!(
            "{upstream}  ↑{} ↓{}",
            state.remote.ahead, state.remote.behind
        ))
        .size(11)
        .color(Color::from_rgb(0.55, 0.55, 0.6)),
    ]
    .spacing(2);

    let network_running = state.network_run.is_some();
    let network_button = |key, message| {
        let button = button(text(t(locale, key)).size(11))
            .padding([4, 6])
            .style(inputs::secondary_button);
        if offline_mode || network_running {
            button
        } else {
            button.on_press(message)
        }
    };
    let actions = row![
        network_button("git.fetch", Message::GitFetch),
        network_button("git.pull", Message::GitPull),
        network_button("git.push", Message::GitPush),
    ]
    .spacing(4)
    .wrap();

    let sync_legend = row![
        sync_legend_item(SyncStatus::Both, locale),
        sync_legend_item(SyncStatus::LocalOnly, locale),
        sync_legend_item(SyncStatus::RemoteOnly, locale),
    ]
    .spacing(8)
    .wrap();

    let mut content: Vec<Element<'static, Message>> =
        vec![header.into(), sync_legend.into(), actions.into()];
    if offline_mode {
        content.push(
            text(t(locale, "git.airplaneBlocked"))
                .size(10)
                .color(Color::from_rgb(0.72, 0.58, 0.35))
                .into(),
        );
    }
    content.push(section_title(
        tw(
            locale,
            "git.changesCount",
            &[("count", &state.files.len().to_string())],
        ),
        locale,
    ));
    content.push(
        row![
            text_input(&t(locale, "git.commitMessage"), &state.commit_message)
                .on_input(Message::GitCommitMessageChanged)
                .size(11)
                .padding([5, 7])
                .style(inputs::field_style),
            {
                let commit = button(text(t(locale, "git.commit")).size(11))
                    .padding([5, 7])
                    .style(inputs::primary_button);
                if state.files.is_empty() || state.commit_message.trim().is_empty() {
                    commit
                } else {
                    commit.on_press(Message::GitCommit)
                }
            },
        ]
        .spacing(4)
        .into(),
    );
    if state.files.is_empty() {
        content.push(muted_text(t(locale, "git.noChanges")));
    } else {
        content.extend(state.files.iter().map(status_button));
    }
    content.push(section_title(t(locale, "git.history"), locale));
    if state.history.is_empty() {
        content.push(muted_text(t(locale, "git.noCommits")));
    } else {
        content.extend(state.history.iter().take(20).map(history_button));
    }
    content.push(
        button(text(t(locale, "git.pruneLfs")).size(11))
            .on_press(Message::GitPruneLfs)
            .padding([4, 7])
            .style(inputs::secondary_button)
            .into(),
    );
    if let Some(run) = &state.network_run {
        content.push(network_output(run, locale));
    }
    content.push(error_view(state.error.as_deref()));
    iced::widget::Column::with_children(content)
        .spacing(6)
        .into()
}

fn section_title(label: String, _locale: UiLocale) -> Element<'static, Message> {
    text(label)
        .size(11)
        .color(Color::from_rgb(0.62, 0.62, 0.68))
        .into()
}

fn muted_text(label: String) -> Element<'static, Message> {
    text(label)
        .size(11)
        .color(Color::from_rgb(0.5, 0.5, 0.55))
        .into()
}

fn error_view(error: Option<&str>) -> Element<'static, Message> {
    text(error.unwrap_or_default().to_string())
        .size(11)
        .color(Color::from_rgb(0.85, 0.35, 0.35))
        .into()
}

fn status_button(file: &GitFileStatus) -> Element<'static, Message> {
    let path = file.path.clone();
    button(
        row![
            text(path.clone()).size(11),
            Space::with_width(Length::Fill),
            text(file.kind.code()).size(11),
        ]
        .spacing(6),
    )
    .on_press(Message::OpenGitFileDiff(path))
    .width(Length::Fill)
    .padding([5, 8])
    .style(inputs::disclosure_button)
    .into()
}

fn history_button(commit: &GitCommit) -> Element<'static, Message> {
    let hash = commit.hash.clone();
    let subject = commit.subject.clone().unwrap_or_else(|| hash.clone());
    let short = hash.chars().take(7).collect::<String>();
    button(
        column![
            text(subject.clone()).size(11),
            row![
                sync_status_dot(commit.sync_status),
                text(short).size(10).color(Color::from_rgb(0.5, 0.5, 0.56)),
            ]
            .spacing(4)
            .align_y(Alignment::Center),
        ]
        .spacing(1),
    )
    .on_press(Message::OpenGitCommitDiff { hash, subject })
    .width(Length::Fill)
    .padding([5, 8])
    .style(inputs::disclosure_button)
    .into()
}

fn sync_legend_item(status: SyncStatus, locale: UiLocale) -> Element<'static, Message> {
    let key = match status {
        SyncStatus::Both => "git.synced",
        SyncStatus::LocalOnly => "git.localOnly",
        SyncStatus::RemoteOnly => "git.remoteOnly",
    };
    row![
        sync_status_dot(status),
        text(t(locale, key))
            .size(10)
            .color(Color::from_rgb(0.5, 0.5, 0.56)),
    ]
    .spacing(4)
    .align_y(Alignment::Center)
    .into()
}

fn sync_status_dot(status: SyncStatus) -> Element<'static, Message> {
    let color = sync_status_color(status);
    container(Space::new(8, 8))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(Background::Color(color)),
            border: iced::Border {
                radius: 4.0.into(),
                ..iced::Border::default()
            },
            ..container::Style::default()
        })
        .into()
}

fn sync_status_color(status: SyncStatus) -> Color {
    match status {
        SyncStatus::Both => Color::from_rgb8(0x73, 0xC9, 0x91),
        SyncStatus::LocalOnly => Color::from_rgb8(0xCC, 0xA7, 0x00),
        SyncStatus::RemoteOnly => Color::from_rgb8(0x75, 0xBE, 0xFF),
    }
}

fn network_output(run: &GitNetworkRunState, locale: UiLocale) -> Element<'static, Message> {
    let output = run
        .output
        .lock()
        .unwrap()
        .iter()
        .map(|chunk| chunk.text.as_str())
        .collect::<String>();
    column![
        text(t(locale, "git.liveOutput"))
            .size(11)
            .color(Color::from_rgb(0.62, 0.62, 0.68)),
        container(
            text(output)
                .size(10)
                .font(Font::MONOSPACE)
                .wrapping(Wrapping::Word)
        )
        .padding(6),
        button(text(t(locale, "common.cancel")).size(11))
            .on_press(Message::CancelTask(
                crate::state::navigation::TaskSource::Local,
                run.task_id,
            ))
            .padding([4, 7])
            .style(inputs::secondary_button),
    ]
    .spacing(4)
    .into()
}

pub fn diff_view(
    state: &GitDiffState,
    view_style: &str,
    wrap_long_lines: bool,
    locale: UiLocale,
) -> Element<'static, Message> {
    let title = match &state.kind {
        GitDiffKind::File => state
            .selected_path
            .clone()
            .unwrap_or_else(|| t(locale, "git.diff")),
        GitDiffKind::Commit { hash } => tw(
            locale,
            "git.commitDiffTitle",
            &[("hash", &hash.chars().take(7).collect::<String>())],
        ),
    };
    let header = inputs::card(
        column![
            text(title).size(20).shaping(Shaping::Advanced),
            text(t(locale, "git.readOnly"))
                .size(11)
                .color(Color::from_rgb(0.58, 0.58, 0.63)),
        ]
        .spacing(4),
    );
    let mut sections: Vec<Element<'static, Message>> = vec![header.into()];
    if matches!(state.kind, GitDiffKind::Commit { .. }) && !state.changes.is_empty() {
        sections.push(inputs::toolbar(
            state
                .changes
                .iter()
                .map(|change| {
                    let selected = state.selected_path.as_deref() == Some(&change.path);
                    let button = button(text(change.path.clone()).size(11))
                        .padding([4, 7])
                        .style(if selected {
                            inputs::primary_button
                        } else {
                            inputs::secondary_button
                        });
                    if selected {
                        button.into()
                    } else if let GitDiffKind::Commit { hash } = &state.kind {
                        button
                            .on_press(Message::SelectGitCommitFile {
                                hash: hash.clone(),
                                change: change.clone(),
                            })
                            .into()
                    } else {
                        button.into()
                    }
                })
                .collect(),
            Vec::new(),
        ));
    }
    if state.loading {
        sections.push(muted_text(t(locale, "git.loadingDiff")));
    } else if let Some(error) = &state.error {
        sections.push(error_view(Some(error)));
    } else if let Some(diff) = &state.diff {
        let wrapping = if wrap_long_lines {
            Wrapping::Word
        } else {
            Wrapping::None
        };
        if view_style == "side-by-side" {
            let original = code_card(t(locale, "git.original"), diff.original.clone(), wrapping);
            let modified = code_card(t(locale, "git.modified"), diff.modified.clone(), wrapping);
            sections.push(
                row![original, modified]
                    .spacing(12)
                    .height(Length::Fill)
                    .into(),
            );
        } else {
            sections.push(code_card(
                t(locale, "git.inlineDiff"),
                diff.patch.clone(),
                wrapping,
            ));
        }
    } else {
        sections.push(muted_text(t(locale, "git.noDiff")));
    }
    container(
        scrollable(
            iced::widget::Column::with_children(sections)
                .spacing(12)
                .padding(16),
        )
        .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
        .style(inputs::scrollable_style),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn code_card(label: String, contents: String, wrapping: Wrapping) -> Element<'static, Message> {
    inputs::card(
        column![
            text(label)
                .size(12)
                .color(Color::from_rgb(0.66, 0.66, 0.72)),
            scrollable(
                text(contents)
                    .size(12)
                    .font(Font::MONOSPACE)
                    .wrapping(wrapping)
            )
            .direction(scrollable::Direction::Vertical(inputs::compact_scrollbar()))
            .style(inputs::scrollable_style)
            .height(Length::Fill),
        ]
        .spacing(8)
        .height(Length::Fill),
    )
    .height(Length::Fill)
    .into()
}

pub fn file_tab(path: &str) -> Tab {
    Tab {
        id: format!("git-diff:{path}"),
        tab_type: TabType::GitDiff,
        title: PathTitle(path).to_string(),
        is_transient: true,
        is_dirty: false,
    }
}

pub fn commit_tab(hash: &str, subject: &str) -> Tab {
    let short = hash.chars().take(7).collect::<String>();
    Tab {
        id: format!("git-diff:commit:{hash}"),
        tab_type: TabType::GitDiff,
        title: if subject.is_empty() {
            short
        } else {
            format!("{short} {subject}")
        },
        is_transient: true,
        is_dirty: false,
    }
}

struct PathTitle<'a>(&'a str);

impl fmt::Display for PathTitle<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.0.rsplit('/').next().unwrap_or(self.0))
    }
}

use std::fmt;

#[cfg(test)]
mod tests {
    use super::*;
    use iced::advanced::widget::{Tree, tree::Tag};
    use iced::widget::text_input;

    type TextInputState =
        text_input::State<<iced::Renderer as iced::advanced::text::Renderer>::Paragraph>;

    fn count_text_inputs(tree: &Tree) -> usize {
        usize::from(tree.tag == Tag::of::<TextInputState>())
            + tree.children.iter().map(count_text_inputs).sum::<usize>()
    }

    #[test]
    fn repository_location_is_never_editable_in_the_git_sidebar() {
        let not_initialized = GitUiState::default();
        let inactive_tree =
            Tree::new(sidebar_view(&not_initialized, false, UiLocale::En).as_widget());
        assert_eq!(count_text_inputs(&inactive_tree), 0);

        let mut initialized = GitUiState::default();
        initialized.repository.is_initialized = true;
        initialized.repository.remote_url = Some("ssh://example.test/blog.git".into());
        let active_tree = Tree::new(sidebar_view(&initialized, false, UiLocale::En).as_widget());
        assert_eq!(count_text_inputs(&active_tree), 1);
    }

    #[test]
    fn commit_sync_colors_match_bds2() {
        assert_eq!(
            sync_status_color(SyncStatus::Both),
            Color::from_rgb8(0x73, 0xC9, 0x91)
        );
        assert_eq!(
            sync_status_color(SyncStatus::LocalOnly),
            Color::from_rgb8(0xCC, 0xA7, 0x00)
        );
        assert_eq!(
            sync_status_color(SyncStatus::RemoteOnly),
            Color::from_rgb8(0x75, 0xBE, 0xFF)
        );
    }
}
