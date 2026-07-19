use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow};
use bds_core::engine::mcp::McpHttpServer;
use russh::keys::PublicKey;
use russh::server::{Auth, ChannelOpenHandle, Handler, Msg, Server, Session};
use russh::{Channel, ChannelId};
use tokio::io::{AsyncBufReadExt as _, AsyncWrite, AsyncWriteExt as _, BufReader};

use crate::auth::KeyMaterial;
use crate::host::{ApplicationHost, ApplicationSession};
use crate::protocol::{Command, PROTOCOL_VERSION, Request, SUBSYSTEM, ServerMessage};

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub database_path: PathBuf,
    pub data_root: PathBuf,
    pub bind: IpAddr,
    pub port: u16,
    pub mcp_port: u16,
}

impl ServerConfig {
    pub fn local(database_path: PathBuf, data_root: PathBuf) -> Self {
        Self {
            database_path,
            data_root,
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 2222,
            mcp_port: 0,
        }
    }

    pub fn from_environment(database_path: PathBuf, data_root: PathBuf) -> Result<Self> {
        let mut config = Self::local(database_path, data_root);
        if let Some(bind) = std::env::var_os("BDS_SSH_BIND") {
            config.bind = bind
                .to_string_lossy()
                .parse()
                .context("BDS_SSH_BIND must be an IP address")?;
        }
        if let Some(port) = std::env::var_os("BDS_SSH_PORT") {
            config.port = port
                .to_string_lossy()
                .parse()
                .context("BDS_SSH_PORT must be a TCP port")?;
        }
        Ok(config)
    }
}

pub struct ServerRuntime {
    address: SocketAddr,
    key_material: KeyMaterial,
    host: ApplicationHost,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    thread: Option<JoinHandle<Result<()>>>,
    _mcp: McpHttpServer,
}

impl ServerRuntime {
    pub fn start(config: ServerConfig) -> Result<Self> {
        let key_material = KeyMaterial::ensure(&config.data_root)?;
        let host = ApplicationHost::start(config.database_path.clone(), config.data_root.clone())?;
        let server_host = host.clone();
        let mcp = McpHttpServer::start(config.database_path, config.mcp_port)?;
        let forward_address = mcp.address();
        let listener =
            std::net::TcpListener::bind((config.bind, config.port)).with_context(|| {
                format!(
                    "could not bind the RuDS SSH server to {}:{}",
                    config.bind, config.port
                )
            })?;
        listener.set_nonblocking(true)?;
        let address = listener.local_addr()?;
        let host_key = key_material.host_key()?;
        let auth = key_material.clone();
        let (shutdown, shutdown_rx) = tokio::sync::oneshot::channel();
        let thread = std::thread::Builder::new()
            .name("bds-ssh-server".into())
            .spawn(move || -> Result<()> {
                let runtime = tokio::runtime::Builder::new_multi_thread()
                    .worker_threads(2)
                    .enable_all()
                    .build()?;
                runtime.block_on(async move {
                    let listener = tokio::net::TcpListener::from_std(listener)?;
                    let ssh_config = Arc::new(russh::server::Config {
                        inactivity_timeout: Some(Duration::from_secs(60 * 60)),
                        auth_rejection_time: Duration::from_millis(500),
                        auth_rejection_time_initial: Some(Duration::ZERO),
                        keys: vec![host_key],
                        nodelay: true,
                        ..Default::default()
                    });
                    let mut factory = ServerFactory {
                        auth,
                        host: server_host,
                        forward_address,
                    };
                    let running = factory.run_on_socket(ssh_config, &listener);
                    let handle = running.handle();
                    tokio::spawn(async move {
                        let _ = shutdown_rx.await;
                        handle.shutdown("RuDS server is shutting down".into());
                    });
                    running.await.map_err(anyhow::Error::from)
                })
            })?;
        Ok(Self {
            address,
            key_material,
            host,
            shutdown: Some(shutdown),
            thread: Some(thread),
            _mcp: mcp,
        })
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    pub fn key_material(&self) -> &KeyMaterial {
        &self.key_material
    }

    pub fn application_host(&self) -> ApplicationHost {
        self.host.clone()
    }

    pub fn stop(mut self) -> Result<()> {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow!("RuDS SSH server thread panicked"))??;
        }
        Ok(())
    }
}

