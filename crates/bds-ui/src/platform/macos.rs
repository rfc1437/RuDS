use std::path::PathBuf;
use std::sync::mpsc;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSApplicationDelegate};
use objc2_foundation::{NSArray, NSNotification, NSObject, NSObjectProtocol, NSString, NSURL};

use crate::app::Message;

/// Events that arrive from macOS lifecycle hooks (openFile, openURLs).
#[derive(Debug, Clone)]
pub enum LifecycleEvent {
    FileOpen(PathBuf),
    UrlOpen(String),
}

/// Ivars for the delegate — holds the sender side of the lifecycle channel.
#[derive(Debug)]
pub struct DelegateIvars {
    tx: mpsc::Sender<LifecycleEvent>,
}

define_class!(
    // SAFETY: NSObject has no subclassing requirements. BdsAppDelegate does not impl Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "BdsAppDelegate"]
    #[ivars = DelegateIvars]
    pub struct BdsAppDelegate;

    // SAFETY: NSObjectProtocol declares no additional safety requirements.
    unsafe impl NSObjectProtocol for BdsAppDelegate {}

    // SAFETY: Every exported selector below uses the exact AppKit delegate signature and the
    // class is main-thread-only, as required by NSApplicationDelegate.
    unsafe impl NSApplicationDelegate for BdsAppDelegate {
        #[unsafe(method(applicationDidFinishLaunching:))]
        fn did_finish_launching(&self, _notification: &NSNotification) {
            // App is already running via Iced; nothing to do here.
        }

        #[unsafe(method(application:openFile:))]
        fn application_open_file(&self, _sender: &NSApplication, filename: &NSString) -> bool {
            let path = PathBuf::from(filename.to_string());
            let _ = self.ivars().tx.send(LifecycleEvent::FileOpen(path));
            true
        }

        #[unsafe(method(application:openURLs:))]
        fn application_open_urls(&self, _sender: &NSApplication, urls: &NSArray<NSURL>) {
            for i in 0..urls.len() {
                if let Some(url) = urls.objectAtIndex(i).absoluteString() {
                    let _ = self
                        .ivars()
                        .tx
                        .send(LifecycleEvent::UrlOpen(url.to_string()));
                }
            }
        }
    }
);

impl BdsAppDelegate {
    fn new(mtm: MainThreadMarker, tx: mpsc::Sender<LifecycleEvent>) -> Retained<Self> {
        let this = Self::alloc(mtm);
        let this = this.set_ivars(DelegateIvars { tx });
        // SAFETY: `this` has initialized ivars and NSObject's `init` accepts ownership of the
        // allocated receiver, returning the retained initialized object.
        unsafe { msg_send![super(this), init] }
    }
}

/// Create the macOS lifecycle channel and wire the native delegate.
///
/// Returns `(Retained<BdsAppDelegate>, Receiver)`. The delegate must be kept alive
/// for the lifetime of the application (drop it and the callbacks stop).
pub fn lifecycle_channel() -> (mpsc::Sender<LifecycleEvent>, mpsc::Receiver<LifecycleEvent>) {
    mpsc::channel()
}

/// Install the native Objective-C delegate on NSApplication, wiring it to the given sender.
///
/// Must be called from the main thread after the Iced application is running.
/// Returns the retained delegate (caller must keep it alive).
pub fn install_delegate(tx: mpsc::Sender<LifecycleEvent>) -> Option<Retained<BdsAppDelegate>> {
    let mtm = MainThreadMarker::new()?;
    let delegate = BdsAppDelegate::new(mtm, tx);
    let app = NSApplication::sharedApplication(mtm);
    let object = ProtocolObject::from_ref(&*delegate);
    app.setDelegate(Some(object));
    Some(delegate)
}

/// Poll the macOS lifecycle receiver for the next event and map to a Message.
pub fn poll_lifecycle(receiver: &mpsc::Receiver<LifecycleEvent>) -> Option<Message> {
    match receiver.try_recv() {
        Ok(LifecycleEvent::FileOpen(path)) => Some(Message::FileOpenRequested(path)),
        Ok(LifecycleEvent::UrlOpen(url)) => Some(Message::UrlOpenRequested(url)),
        Err(_) => None,
    }
}
