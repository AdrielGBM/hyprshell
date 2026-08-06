//! The shared-source primitive every system service is built on.
//!
//! A service owns exactly one producer — a D-Bus subscription, a socket, a watcher process — running on its own
//! thread for the whole shell. Surfaces don't read the system; they subscribe, and the producer fans each
//! reading out to all of them. N bars therefore cost one connection and one parse per change, not N, and a
//! surface never runs a timer of its own.
//!
//! A module consumes one by handing [`Service::subscribe`] to `platform_wayland::watch`, which delivers each
//! value on that surface's own loop thread and unsubscribes it when the surface goes away.

use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use platform_wayland::EventSender;

/// The current reading plus the surfaces listening for the next one.
pub struct Broadcast<T> {
    current: Mutex<Option<T>>,
    subscribers: Mutex<Vec<EventSender<T>>>,
    /// Plain-channel listeners, for another *producer* thread rather than a surface.
    ///
    /// A surface reads through `EventSender`, which only the driver can hand out — so a producer that has to
    /// react to a service instead of to the system had no way to wait for one, and could only poll. The
    /// wallpaper surface is the case that needs it: the moment the choice changes, something has to decode a
    /// full-resolution image, and that something must be neither the UI thread nor a timer.
    listeners: Mutex<Vec<mpsc::Sender<T>>>,
}

impl<T: Clone> Broadcast<T> {
    fn new() -> Self {
        Self {
            current: Mutex::new(None),
            subscribers: Mutex::new(Vec::new()),
            listeners: Mutex::new(Vec::new()),
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
        self.listeners
            .lock()
            .unwrap()
            .retain(|tx| tx.send(value.clone()).is_ok());
    }

    pub fn current(&self) -> Option<T> {
        self.current.lock().unwrap().clone()
    }
}

/// A lazily-started shared service. The producer thread spins up on the first subscription and lives for the
/// process, so a shell configured without a battery chip never opens a UPower connection.
///
/// The producer receives the broadcast as an `Arc`, not a borrow, so it can either park on a loop of its own or
/// hand a clone to a callback and return — which is what a service reading off an event stream someone else
/// owns has to do. Either way the broadcast outlives the producer.
pub struct Service<T: 'static> {
    cell: OnceLock<Arc<Broadcast<T>>>,
    producer: fn(&Arc<Broadcast<T>>),
    thread_name: &'static str,
}

