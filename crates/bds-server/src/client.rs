use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::Duration;

use anyhow::{Context, Result, anyhow, bail};
use bds_core::model::Project;
use russh::ChannelMsg;
use russh::client;
use russh::keys::key::PrivateKeyWithHashAlg;
use russh::keys::{PublicKey, load_secret_key};
use serde_json::Value;
use tokio::sync::mpsc;
use uuid::Uuid;

use crate::auth::ClientKeyMaterial;
use crate::protocol::{Command, PROTOCOL_VERSION, Request, SUBSYSTEM, ServerMessage};

const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteTarget {
    pub user: String,
    pub host: String,
    pub port: u16,
}

impl RemoteTarget {
    pub fn parse(value: &str) -> Result<Self> {
        let (user, address) = value
            .trim()
            .split_once('@')
            .filter(|(user, address)| !user.is_empty() && !address.is_empty())
            .ok_or_else(|| anyhow!("use the form user@host or user@host:port"))?;
        let (host, port) = if let Some(address) = address.strip_prefix('[') {
            let (host, suffix) = address
                .split_once(']')
                .ok_or_else(|| anyhow!("invalid bracketed host"))?;
            let port = match suffix.strip_prefix(':') {
                Some(value) => parse_port(value)?,
                None if suffix.is_empty() => 2222,
                None => bail!("invalid host and port"),
            };
            (host, port)
        } else if address.matches(':').count() == 1 {
            let (host, port) = address.rsplit_once(':').expect("one colon");
            (host, parse_port(port)?)
        } else {
            (address, 2222)
        };
        if host.is_empty() {
            bail!("remote host is required");
        }
        Ok(Self {
            user: user.to_owned(),
            host: host.to_owned(),
            port,
        })
    }

    pub fn label(&self) -> String {
        if self.port == 2222 {
            format!("{}@{}", self.user, self.host)
        } else if self.host.contains(':') {
            format!("{}@[{}]:{}", self.user, self.host, self.port)
        } else {
            format!("{}@{}:{}", self.user, self.host, self.port)
        }
    }
}

fn parse_port(value: &str) -> Result<u16> {
    value
        .parse::<u16>()
        .ok()
        .filter(|port| *port > 0)
        .ok_or_else(|| anyhow!("remote SSH port is invalid"))
}

enum ClientCommand {
    Request {
        request: Request,
        response: std::sync::mpsc::Sender<Result<Value, String>>,
    },
    Stop,
}

pub struct DesktopClient {
    target: RemoteTarget,
    server_locale: String,
    commands: mpsc::UnboundedSender<ClientCommand>,
    events: Arc<Mutex<Vec<ServerMessage>>>,
    thread: Option<JoinHandle<()>>,
}

impl std::fmt::Debug for DesktopClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("DesktopClient")
            .field("target", &self.target)
            .field("server_locale", &self.server_locale)
            .finish_non_exhaustive()
    }
}

impl DesktopClient {
    pub fn connect(target: RemoteTarget, data_root: &Path) -> Result<Self> {
        let keys = ClientKeyMaterial::ensure(data_root)?;
        let (commands, command_rx) = mpsc::unbounded_channel();
        let events = Arc::new(Mutex::new(Vec::new()));
        let event_sink = Arc::clone(&events);
        let error_sink = Arc::clone(&events);
        let thread_target = target.clone();
        let (started_tx, started_rx) = std::sync::mpsc::sync_channel(1);
        let thread = std::thread::Builder::new()
            .name("bds-remote-client".into())
            .spawn(move || {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build();
                let result = match runtime {
                    Ok(runtime) => runtime.block_on(run_client(
                        thread_target,
                        keys,
                        command_rx,
                        event_sink,
                        started_tx,
                    )),
                    Err(error) => {
                        let _ = started_tx.send(Err(error.to_string()));
                        return;
                    }
                };
                if let Err(error) = result {
                    lock(&error_sink).push(ServerMessage::Error {
                        id: String::new(),
                        code: "connection_lost".into(),
                        message: error.to_string(),
                    });
                }
            })?;
        match started_rx.recv_timeout(CONNECT_TIMEOUT) {
            Ok(Ok(server_locale)) => {
                let client = Self {
                    target,
                    server_locale,
                    commands,
                    events,
                    thread: Some(thread),
                };
                // Prove the selected endpoint speaks the RuDS protocol before
                // exposing the connection to the desktop.
                client.list_projects()?;
                Ok(client)
            }
            Ok(Err(error)) => {
                let _ = thread.join();
                Err(anyhow!(error))
            }
            Err(_) => {
                let _ = commands.send(ClientCommand::Stop);
                let _ = thread.join();
                bail!("timed out connecting to the RuDS server")
            }
        }
    }

    pub fn target(&self) -> &RemoteTarget {
        &self.target
    }

    pub fn server_locale(&self) -> &str {
        &self.server_locale
    }

