//! State that outlives the tree that reads it.
//!
//! A surface is rebuilt when the config changes: the widget tree is dropped and built again on the same
//! surface, so everything that lived *in* the tree goes with it — the search a user had typed, which page of
//! the settings they were on, how far a transition had got. Two shapes of answer, both scoped to the surface's
//! own service world (the one thing that outlives a build and dies with the surface):
//!
//! - [`kept`] — a value created once and handed back on every build after that. Upstream, because nothing
//!   about it is this shell's: any telar app that remounts a surface needs it.
//! - [`set_context`]/[`context`] — what a surface's content wants everything under it to be able to read.
//!
//! Keys are namespaced by whoever owns them (`"launcher.query"`, `"settings.page"`) because a *bar* surface is
//! shared by every module on it — two modules reaching for `"query"` would be reaching for the same value.

use std::cell::RefCell;
use std::rc::Rc;

pub use telar::kept;

/// A surface's context of some type, in a cell so a rebuild can replace it.
#[derive(Clone)]
struct Slot<T>(Rc<RefCell<T>>);

/// Sets this surface's context of type `T` — what its content wants every widget under it to be able to read
/// without being handed it (rsx `provide`/`inject`): which module a panel shows, which bar a chip is on.
///
/// **Written rather than provided, and that is the whole point.** `provide` registers a type once per scope
/// and refuses the second attempt, while a surface's scope outlives every build of its content — so a rebuild
/// that provided again would be told it already had one and go on drawing against the context of an edit ago.
pub fn set_context<T: Clone + 'static>(value: T) {
    match telar::try_inject::<Slot<T>>() {
        Some(slot) => *slot.0.borrow_mut() = value,
        None => {
            let _ = telar::provide(Slot(Rc::new(RefCell::new(value))));
        }
    }
}

/// This surface's context of type `T`, as the latest build left it. `None` before anything set one — a widget
/// built outside a surface, which is every unit test.
pub fn context<T: Clone + 'static>() -> Option<T> {
    telar::try_inject::<Slot<T>>().map(|slot| slot.0.borrow().clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A context is written by every build, not provided by the first one — or a rebuilt panel would still be
    /// showing the module, the radius and the config the build before it was given.
    #[test]
    fn a_second_build_replaces_the_context_the_first_one_set() {
        #[derive(Clone, PartialEq, Debug)]
        struct Ctx(&'static str);

        telar::Scope::with(|| {
            set_context(Ctx("first"));
            set_context(Ctx("second"));
            assert_eq!(context::<Ctx>(), Some(Ctx("second")));
        });
    }
}
