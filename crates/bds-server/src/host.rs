use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, mpsc};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Result, anyhow};
use bds_core::db::Database;
use bds_core::engine::domain_events::EventSubscription;
use bds_core::engine::task::{TaskManager, TaskSnapshot, TaskStatus};
use bds_core::engine::{domain_events, project, settings};
use bds_core::scripting::{CoreHost, HostApi};
use serde_json::{Value, json};

use crate::protocol::{Command, PROTOCOL_VERSION, RemoteTask, Request, ServerMessage};

pub fn run_local_terminal(host: ApplicationHost) -> Result<()> {
    crate::tui::run_local(host)
}

#[derive(Clone)]
pub struct ApplicationHost {
    inner: Arc<HostInner>,
}

struct HostInner {
    database_path: PathBuf,
    data_root: PathBuf,
    tasks: Arc<TaskManager>,
    sync_watcher: SyncWatcher,
    completed_requests: Mutex<HashMap<String, ServerMessage>>,
    execution_lock: Mutex<()>,
}

struct SyncWatcher {
    shutdown: mpsc::Sender<()>,
    thread: Option<JoinHandle<()>>,
    errors: Arc<Mutex<WatcherErrors>>,
}

#[derive(Default)]
struct WatcherErrors {
    next_id: u64,
    entries: VecDeque<(u64, String)>,
}

impl SyncWatcher {
    fn start(database_path: PathBuf) -> Result<Self> {
        let (shutdown, shutdown_rx) = mpsc::channel();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let errors = Arc::new(Mutex::new(WatcherErrors::default()));
        let thread_errors = Arc::clone(&errors);
        let thread = std::thread::Builder::new()
            .name("bds-cli-sync-watcher".into())
            .spawn(move || {
                let database = match Database::open(&database_path) {
                    Ok(database) => database,
                    Err(error) => {
                        let _ = started_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                let _ = started_tx.send(Ok(()));
                loop {
                    match shutdown_rx.recv_timeout(Duration::from_millis(100)) {
                        Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                        Err(mpsc::RecvTimeoutError::Timeout) => {
                            if let Err(error) =
                                bds_core::engine::cli_sync::poll_notifications(database.conn())
                            {
                                let mut errors = lock(&thread_errors);
                                let message = error.to_string();
                                if errors
                                    .entries
                                    .back()
                                    .is_some_and(|(_, last)| last == &message)
                                {
                                    continue;
                                }
                                errors.next_id += 1;
                                let id = errors.next_id;
                                errors.entries.push_back((id, message));
                                if errors.entries.len() > 128 {
                                    errors.entries.pop_front();
                                }
                            }
                        }
                    }
                }
            })?;
        match started_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                shutdown,
                thread: Some(thread),
                errors,
            }),
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(anyhow!("could not start the CLI sync watcher: {error}"))
            }
            Err(_) => {
                let _ = thread.join();
                Err(anyhow!("the CLI sync watcher stopped during startup"))
            }
        }
    }

    fn errors_since(&self, id: u64) -> Vec<(u64, String)> {
        lock(&self.errors)
            .entries
            .iter()
            .filter(|(entry_id, _)| *entry_id > id)
            .cloned()
            .collect()
    }
}