    pub fn list_projects(&self) -> Result<Vec<Project>> {
        let value = self.request(Command::ListProjects)?;
        serde_json::from_value(value).context("server returned an invalid project list")
    }

    pub fn open_project(&self, project_id: &str) -> Result<Project> {
        let value = self.request(Command::OpenProject {
            project_id: project_id.to_owned(),
        })?;
        serde_json::from_value(value).context("server returned an invalid project")
    }

    pub fn call(&self, namespace: &str, method: &str, arguments: Vec<Value>) -> Result<Value> {
        self.request(Command::Call {
            namespace: namespace.to_owned(),
            method: method.to_owned(),
            arguments,
        })
    }

    pub fn ping(&self) -> Result<()> {
        self.request(Command::Ping).map(|_| ())
    }

    pub fn drain_events(&self) -> Vec<ServerMessage> {
        std::mem::take(&mut *lock(&self.events))
    }

    pub fn close(&self) {
        let _ = self.commands.send(ClientCommand::Stop);
    }

    pub fn disconnect(mut self) -> Result<()> {
        let _ = self.commands.send(ClientCommand::Stop);
        if let Some(thread) = self.thread.take() {
            thread
                .join()
                .map_err(|_| anyhow!("remote client thread panicked"))?;
        }
        Ok(())
    }

    fn request(&self, command: Command) -> Result<Value> {
        let (response, receiver) = std::sync::mpsc::channel();
        self.commands
            .send(ClientCommand::Request {
                request: Request {
                    id: Uuid::new_v4().to_string(),
                    command,
                },
                response,
            })
            .map_err(|_| anyhow!("remote connection is closed"))?;
        match receiver.recv_timeout(REQUEST_TIMEOUT) {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(error)) => Err(anyhow!(error)),
            Err(_) => bail!("remote request timed out"),
        }
    }
}

