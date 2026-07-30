//! Deriving one value from another, for a card that has to follow a service while it is on screen.
//!
//! Two rules are encoded here, both learned the hard way.
//!
//! **Read the value out before mapping it.** `with` holds the reactive runtime's borrow for as long as its
//! closure runs, so a nested read panics with `RefCell already borrowed`. The nested read is rarely visible:
//! `t!` reads the locale signal, so any closure translating its own string is one. It is not a compile error
//! and it does not fire until the surface is built. Reading the locale still happens inside `map`, which is
//! what makes a derived label re-render on a live language switch.
//!
//! **A derivation is a [`Memo`], never a signal written by an effect.** `telar::effect` hands back a handle whose
//! `Drop` deregisters the effect, so `let _ = effect(…)` runs exactly once and then stops — the derived value is
//! seeded correctly and never moves again, which looks like a working card until you watch it. A `Memo` is
//! `Rc`-backed and lives as long as the closure reading it, so the widget that draws the value is what keeps
//! the derivation alive, with nothing for a caller to remember.

use telar::{
    Effect, LayoutError, LayoutItem, LayoutStyle, Memo, ReadSignal, RectStyle, RwSignal,
    SizeDimension, StyledContainer, memo,
};

/// A value a surface reads and re-reads: derived from a service, or fixed for the life of the surface. One type
/// for both so a card takes one kind of argument rather than two.
pub type Live<T> = Memo<T>;

/// A value that never changes while the surface is up — a device name, a configured step, a mount point.
pub fn fixed<T: Clone + PartialEq + 'static>(value: T) -> Live<T> {
    memo(move || value.clone())
}

/// [`fixed`] for a literal, saving the `.to_string()` at every call site that labels a row.
pub fn fixed_text(text: impl Into<String>) -> Live<String> {
    fixed(text.into())
}

/// Anything a derivation can read: either handle on a signal, or another derivation.
///
/// One trait rather than a `derive`/`derive_from`/`map` family, because the difference between them was never
/// about behaviour — a card reading a service, a read handle, or a value already derived once all want the same
/// thing, and three spellings of it only meant picking the wrong one and chasing a type error.
pub trait Source {
    type Value;
    fn read(&self) -> Self::Value;
}

impl<T: Clone + 'static> Source for RwSignal<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

impl<T: Clone + 'static> Source for ReadSignal<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

impl<T: Clone + 'static> Source for Live<T> {
    type Value = T;
    fn read(&self) -> T {
        self.get()
    }
}

pub fn derive<S, U>(source: S, map: impl Fn(S::Value) -> U + 'static) -> Live<U>
where
    S: Source + 'static,
    U: PartialEq + 'static,
{
    memo(move || map(source.read()))
}

pub fn derive_pair<A, B, U>(
    first: A,
    second: B,
    map: impl Fn(A::Value, B::Value) -> U + 'static,
) -> Live<U>
where
    A: Source + 'static,
    B: Source + 'static,
    U: PartialEq + 'static,
{
    memo(move || map(first.read(), second.read()))
}

/// Ties `subscription` to `item`'s lifetime, for an effect that belongs to one widget.
///
/// Needed in both directions. Dropping the handle deregisters the effect, so it would run once and stop; but
/// parking it somewhere longer-lived — a list that rebuilds its rows, say — leaves it firing against a node
/// that is gone. A `StyledContainer` holds its style closure for exactly its own lifetime, which is the span
/// wanted, so the closure is where the handle lives. The wrapper paints nothing.
pub fn keeping(
    item: Box<dyn LayoutItem>,
    subscription: Effect,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        move |_r| {
            let _ = &subscription;
            RectStyle::default()
        },
        vec![item],
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use telar::signal;

    #[test]
    fn a_derived_value_follows_its_source() {
        telar::reset_runtime();
        let source = signal(2i32);
        let doubled = derive(source.clone(), |n| n * 2);
        assert_eq!(
            doubled.get(),
            4,
            "seeded from the source, not from a default"
        );
        source.set(5);
        assert_eq!(doubled.get(), 10);
    }

    #[test]
    fn a_pair_recomputes_when_either_half_moves() {
        telar::reset_runtime();
        let level = signal(10i32);
        let charging = signal(false);
        let label = derive_pair(
            level.read_only(),
            charging.read_only(),
            |level, charging| format!("{level}{}", if charging { "+" } else { "" }),
        );
        assert_eq!(label.get(), "10");
        charging.set(true);
        assert_eq!(label.get(), "10+");
        level.set(11);
        assert_eq!(label.get(), "11+");
    }

    /// The regression this module exists for. Deriving through a signal written by an effect seeds correctly
    /// and then goes dead the moment the handle drops, which is what every hover popout used to do.
    #[test]
    fn a_derivation_outlives_the_call_that_made_it() {
        telar::reset_runtime();
        let source = signal(1i32);
        let derived = derive(source.clone(), |n| n * 10);
        // Whatever a widget would do: hold the handle in a closure and read it later.
        let read: Box<dyn Fn() -> i32> = Box::new(move || derived.get());
        source.set(7);
        assert_eq!(
            read(),
            70,
            "a widget holding the derivation keeps it subscribed"
        );
    }

    #[test]
    fn a_fixed_value_reads_back_unchanged() {
        telar::reset_runtime();
        assert_eq!(fixed_text("Tctl").get(), "Tctl");
        assert_eq!(fixed(42u32).get(), 42);
    }
}
