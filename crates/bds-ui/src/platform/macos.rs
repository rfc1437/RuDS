use std::path::PathBuf;
use std::sync::mpsc;

use crate::app::Message;

/// Events that arrive from macOS lifecycle hooks (openFile, openURLs).
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    FileOpen(PathBuf),
    UrlOpen(String),
}

/// Create the macOS lifecycle channel (sender goes to objc hooks, receiver polled by subscription).
pub fn lifecycle_channel() -> (mpsc::Sender<LifecycleEvent>, mpsc::Receiver<LifecycleEvent>) {
    mpsc::channel()
}

/// Poll the macOS lifecycle receiver for the next event and map to a Message.
pub fn poll_lifecycle(
    receiver: &mpsc::Receiver<LifecycleEvent>,
) -> Option<Message> {
    match receiver.try_recv() {
        Ok(LifecycleEvent::FileOpen(path)) => Some(Message::FileOpenRequested(path)),
        Ok(LifecycleEvent::UrlOpen(url)) => Some(Message::UrlOpenRequested(url)),
        Err(_) => None,
    }
}