impl Drop for SyncWatcher {
    fn drop(&mut self) {
        let _ = self.shutdown.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl ApplicationHost {
    pub fn start(database_path: PathBuf, data_root: PathBuf) -> Result<Self> {
        if let Some(parent) = database_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let db = Database::open(&database_path)?;
        db.migrate()
            .map_err(|error| anyhow!("could not migrate the application database: {error}"))?;
        bds_core::engine::search::prepare_search_index(db.conn())?;
        let sync_watcher = SyncWatcher::start(database_path.clone())?;
        Ok(Self {
            inner: Arc::new(HostInner {
                database_path,
                data_root,
                tasks: Arc::new(TaskManager::default()),
                sync_watcher,
                completed_requests: Mutex::new(HashMap::new()),
                execution_lock: Mutex::new(()),
            }),
        })
    }

    pub fn tasks(&self) -> Arc<TaskManager> {
        Arc::clone(&self.inner.tasks)
    }

    pub fn session(&self) -> Result<ApplicationSession> {
        let db = self.database()?;
        let locale = settings::ui_language(db.conn())?
            .map(|value| bds_core::i18n::normalize_language(&value))
            .unwrap_or_else(bds_core::i18n::detect_os_locale)
            .code()
            .to_owned();
        Ok(ApplicationSession {
            host: self.clone(),
            selected_project: None,
            negotiated: false,
            locale,
            sequence: 0,
            events: domain_events::subscribe(),
            last_tasks: Vec::new(),
            last_watcher_error: 0,
        })
    }

    pub(crate) fn database(&self) -> Result<Database> {
        Database::open(&self.inner.database_path).map_err(Into::into)
    }

    pub(crate) fn database_path(&self) -> &std::path::Path {
        &self.inner.database_path
    }

    pub(crate) fn project_data_dir(&self, project: &bds_core::model::Project) -> PathBuf {
        project
            .data_path
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| self.inner.data_root.join("projects").join(&project.id))
    }

    fn cached(&self, id: &str) -> Option<ServerMessage> {
        lock(&self.inner.completed_requests).get(id).cloned()
    }

    fn remember(&self, id: String, response: ServerMessage) {
        let mut completed = lock(&self.inner.completed_requests);
        if completed.len() >= 4_096
            && let Some(oldest) = completed.keys().next().cloned()
        {
            completed.remove(&oldest);
        }
        completed.insert(id, response);
    }
}

pub struct ApplicationSession {
    host: ApplicationHost,
    selected_project: Option<SelectedProject>,
    negotiated: bool,
    locale: String,
    sequence: u64,
    events: EventSubscription,
    last_tasks: Vec<RemoteTask>,
    last_watcher_error: u64,
}

#[derive(Clone)]
struct SelectedProject {
    id: String,
    data_dir: PathBuf,
}

impl ApplicationSession {
    pub fn locale(&self) -> &str {
        &self.locale
    }

    pub fn handle(&mut self, request: Request) -> ServerMessage {
        let idempotent = matches!(request.command, Command::Call { .. });
        if idempotent {
            let inner = Arc::clone(&self.host.inner);
            let _execution = lock(&inner.execution_lock);
            if let Some(response) = self.host.cached(&request.id) {
                return response;
            }
            let id = request.id;
            let response = self.response(id.clone(), &request.command);
            self.host.remember(id, response.clone());
            return response;
        }
        let id = request.id;
        self.response(id, &request.command)
    }

    fn response(&mut self, id: String, command: &Command) -> ServerMessage {
        match self.execute(command) {
            Ok(result) => ServerMessage::Response { id, result },
            Err(error) => ServerMessage::Error {
                id,
                code: error.code.to_owned(),
                message: error.message,
            },
        }
    }

    pub fn pending(&mut self) -> Vec<ServerMessage> {
        let mut messages = self
            .events
            .drain()
            .into_iter()
            .filter(|event| {
                event.project_id().is_none()
                    || self
                        .selected_project
                        .as_ref()
                        .is_some_and(|project| event.project_id() == Some(project.id.as_str()))
            })
            .map(|event| {
                self.sequence += 1;
                ServerMessage::Event {
                    sequence: self.sequence,
                    event,
                }
            })
            .collect::<Vec<_>>();
        let tasks = self
            .host
            .inner
            .tasks
            .snapshots()
            .into_iter()
            .map(remote_task)
            .collect::<Vec<_>>();
        if tasks != self.last_tasks {
            self.last_tasks.clone_from(&tasks);
            self.sequence += 1;
            messages.push(ServerMessage::Tasks {
                sequence: self.sequence,
                tasks,
            });
        }
        for (id, message) in self
            .host
            .inner
            .sync_watcher
            .errors_since(self.last_watcher_error)
        {
            self.last_watcher_error = id;
            messages.push(ServerMessage::Error {
                id: String::new(),
                code: "sync_watcher_error".into(),
                message,
            });
        }
        messages
    }

