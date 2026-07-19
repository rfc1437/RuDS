use std::cell::RefCell;
use std::collections::BTreeMap;
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, OnceLock};

use crate::model::{DomainEntity, DomainEvent, NotificationAction};

#[derive(Default)]
struct BusState {
    next_id: u64,
    subscribers: BTreeMap<u64, Sender<DomainEvent>>,
}

#[derive(Default)]
struct BusInner {
    state: Mutex<BusState>,
    publish_lock: Mutex<()>,
}

/// Minimal deterministic in-process event bus. Delivery is synchronous and
/// ordered, while every subscriber owns an independent unbounded queue.
#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<BusInner>,
}

impl EventBus {
    pub fn subscribe(&self) -> EventSubscription {
        let (sender, receiver) = mpsc::channel();
        let _publish = lock(&self.inner.publish_lock);
        let mut state = lock(&self.inner.state);
        let id = state.next_id;
        state.next_id = state.next_id.wrapping_add(1);
        state.subscribers.insert(id, sender);
        EventSubscription {
            id: Some(id),
            inner: Arc::clone(&self.inner),
            receiver,
        }
    }

    pub fn publish(&self, event: DomainEvent) {
        let _publish = lock(&self.inner.publish_lock);
        let subscribers = lock(&self.inner.state)
            .subscribers
            .iter()
            .map(|(id, sender)| (*id, sender.clone()))
            .collect::<Vec<_>>();
        let disconnected = subscribers
            .into_iter()
            .filter_map(|(id, sender)| sender.send(event.clone()).is_err().then_some(id))
            .collect::<Vec<_>>();
        if !disconnected.is_empty() {
            let mut state = lock(&self.inner.state);
            for id in disconnected {
                state.subscribers.remove(&id);
            }
        }
    }
}

pub struct EventSubscription {
    id: Option<u64>,
    inner: Arc<BusInner>,
    receiver: Receiver<DomainEvent>,
}

impl EventSubscription {
    pub fn drain(&self) -> Vec<DomainEvent> {
        self.receiver.try_iter().collect()
    }

    pub fn unsubscribe(mut self) {
        self.detach();
    }

    fn detach(&mut self) {
        if let Some(id) = self.id.take() {
            let _publish = lock(&self.inner.publish_lock);
            lock(&self.inner.state).subscribers.remove(&id);
        }
    }
}

impl Drop for EventSubscription {
    fn drop(&mut self) {
        self.detach();
    }
}

fn lock<T>(mutex: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn global_bus() -> &'static EventBus {
    static BUS: OnceLock<EventBus> = OnceLock::new();
    BUS.get_or_init(EventBus::default)
}

thread_local! {
    static CAPTURED_EVENTS: RefCell<Option<Vec<DomainEvent>>> = const { RefCell::new(None) };
}

pub fn subscribe() -> EventSubscription {
    global_bus().subscribe()
}

pub fn publish(event: DomainEvent) {
    CAPTURED_EVENTS.with(|captured| {
        if let Some(events) = captured.borrow_mut().as_mut() {
            events.push(event.clone());
        }
    });
    global_bus().publish(event);
}

pub fn entity_changed(
    project_id: &str,
    entity: DomainEntity,
    entity_id: &str,
    action: NotificationAction,
) {
    publish(DomainEvent::EntityChanged {
        project_id: project_id.to_string(),
        entity,
        entity_id: entity_id.to_string(),
        action,
    });
}

pub fn settings_changed(project_id: Option<&str>, key: &str) {
    publish(DomainEvent::SettingsChanged {
        project_id: project_id.map(str::to_string),
        key: key.to_string(),
    });
}

pub(crate) fn capture_current_thread<T>(
    operation: impl FnOnce() -> T,
) -> Result<(T, Vec<DomainEvent>), &'static str> {
    let started = CAPTURED_EVENTS.with(|captured| {
        let mut captured = captured.borrow_mut();
        if captured.is_some() {
            false
        } else {
            *captured = Some(Vec::new());
            true
        }
    });
    if !started {
        return Err("nested CLI event capture is not supported");
    }
    let mut reset = CaptureReset::default();
    let result = operation();
    let events = CAPTURED_EVENTS.with(|captured| captured.borrow_mut().take().unwrap_or_default());
    reset.finished = true;
    Ok((result, events))
}

#[derive(Default)]
struct CaptureReset {
    finished: bool,
}

impl Drop for CaptureReset {
    fn drop(&mut self) {
        if !self.finished {
            CAPTURED_EVENTS.with(|captured| {
                captured.borrow_mut().take();
            });
        }
    }
}