impl Drop for DesktopClient {
    fn drop(&mut self) {
        let _ = self.commands.send(ClientCommand::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[derive(Clone)]
struct HostVerifier {
    host: String,
    port: u16,
    path: PathBuf,
    error: Arc<Mutex<Option<String>>>,
}

impl client::Handler for HostVerifier {
    type Error = anyhow::Error;

    async fn check_server_key(&mut self, key: &PublicKey) -> Result<bool, Self::Error> {
        match russh::keys::known_hosts::check_known_hosts_path(
            &self.host, self.port, key, &self.path,
        ) {
            Ok(true) => Ok(true),
            Ok(false) => {
                russh::keys::known_hosts::learn_known_hosts_path(
                    &self.host, self.port, key, &self.path,
                )?;
                set_private_mode(&self.path)?;
                Ok(true)
            }
            Err(error) => {
                *lock(&self.error) = Some(match error {
                    russh::keys::Error::KeyChanged { .. } => {
                        "the remote server host key changed; remove its known_hosts entry only if the change is trusted".into()
                    }
                    _ => format!("could not verify the remote server host key: {error}"),
                });
                Ok(false)
            }
        }
    }
}

async fn run_client(
    target: RemoteTarget,
    keys: ClientKeyMaterial,
    commands: mpsc::UnboundedReceiver<ClientCommand>,
    events: Arc<Mutex<Vec<ServerMessage>>>,
    started: std::sync::mpsc::SyncSender<Result<String, String>>,
) -> Result<()> {
    let verification_error = Arc::new(Mutex::new(None));
    let verifier = HostVerifier {
        host: target.host.clone(),
        port: target.port,
        path: keys.known_hosts_path.clone(),
        error: Arc::clone(&verification_error),
    };
    let config = Arc::new(client::Config {
        inactivity_timeout: Some(Duration::from_secs(60 * 60)),
        nodelay: true,
        ..Default::default()
    });
    let mut ssh = match client::connect(config, (target.host.as_str(), target.port), verifier).await
    {
        Ok(ssh) => ssh,
        Err(error) => {
            let reason = lock(&verification_error)
                .take()
                .unwrap_or_else(|| format!("SSH connection failed: {error}"));
            let _ = started.send(Err(reason.clone()));
            bail!(reason);
        }
    };
    let private_key = load_secret_key(&keys.private_key_path, None)?;
    let hash = ssh.best_supported_rsa_hash().await?.flatten();
    let authentication = ssh
        .authenticate_publickey(
            &target.user,
            PrivateKeyWithHashAlg::new(Arc::new(private_key), hash),
        )
        .await?;
    if !authentication.success() {
        let message = format!(
            "public-key authentication failed; add {} to the server authorized_keys file",
            keys.public_key_path.display()
        );
        let _ = started.send(Err(message.clone()));
        bail!(message);
    }
    let mut channel = ssh.channel_open_session().await?;
    channel.request_subsystem(true, SUBSYSTEM).await?;
    let hello = Request {
        id: Uuid::new_v4().to_string(),
        command: Command::Hello {
            protocol_version: PROTOCOL_VERSION,
        },
    };
    let encoded = encode_request(&hello)?;
    channel.data(&encoded[..]).await?;
    let mut bytes = Vec::new();
    loop {
        let Some(message) = channel.wait().await else {
            let _ = started.send(Err("server closed during protocol negotiation".into()));
            bail!("server closed during protocol negotiation");
        };
        if let ChannelMsg::Data { data } = message {
            bytes.extend_from_slice(&data);
            for message in decode_messages(&mut bytes)? {
                match message {
                    ServerMessage::Response { id, result } if id == hello.id => {
                        let Some(locale) = result.get("locale").and_then(Value::as_str) else {
                            let message = "server hello did not include its locale".to_owned();
                            let _ = started.send(Err(message.clone()));
                            bail!(message);
                        };
                        let locale = locale.to_owned();
                        let _ = started.send(Ok(locale));
                        return client_loop(ssh, channel, commands, events, bytes).await;
                    }
                    ServerMessage::Error { id, message, .. } if id == hello.id => {
                        let _ = started.send(Err(message.clone()));
                        bail!(message);
                    }
                    other => lock(&events).push(other),
                }
            }
        }
    }
}

async fn client_loop(
    ssh: client::Handle<HostVerifier>,
    mut channel: russh::Channel<client::Msg>,
    mut commands: mpsc::UnboundedReceiver<ClientCommand>,
    events: Arc<Mutex<Vec<ServerMessage>>>,
    mut bytes: Vec<u8>,
) -> Result<()> {
    let mut pending = HashMap::<String, std::sync::mpsc::Sender<Result<Value, String>>>::new();
    loop {
        tokio::select! {
            command = commands.recv() => match command {
                Some(ClientCommand::Request { request, response }) => {
                    pending.insert(request.id.clone(), response);
                    let encoded = encode_request(&request)?;
                    if let Err(error) = channel.data(&encoded[..]).await {
                        fail_pending(&mut pending, &format!("remote connection write failed: {error}"));
                        return Err(error.into());
                    }
                }
                Some(ClientCommand::Stop) | None => {
                    let _ = channel.eof().await;
                    let _ = ssh.disconnect(russh::Disconnect::ByApplication, "", "en").await;
                    fail_pending(&mut pending, "remote connection closed");
                    return Ok(());
                }
            },
            message = channel.wait() => match message {
                Some(ChannelMsg::Data { data }) => {
                    bytes.extend_from_slice(&data);
                    for message in decode_messages(&mut bytes)? {
                        match message {
                            ServerMessage::Response { ref id, ref result } => {
                                if let Some(response) = pending.remove(id) {
                                    let _ = response.send(Ok(result.clone()));
                                } else {
                                    lock(&events).push(message);
                                }
                            }
                            ServerMessage::Error { ref id, message: ref error_message, .. } => {
                                if let Some(response) = pending.remove(id) {
                                    let _ = response.send(Err(error_message.clone()));
                                } else {
                                    lock(&events).push(message.clone());
                                }
                            }
                            other => lock(&events).push(other),
                        }
                    }
                }
                Some(ChannelMsg::Close | ChannelMsg::Eof) | None => {
                    fail_pending(&mut pending, "remote server disconnected");
                    bail!("remote server disconnected");
                }
                _ => {}
            }
        }
    }
}

fn encode_request(request: &Request) -> Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec(request)?;
    bytes.push(b'\n');
    Ok(bytes)
}

fn decode_messages(bytes: &mut Vec<u8>) -> Result<Vec<ServerMessage>> {
    let mut messages = Vec::new();
    while let Some(newline) = bytes.iter().position(|byte| *byte == b'\n') {
        let line = bytes.drain(..=newline).collect::<Vec<_>>();
        if line.len() > 1 {
            messages.push(serde_json::from_slice(&line[..line.len() - 1])?);
        }
    }
    if bytes.len() > 1024 * 1024 {
        bail!("remote response exceeds 1 MiB");
    }
    Ok(messages)
}

fn fail_pending(
    pending: &mut HashMap<String, std::sync::mpsc::Sender<Result<Value, String>>>,
    message: &str,
) {
    for (_, response) in pending.drain() {
        let _ = response.send(Err(message.to_owned()));
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

#[cfg(unix)]
fn set_private_mode(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt as _;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    Ok(())
}

#[cfg(not(unix))]
fn set_private_mode(_path: &Path) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_default_explicit_and_ipv6_targets() {
        assert_eq!(
            RemoteTarget::parse(" gb@blog.example ").unwrap(),
            RemoteTarget {
                user: "gb".into(),
                host: "blog.example".into(),
                port: 2222,
            }
        );
        assert_eq!(RemoteTarget::parse("gb@host:2022").unwrap().port, 2022);
        assert_eq!(RemoteTarget::parse("gb@[::1]:2200").unwrap().host, "::1");
        assert!(RemoteTarget::parse("host").is_err());
        assert!(RemoteTarget::parse("@host").is_err());
        assert!(RemoteTarget::parse("user@host:nope").is_err());
    }
}
