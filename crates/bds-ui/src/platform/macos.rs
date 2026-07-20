use std::path::PathBuf;
use std::sync::mpsc;

use objc2::rc::Retained;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel};
use objc2_foundation::{NSAppleEventDescriptor, NSAppleEventManager, NSObject, NSObjectProtocol};

use crate::app::Message;

/// Events that arrive from macOS lifecycle hooks (openFile, openURLs).
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    FileOpen(PathBuf),
    UrlOpen(String),
}

/// Ivars for the Apple-event handler — holds the lifecycle sender.
#[derive(Debug)]
pub struct HandlerIvars {
    tx: mpsc::Sender<LifecycleEvent>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. BdsAppleEventHandler does not impl Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BdsAppleEventHandler"]
    #[ivars = HandlerIvars]
    pub struct BdsAppleEventHandler;

    // SAFETY: NSObjectProtocol declares no additional safety requirements.
    unsafe impl NSObjectProtocol for BdsAppleEventHandler {}

    impl BdsAppleEventHandler {
        #[unsafe(method(handleGetURLEvent:withReplyEvent:))]
        fn handle_get_url_event(
            &self,
            event: &NSAppleEventDescriptor,
            _reply: &NSAppleEventDescriptor,
        ) {
            if let Some(url) = event
                .paramDescriptorForKeyword(four_char_code(*b"----"))
                .and_then(|descriptor| descriptor.stringValue())
            {
                let _ = self
                    .ivars()
                    .tx
                    .send(LifecycleEvent::UrlOpen(url.to_string()));
            }
        }
    }
);

impl BdsAppleEventHandler {
    fn new(mtm: MainThreadMarker, tx: mpsc::Sender<LifecycleEvent>) -> Retained<Self> {
        let this = Self::alloc(mtm);
        let this = this.set_ivars(HandlerIvars { tx });
        // SAFETY: `this` has initialized ivars and NSObject's `init` accepts ownership of the
        // allocated receiver, returning the retained initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

const fn four_char_code(bytes: [u8; 4]) -> u32 {
    u32::from_be_bytes(bytes)
}

pub fn lifecycle_channel() -> (mpsc::Sender<LifecycleEvent>, mpsc::Receiver<LifecycleEvent>) {
    mpsc::channel()
}

/// Install a Get URL Apple-event handler without replacing Winit's application delegate.
///
/// Must be called after Iced has created its event loop. The returned handler must remain alive.
pub fn install_lifecycle_handler() -> Option<(
    Retained<BdsAppleEventHandler>,
    mpsc::Receiver<LifecycleEvent>,
)> {
    let mtm = MainThreadMarker::new()?;
    let (tx, receiver) = lifecycle_channel();
    let handler = BdsAppleEventHandler::new(mtm, tx);
    let manager = NSAppleEventManager::sharedAppleEventManager();
    // SAFETY: The selector is implemented above with the two Apple-event descriptor arguments
    // required by NSAppleEventManager. `handler` is retained by the caller for the app lifetime.
    unsafe {
        manager.setEventHandler_andSelector_forEventClass_andEventID(
            &handler,
            sel!(handleGetURLEvent:withReplyEvent:),
            four_char_code(*b"GURL"),
            four_char_code(*b"GURL"),
        );
    }
    Some((handler, receiver))
}

/// Poll the macOS lifecycle receiver for the next event and map to a Message.
pub fn poll_lifecycle(receiver: &mpsc::Receiver<LifecycleEvent>) -> Option<Message> {
    match receiver.try_recv() {
        Ok(LifecycleEvent::FileOpen(path)) => Some(Message::FileOpenRequested(path)),
        Ok(LifecycleEvent::UrlOpen(url)) => Some(Message::UrlOpenRequested(url)),
        Err(_) => None,
    }
}

pub fn drain_lifecycle(receiver: &mpsc::Receiver<LifecycleEvent>) -> Vec<Message> {
    std::iter::from_fn(|| poll_lifecycle(receiver)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drains_cold_start_events_in_arrival_order() {
        let (sender, receiver) = lifecycle_channel();
        sender
            .send(LifecycleEvent::UrlOpen("ruds://new-post?title=One".into()))
            .unwrap();
        sender
            .send(LifecycleEvent::UrlOpen("ruds://new-post?title=Two".into()))
            .unwrap();

        let messages = drain_lifecycle(&receiver);
        assert!(matches!(
            &messages[..],
            [Message::UrlOpenRequested(one), Message::UrlOpenRequested(two)]
                if one.ends_with("One") && two.ends_with("Two")
        ));
    }
}