impl<T: Clone + Send + 'static> Service<T> {
    pub const fn new(thread_name: &'static str, producer: fn(&Arc<Broadcast<T>>)) -> Self {
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
    /// rather than blank until the next change. Pass this as the producer to `platform_wayland::watch`.
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

    /// The last published reading, waiting up to `patience` for the first one when the producer has only just
    /// started.
    ///
    /// **[`current`](Self::current) starts the producer and reads in the same breath**, which is right for a UI
    /// handler — it either has a reading to step from or has nothing to draw — and wrong for a caller that is
    /// itself the reason the service started. An IPC `volume get` on a shell whose bar carries no volume chip
    /// asked a listener that had not had a turn yet, got `None`, and answered "no audio sink available" about a
    /// machine with one; `volume up` did nothing at all and said it had.
    ///
    /// Polled rather than signalled because the wait only ever happens once, on the first read of a service
    /// nothing had subscribed to, and a condvar per broadcast is a lot of machinery for that.
    pub fn awaited(&'static self, patience: Duration) -> Option<T> {
        let service = self.started();
        let deadline = Instant::now() + patience;
        loop {
            if let Some(value) = service.current() {
                return Some(value);
            }
            if Instant::now() >= deadline {
                return None;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
    }

    /// Records `value` as the current reading *without* starting the producer, so a reader sees it and the
    /// thread that would have read the system never runs.
    ///
    /// What a `[preview]` fixture seeds a service with. [`publish`](Self::publish) is the wrong door for that:
    /// it starts the producer, which for the tray means claiming a D-Bus name and then overwriting the seeded
    /// reading with the machine's own — so a preview would draw whatever happens to be running.
    pub fn seed(&'static self, value: T) {
        self.cell
            .get_or_init(|| Arc::new(Broadcast::new()))
            .publish(value);
    }
}

/// A producerless shared value: the same one-writer/N-reader fan-out as [`Service`], for state the shell itself
/// owns — persisted toggles, the current wallpaper, launch counts — rather than reads off the system. Nothing
/// polls and no thread is spawned; the value is seeded by `init` on first touch and changed by [`Store::update`].
pub struct Store<T: 'static> {
    cell: OnceLock<Arc<Broadcast<T>>>,
    init: fn() -> T,
}

impl<T: Clone + Send + 'static> Store<T> {
    pub const fn new(init: fn() -> T) -> Self {
        Self {
            cell: OnceLock::new(),
            init,
        }
    }

    fn started(&'static self) -> &'static Arc<Broadcast<T>> {
        self.cell.get_or_init(|| {
            let broadcast = Arc::new(Broadcast::new());
            *broadcast.current.lock().unwrap() = Some((self.init)());
            broadcast
        })
    }

    /// The current value, seeding it on first call.
    pub fn get(&'static self) -> T {
        self.started()
            .current()
            .expect("a store is seeded when it starts")
    }

    /// Applies `change` to the current value and fans the result out. Returns the new value so a caller can
    /// persist it without a second read racing another writer.
    pub fn update(&'static self, change: impl FnOnce(&mut T)) -> T {
        let broadcast = self.started();
        let mut next = broadcast
            .current()
            .expect("a store is seeded when it starts");
        change(&mut next);
        broadcast.publish(next.clone());
        next
    }

    /// Registers `tx` for changes, sending the current value immediately so a surface starts in sync.
    pub fn subscribe(&'static self, tx: EventSender<T>) {
        let broadcast = self.started();
        if let Some(value) = broadcast.current()
            && !tx.send(value)
        {
            return;
        }
        broadcast.subscribers.lock().unwrap().push(tx);
    }

    /// Registers a plain channel for changes, sending the current value immediately. For a producer thread
    /// that has to *wait* on this store rather than poll it; a surface wants [`subscribe`](Self::subscribe).
    pub fn listen(&'static self, tx: mpsc::Sender<T>) {
        let broadcast = self.started();
        if let Some(value) = broadcast.current()
            && tx.send(value).is_err()
        {
            return;
        }
        broadcast.listeners.lock().unwrap().push(tx);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicBool, Ordering};

    use super::*;

    static COUNTER: Store<u32> = Store::new(|| 7);

    static PRODUCER_RAN: AtomicBool = AtomicBool::new(false);
    static SEEDED: Service<u32> = Service::new("test-seeded", |broadcast| {
        PRODUCER_RAN.store(true, Ordering::SeqCst);
        broadcast.publish(0);
    });

    #[test]
    fn a_seeded_service_answers_readers_without_starting_its_producer() {
        SEEDED.seed(9);
        assert_eq!(SEEDED.current(), Some(9), "a reader sees the seeded value");
        assert!(
            !PRODUCER_RAN.load(Ordering::SeqCst),
            "the thread that would read the system never started"
        );
    }

    #[test]
    fn store_seeds_from_init_and_updates_in_place() {
        assert_eq!(COUNTER.get(), 7, "seeded lazily from `init`");
        assert_eq!(
            COUNTER.update(|n| *n += 5),
            12,
            "update returns the new value"
        );
        assert_eq!(COUNTER.get(), 12, "and it is what later readers see");
    }

    /// **A caller that starts a service is the one caller that cannot use [`Service::current`].**
    ///
    /// `current` starts the producer and reads in the same breath, so the very first read of a service nothing
    /// had subscribed to is always `None` — which reached the user as `volume get` answering "no audio sink
    /// available" on a machine with one, and `volume up` doing nothing and reporting the step it had not taken.
    #[test]
    fn the_first_read_of_a_cold_service_waits_for_it_rather_than_answering_nothing() {
        static SLOW: Service<u8> = Service::new("test-slow", |out| {
            std::thread::sleep(Duration::from_millis(40));
            out.publish(7);
        });

        assert_eq!(
            SLOW.current(),
            None,
            "the producer has been started and has not had a turn: this is the answer that lied"
        );
        assert_eq!(SLOW.awaited(Duration::from_secs(2)), Some(7));
        assert_eq!(SLOW.current(), Some(7), "and it stands for every later read");
    }

    /// The wait is bounded: a service that genuinely has nothing to report still answers, rather than holding
    /// the caller for as long as it takes to find out there is no answer.
    #[test]
    fn a_service_with_nothing_to_say_still_answers_within_its_patience() {
        static SILENT: Service<u8> = Service::new("test-silent", |_| {});
        assert_eq!(SILENT.awaited(Duration::from_millis(30)), None);
    }
}
