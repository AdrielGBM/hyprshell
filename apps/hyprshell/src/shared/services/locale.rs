//! The UI language as a shared source: one current locale broadcast to every subscribed surface, so a language
//! switch in one panel updates the bars and other panels live. The locale signal is thread-local per surface
//! (like the theme), so cross-surface propagation uses the same watch/subscribe channel pattern as the
//! notification and battery services — `set` fans the new tag out to each surface's event loop, which applies
//! it on its own thread and redraws.

use std::cell::Cell;
use std::sync::Mutex;

use platform_layershell::EventSender;

struct State {
    current: String,
    subscribers: Vec<EventSender<String>>,
}

static STATE: Mutex<State> = Mutex::new(State {
    current: String::new(),
    subscribers: Vec::new(),
});

thread_local! {
    // Ensures a surface's event loop subscribes exactly once, even if its content is rebuilt many times
    // (reopening a panel), so `set` doesn't fan out to a growing list of duplicate subscribers per thread.
    static SUBSCRIBED: Cell<bool> = const { Cell::new(false) };
}

/// Seeds the current language (from config) without broadcasting. Called at startup and on each config reload.
pub fn init(lang: String) {
    STATE.lock().unwrap().current = lang;
}

/// The current language, or `fallback` if none has been set yet.
pub fn current_or(fallback: String) -> String {
    let current = STATE.lock().unwrap().current.clone();
    if current.is_empty() { fallback } else { current }
}

/// Registers `tx` (bound to a surface's event loop) and immediately sends the current language so the surface
/// starts in sync. The `watch` producer for [`attach`].
pub fn subscribe(tx: EventSender<String>) {
    let mut state = STATE.lock().unwrap();
    let current = state.current.clone();
    if !current.is_empty() {
        let _ = tx.send(current);
    }
    state.subscribers.push(tx);
}

/// Switches the language and broadcasts it to every subscribed surface (dropping any whose loop has closed).
pub fn set(lang: impl Into<String>) {
    let lang = lang.into();
    let mut state = STATE.lock().unwrap();
    state.current = lang.clone();
    state.subscribers.retain(|tx| tx.send(lang.clone()));
}

/// Applies the language on THIS surface's thread and subscribes its event loop to future switches. Call at the
/// top of a surface builder (after loading config): the initial `set_locale` avoids a first-frame flash, and
/// the one-time `watch` keeps it live when another surface calls [`set`].
pub fn attach(fallback: String) {
    rsx::set_locale(current_or(fallback));
    SUBSCRIBED.with(|done| {
        if !done.get() {
            done.set(true);
            platform_layershell::watch(subscribe, |lang| rsx::set_locale(lang));
        }
    });
}