    fn execute(&mut self, command: &Command) -> Result<Value, ProtocolError> {
        if !matches!(command, Command::Hello { .. }) && !self.negotiated {
            return Err(ProtocolError::new(
                "protocol_required",
                "hello must be the first request",
            ));
        }
        match command {
            Command::Hello { protocol_version } => {
                if *protocol_version != PROTOCOL_VERSION {
                    return Err(ProtocolError::new(
                        "unsupported_protocol",
                        format!(
                            "unsupported remote protocol {protocol_version}; server requires {PROTOCOL_VERSION}"
                        ),
                    ));
                }
                self.negotiated = true;
                Ok(json!({
                    "protocol_version": PROTOCOL_VERSION,
                    "server_name": "Blogging Desktop Server",
                    "locale": self.locale,
                }))
            }
            Command::ListProjects => {
                let db = self.host.database().map_err(ProtocolError::engine)?;
                let projects = project::list_projects(db.conn()).map_err(ProtocolError::engine)?;
                serde_json::to_value(projects).map_err(ProtocolError::engine)
            }
            Command::OpenProject { project_id } => {
                let db = self.host.database().map_err(ProtocolError::engine)?;
                let value =
                    bds_core::db::queries::project::get_project_by_id(db.conn(), project_id)
                        .map_err(|_| {
                            ProtocolError::new(
                                "project_not_found",
                                format!("project '{project_id}' was not found on the server"),
                            )
                        })?;
                let data_dir = value
                    .data_path
                    .as_deref()
                    .map(PathBuf::from)
                    .unwrap_or_else(|| self.host.inner.data_root.join("projects").join(&value.id));
                if !data_dir.join("meta/project.json").is_file() {
                    return Err(ProtocolError::new(
                        "project_unavailable",
                        format!("project '{}' data is unavailable", value.name),
                    ));
                }
                self.selected_project = Some(SelectedProject {
                    id: value.id.clone(),
                    data_dir,
                });
                serde_json::to_value(value).map_err(ProtocolError::engine)
            }
            Command::Call {
                namespace,
                method,
                arguments,
            } => {
                let selected = self.selected_project.as_ref().ok_or_else(|| {
                    ProtocolError::new("project_required", "open a remote project first")
                })?;
                CoreHost::new(
                    &self.host.inner.database_path,
                    &selected.id,
                    &selected.data_dir,
                )
                .with_task_manager(Arc::clone(&self.host.inner.tasks))
                .call(namespace, method, arguments.clone())
                .map_err(|message| ProtocolError::new("engine_error", message))
            }
            Command::Ping => Ok(json!({"ok": true})),
        }
    }
}

struct ProtocolError {
    code: &'static str,
    message: String,
}

impl ProtocolError {
    fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
        }
    }

    fn engine(error: impl std::fmt::Display) -> Self {
        Self::new("engine_error", error.to_string())
    }
}