impl Drop for ServerRuntime {
    fn drop(&mut self) {
        self.shutdown.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

struct ServerFactory {
    auth: KeyMaterial,
    host: ApplicationHost,
    forward_address: SocketAddr,
}

impl Server for ServerFactory {
    type Handler = ConnectionHandler;

    fn new_client(&mut self, _peer_addr: Option<SocketAddr>) -> Self::Handler {
        ConnectionHandler {
            auth: self.auth.clone(),
            host: self.host.clone(),
            forward_address: self.forward_address,
            channels: HashMap::new(),
        }
    }
}

struct ConnectionHandler {
    auth: KeyMaterial,
    host: ApplicationHost,
    forward_address: SocketAddr,
    channels: HashMap<ChannelId, Channel<Msg>>,
}

impl Handler for ConnectionHandler {
    type Error = anyhow::Error;

    async fn auth_publickey(&mut self, _user: &str, key: &PublicKey) -> Result<Auth, Self::Error> {
        Ok(if self.auth.authorizes(key)? {
            Auth::Accept
        } else {
            Auth::reject()
        })
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.channels.insert(channel.id(), channel);
        reply.accept().await;
        Ok(())
    }

    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn subsystem_request(
        &mut self,
        channel: ChannelId,
        name: &str,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        if name != SUBSYSTEM {
            session.channel_failure(channel)?;
            return Ok(());
        }
        let Some(channel) = self.channels.remove(&channel) else {
            session.channel_failure(channel)?;
            return Ok(());
        };
        let application = self.host.session()?;
        session.channel_success(channel.id())?;
        tokio::spawn(async move {
            let _ = run_protocol(channel, application).await;
        });
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let Some(channel) = self.channels.remove(&channel) else {
            session.channel_failure(channel)?;
            return Ok(());
        };
        let application = self.host.session()?;
        session.channel_success(channel.id())?;
        tokio::spawn(async move {
            let _ = run_terminal_session(channel, application).await;
        });
        Ok(())
    }

    async fn channel_open_direct_tcpip(
        &mut self,
        channel: Channel<Msg>,
        host_to_connect: &str,
        port_to_connect: u32,
        _originator_address: &str,
        _originator_port: u32,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        let permitted_host = matches!(host_to_connect, "127.0.0.1" | "localhost" | "::1");
        if !permitted_host || port_to_connect != u32::from(self.forward_address.port()) {
            return Ok(());
        }
        let Ok(mut target) = tokio::net::TcpStream::connect(self.forward_address).await else {
            return Ok(());
        };
        reply.accept().await;
        tokio::spawn(async move {
            let mut stream = channel.into_stream();
            let _ = tokio::io::copy_bidirectional(&mut stream, &mut target).await;
        });
        Ok(())
    }
}

async fn run_protocol(channel: Channel<Msg>, mut application: ApplicationSession) -> Result<()> {
    let (read, mut write) = tokio::io::split(channel.into_stream());
    let mut lines = BufReader::new(read).lines();
    let mut interval = tokio::time::interval(Duration::from_millis(100));
    loop {
        tokio::select! {
            line = lines.next_line() => {
                let Some(line) = line? else { break };
                let response = if line.len() > 1024 * 1024 {
                    ServerMessage::Error { id: String::new(), code: "request_too_large".into(), message: "remote request exceeds 1 MiB".into() }
                } else {
                    match serde_json::from_str::<Request>(&line) {
                        Ok(request) => application.handle(request),
                        Err(error) => ServerMessage::Error { id: String::new(), code: "invalid_request".into(), message: error.to_string() },
                    }
                };
                write_message(&mut write, &response).await?;
            }
            _ = interval.tick() => {
                for message in application.pending() {
                    write_message(&mut write, &message).await?;
                }
            }
        }
    }
    Ok(())
}

async fn write_message(
    write: &mut (impl AsyncWrite + Unpin),
    message: &ServerMessage,
) -> Result<()> {
    let mut encoded = serde_json::to_vec(message)?;
    encoded.push(b'\n');
    write.write_all(&encoded).await?;
    write.flush().await?;
    Ok(())
}

async fn run_terminal_session(
    mut channel: Channel<Msg>,
    mut application: ApplicationSession,
) -> Result<()> {
    let locale = application.locale().to_owned();
    let hello = application.handle(Request {
        id: "terminal-hello".into(),
        command: Command::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    });
    if matches!(hello, ServerMessage::Error { .. }) {
        return Err(anyhow!("terminal protocol negotiation failed"));
    }
    let projects = application.handle(Request {
        id: "terminal-projects".into(),
        command: Command::ListProjects,
    });
    let project_names = match projects {
        ServerMessage::Response { result, .. } => result
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|project| project.get("name").and_then(|name| name.as_str()))
            .collect::<Vec<_>>()
            .join("\r\n  • "),
        _ => String::new(),
    };
    let banner = terminal_banner(&locale, &project_names);
    channel.data(banner.as_bytes()).await?;
    let mut interval = tokio::time::interval(Duration::from_millis(250));
    loop {
        tokio::select! {
            message = channel.wait() => match message {
                Some(russh::ChannelMsg::Data { data }) if data.iter().any(|byte| matches!(byte, b'q' | 3)) => {
                    channel.close().await?;
                    break;
                }
                None | Some(russh::ChannelMsg::Close) => break,
                _ => {}
            },
            _ = interval.tick() => {
                let updates = application.pending();
                if !updates.is_empty() {
                    let status = format!("\r\n[{} update{}]\r\n", updates.len(), if updates.len() == 1 { "" } else { "s" });
                    channel.data(status.as_bytes()).await?;
                }
            }
        }
    }
    Ok(())
}

fn terminal_banner(locale: &str, projects: &str) -> String {
    let locale = bds_core::i18n::normalize_language(locale);
    let title = bds_core::i18n::translate(locale, "remoteTerminal.serverTitle");
    let available = bds_core::i18n::translate(locale, "remoteTerminal.availableProjects");
    let quit = bds_core::i18n::translate(locale, "remoteTerminal.quit");
    let projects = if projects.is_empty() { "—" } else { projects };
    format!("\x1b[2J\x1b[H{title}\r\n\r\n{available}:\r\n  • {projects}\r\n\r\n{quit}\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::ClientKeyMaterial;
    use crate::client::{DesktopClient, RemoteTarget};
    use bds_core::db::Database;
    use std::fs;
    use std::thread;

    #[test]
    fn defaults_to_loopback_and_external_binding_requires_an_explicit_value() {
        let config = ServerConfig::local("db".into(), "data".into());
        assert_eq!(config.bind, IpAddr::V4(Ipv4Addr::LOCALHOST));
        assert_eq!(config.port, 2222);
    }

    #[test]
    fn terminal_banner_uses_the_server_locale_and_only_terminal_control_bytes() {
        let banner = terminal_banner("de", "Blog");
        assert!(banner.starts_with("\x1b[2J\x1b[H"));
        assert!(banner.contains("RuDS-Serversitzung"));
        assert!(banner.contains("Blog"));
        assert!(!banner.contains('{'));
    }

    #[test]
    fn real_ssh_authentication_revocation_reconnect_events_and_shutdown() {
        let root = tempfile::tempdir().unwrap();
        let server_data = root.path().join("server");
        let client_data = root.path().join("client");
        let unknown_data = root.path().join("unknown");
        fs::create_dir_all(&server_data).unwrap();
        let database_path = server_data.join("bds.db");
        let db = Database::open(&database_path).unwrap();
        db.migrate().unwrap();
        let project_dir = root.path().join("blog");
        let project = bds_core::engine::project::create_project(
            db.conn(),
            "Remote Blog",
            Some(project_dir.to_str().unwrap()),
        )
        .unwrap();
        bds_core::engine::settings::set(
            db.conn(),
            bds_core::engine::settings::UI_LANGUAGE_KEY,
            "fr",
        )
        .unwrap();

        let runtime = ServerRuntime::start(ServerConfig {
            database_path,
            data_root: server_data.clone(),
            bind: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 0,
            mcp_port: 0,
        })
        .unwrap();
        let target = RemoteTarget {
            user: "author".into(),
            host: runtime.address().ip().to_string(),
            port: runtime.address().port(),
        };

        let unknown = match DesktopClient::connect(target.clone(), &unknown_data) {
            Ok(_) => panic!("unknown key authenticated"),
            Err(error) => error,
        };
        assert!(
            unknown
                .to_string()
                .contains("public-key authentication failed")
        );

        let identity = ClientKeyMaterial::ensure(&client_data).unwrap();
        let public_key = fs::read_to_string(&identity.public_key_path).unwrap();
        fs::write(&runtime.key_material().authorized_keys_path, &public_key).unwrap();

        let first = DesktopClient::connect(target.clone(), &client_data).unwrap();
        let second = DesktopClient::connect(target.clone(), &client_data).unwrap();
        assert_eq!(first.server_locale(), "fr");
        assert_eq!(second.server_locale(), "fr");
        assert_eq!(first.list_projects().unwrap()[0].name, "Remote Blog");
        first.open_project(&project.id).unwrap();
        second.open_project(&project.id).unwrap();
        let _ = first.drain_events();
        let _ = second.drain_events();
        first
            .call(
                "posts",
                "create",
                vec![serde_json::json!({"title":"Over SSH","content":"body"})],
            )
            .unwrap();
        thread::sleep(Duration::from_millis(250));
        assert_eq!(domain_event_count(&first.drain_events(), &project.id), 1);
        assert_eq!(domain_event_count(&second.drain_events(), &project.id), 1);
        assert_eq!(
            bds_core::db::queries::post::list_posts_by_project(db.conn(), &project.id)
                .unwrap()
                .len(),
            1
        );

        // Revocation affects new authentication attempts immediately without
        // corrupting an already authenticated session.
        fs::write(&runtime.key_material().authorized_keys_path, "").unwrap();
        first.ping().unwrap();
        second.disconnect().unwrap();
        let revoked = match DesktopClient::connect(target.clone(), &client_data) {
            Ok(_) => panic!("revoked key authenticated"),
            Err(error) => error,
        };
        assert!(
            revoked
                .to_string()
                .contains("public-key authentication failed")
        );

        fs::write(&runtime.key_material().authorized_keys_path, public_key).unwrap();
        let reconnected = DesktopClient::connect(target, &client_data).unwrap();
        reconnected.ping().unwrap();
        reconnected.disconnect().unwrap();

        runtime.stop().unwrap();
        assert!(first.ping().is_err());
        first.disconnect().unwrap();
    }

    fn domain_event_count(messages: &[ServerMessage], project_id: &str) -> usize {
        messages
            .iter()
            .filter(|message| {
                matches!(message, ServerMessage::Event { event, .. } if event.project_id() == Some(project_id))
            })
            .count()
    }
}
