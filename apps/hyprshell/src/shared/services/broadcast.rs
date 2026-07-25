//! The shared-source primitive every system service is built on.
//!
//! A service owns exactly one producer — a D-Bus subscription, a socket, a watcher process — running on its own
//! thread for the whole shell. Surfaces don't read the system; they subscribe, and the producer fans each
//! reading out to all of them. N bars therefore cost one connection and one parse per change, not N, and a
//! surface never runs a timer of its own.
//!
//! A module consumes one by handing [`Service::subscribe`] to `platform_layershell::watch`, which delivers each
//! value on that surface's own loop thread and unsubscribes it when the surface goes away.

use std::sync::{Arc, Mutex, OnceLock};

use platform_layershell::EventSender;

/// The current reading plus the surfaces listening for the next one.
pub struct Broadcast<T> {
    current: Mutex<Option<T>>,
    subscribers: Mutex<Vec<EventSender<T>>>,
}

impl<T: Clone> Broadcast<T> {
    fn new() -> Self {
        Self {
            current: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
        }
    }

    /// Records `value` as the current reading and fans it out, dropping subscribers whose surface has closed
    /// (their channel receiver is gone, so `send` fails).
    pub fn publish(&self, value: T) {
        *self.current.lock().unwrap() = Some(value.clone());
        self.subscribers
            .lock()
            .unwrap()
            .retain(|tx| tx.send(value.clone()));
    }

    pub fn current(&self) -> Option<T> {
        self.current.lock().unwrap().clone()
    }
}

/// A lazily-started shared service. The producer thread spins up on the first subscription and lives for the
/// process, so a shell configured without a battery chip never opens a UPower connection.
pub struct Service<T: 'static> {
    cell: OnceLock<Arc<Broadcast<T>>>,
    producer: fn(&Broadcast<T>),
    thread_name: &'static str,
}

impl<T: Clone + Send + 'static> Service<T> {
    pub const fn new(thread_name: &'static str, producer: fn(&Broadcast<T>)) -> Self {
        Self {
            cell: OnceLock::new(),
            producer,
            thread_name,
        }
    }

    fn started(&'static self) -> &'static Arc<Broadcast<T>> {
        self.cell.get_or_init(|| {
            let broadcast = Arc::new(Broadcast::new());
            let producer = self.producer;
            let owned = Arc::clone(&broadcast);
            let _ = std::thread::Builder::new()
                .name(self.thread_name.to_string())
                .spawn(move || producer(&owned));
            broadcast
        })
    }

    /// Registers `tx` for live readings, sending the current one immediately so the surface starts in sync
    /// rather than blank until the next change. Pass this as the producer to `platform_layershell::watch`.
    pub fn subscribe(&'static self, tx: EventSender<T>) {
        let service = self.started();
        if let Some(value) = service.current()
            && !tx.send(value)
        {
            return;
        }
        service.subscribers.lock().unwrap().push(tx);
    }

    /// Publishes a reading taken outside the producer thread — used when the shell itself causes the change (a
    /// mute toggle, a brightness step) so the UI reflects it immediately instead of at the producer's next turn.
    pub fn publish(&'static self, value: T) {
        self.started().publish(value);
    }

    /// The last published reading, without touching the system. Lets a UI handler act on the current value (a
    /// scroll stepping from it) without doing blocking I/O — or spawning a process — on the render thread.
    pub fn current(&'static self) -> Option<T> {
        self.started().current()
    }
}