fn remote_task(snapshot: TaskSnapshot) -> RemoteTask {
    let (status, failure) = match snapshot.status {
        TaskStatus::Pending => ("pending", None),
        TaskStatus::Running => ("running", None),
        TaskStatus::Completed => ("completed", None),
        TaskStatus::Failed(message) => ("failed", Some(message)),
        TaskStatus::Cancelled => ("cancelled", None),
    };
    RemoteTask {
        id: snapshot.id,
        label: snapshot.label,
        status: status.to_owned(),
        progress: snapshot.progress,
        message: snapshot.message.or(failure),
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::{Command, Request};

    struct Fixture {
        _root: tempfile::TempDir,
        host: ApplicationHost,
        project_id: String,
    }

    impl Fixture {
        fn new() -> Self {
            let root = tempfile::tempdir().unwrap();
            let database_path = root.path().join("bds.db");
            let data_root = root.path().join("data");
            let host = ApplicationHost::start(database_path.clone(), data_root.clone()).unwrap();
            let db = Database::open(&database_path).unwrap();
            let project_dir = root.path().join("blog");
            let value = project::create_project(
                db.conn(),
                "Remote Blog",
                Some(project_dir.to_str().unwrap()),
            )
            .unwrap();
            settings::set(db.conn(), settings::UI_LANGUAGE_KEY, "de").unwrap();
            Self {
                _root: root,
                host,
                project_id: value.id,
            }
        }

        fn session(&self) -> ApplicationSession {
            let mut session = self.host.session().unwrap();
            assert!(matches!(
                session.handle(request(
                    "hello",
                    Command::Hello {
                        protocol_version: PROTOCOL_VERSION,
                    }
                )),
                ServerMessage::Response { .. }
            ));
            session
        }
    }

    fn request(id: &str, command: Command) -> Request {
        Request {
            id: id.to_owned(),
            command,
        }
    }

    #[test]
    fn session_negotiates_server_locale_and_selects_a_project() {
        let fixture = Fixture::new();
        let mut session = fixture.host.session().unwrap();
        assert_eq!(session.locale(), "de");
        assert!(matches!(
            session.handle(request("early", Command::ListProjects)),
            ServerMessage::Error { ref code, .. } if code == "protocol_required"
        ));
        let mut session = fixture.session();
        let listed = session.handle(request("list", Command::ListProjects));
        assert!(matches!(listed, ServerMessage::Response { .. }));
        let opened = session.handle(request(
            "open",
            Command::OpenProject {
                project_id: fixture.project_id.clone(),
            },
        ));
        assert!(matches!(opened, ServerMessage::Response { .. }));
    }

    #[test]
    fn two_clients_observe_one_ordered_mutation_and_request_replay_is_idempotent() {
        let fixture = Fixture::new();
        let mut first = fixture.session();
        let mut second = fixture.session();
        for (index, session) in [&mut first, &mut second].into_iter().enumerate() {
            session.handle(request(
                &format!("open-{index}"),
                Command::OpenProject {
                    project_id: fixture.project_id.clone(),
                },
            ));
            let _ = session.pending();
        }
        let create = request(
            "globally-unique-create",
            Command::Call {
                namespace: "posts".into(),
                method: "create".into(),
                arguments: vec![json!({"title":"Exactly Once","content":"body"})],
            },
        );
        assert!(matches!(
            first.handle(create.clone()),
            ServerMessage::Response { .. }
        ));
        assert_eq!(second.handle(create.clone()), first.handle(create));

        let first_events = first.pending();
        let second_events = second.pending();
        assert_eq!(event_count(&first_events, &fixture.project_id), 1);
        assert_eq!(event_count(&second_events, &fixture.project_id), 1);
        assert!(strictly_ordered(&first_events));
        assert!(strictly_ordered(&second_events));

        let db = fixture.host.database().unwrap();
        assert_eq!(
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn simultaneous_replay_from_two_clients_executes_the_write_once() {
        let fixture = Fixture::new();
        let mut first = fixture.session();
        let mut second = fixture.session();
        for (index, session) in [&mut first, &mut second].into_iter().enumerate() {
            session.handle(request(
                &format!("parallel-open-{index}"),
                Command::OpenProject {
                    project_id: fixture.project_id.clone(),
                },
            ));
        }
        let barrier = Arc::new(std::sync::Barrier::new(2));
        let command = request(
            "same-concurrent-id",
            Command::Call {
                namespace: "posts".into(),
                method: "create".into(),
                arguments: vec![json!({"title":"Concurrent","content":"body"})],
            },
        );
        let first_barrier = Arc::clone(&barrier);
        let first_command = command.clone();
        let first = std::thread::spawn(move || {
            first_barrier.wait();
            first.handle(first_command)
        });
        let second = std::thread::spawn(move || {
            barrier.wait();
            second.handle(command)
        });
        assert_eq!(first.join().unwrap(), second.join().unwrap());
        let db = fixture.host.database().unwrap();
        assert_eq!(
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &fixture.project_id)
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn task_progress_is_shared_without_repeating_unchanged_snapshots() {
        let fixture = Fixture::new();
        let mut session = fixture.session();
        let _ = session.pending();
        let task = fixture.host.tasks().submit("Generate site");
        fixture
            .host
            .tasks()
            .report_progress(task, Some(0.5), Some("Writing".into()));
        let update = session.pending();
        assert!(matches!(
            update.as_slice(),
            [ServerMessage::Tasks { tasks, .. }] if tasks[0].progress == Some(0.5)
        ));
        assert!(
            session
                .pending()
                .iter()
                .all(|message| !matches!(message, ServerMessage::Tasks { .. }))
        );
    }

    #[test]
    fn headless_sync_watcher_republishes_external_cli_notifications() {
        let fixture = Fixture::new();
        let mut session = fixture.session();
        session.handle(request(
            "open-for-sync",
            Command::OpenProject {
                project_id: fixture.project_id.clone(),
            },
        ));
        let _ = session.pending();

        let db = fixture.host.database().unwrap();
        bds_core::engine::cli_sync::record_cli_event(
            db.conn(),
            &bds_core::model::DomainEvent::EntityChanged {
                project_id: fixture.project_id.clone(),
                entity: bds_core::model::DomainEntity::Post,
                entity_id: "external-post".into(),
                action: bds_core::model::NotificationAction::Created,
            },
        )
        .unwrap();

        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        let mut updates = Vec::new();
        while std::time::Instant::now() < deadline {
            updates.extend(session.pending());
            if event_count(&updates, &fixture.project_id) == 1 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }

        assert_eq!(event_count(&updates, &fixture.project_id), 1);
        let notifications =
            bds_core::db::queries::db_notification::list_notifications(db.conn()).unwrap();
        assert_eq!(notifications.len(), 1);
        assert!(notifications[0].seen_at.is_some());
    }

    fn event_count(messages: &[ServerMessage], project_id: &str) -> usize {
        messages
            .iter()
            .filter(|message| {
                matches!(
                    message,
                    ServerMessage::Event { event, .. }
                        if event.project_id() == Some(project_id)
                )
            })
            .count()
    }

    fn strictly_ordered(messages: &[ServerMessage]) -> bool {
        let sequences = messages
            .iter()
            .filter_map(|message| match message {
                ServerMessage::Event { sequence, .. } | ServerMessage::Tasks { sequence, .. } => {
                    Some(*sequence)
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        sequences.windows(2).all(|pair| pair[0] < pair[1])
    }
}
