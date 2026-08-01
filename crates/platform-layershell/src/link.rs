//! The control channel between a live surface and whoever holds it.
//!
//! A layer surface used to be something the driver decided on its own: configured at one size when it was
//! created and never renegotiated, and closed by a flag that tore it down on the next loop turn. Both halves
//! are here instead. A [`SurfaceLink`] is shared by the driver's surface entry and the `SurfaceHandle` its
//! opener holds — one side asks, the other applies on its next turn — and an [`ExitPlan`] is what the surface's
//! own content registered for the moment it is asked to close, together with how long the driver must keep it
//! mapped for that to be seen.

use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// A change to a live surface's layer-shell geometry. Every field is optional because the three are asked for
/// independently: a bar sliding out of view retargets only its margin, a float being dragged wider only its
/// size, and an auto-hiding bar gives up its exclusive zone without touching either.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Geometry {
    pub size: Option<(u32, u32)>,
    /// `(top, right, bottom, left)`, in logical pixels. Negative values push the surface off its own edge,
    /// which is what leaves a hover strip of an auto-hidden bar on screen.
    pub margin: Option<(i32, i32, i32, i32)>,
    pub exclusive_zone: Option<i32>,
}

impl Geometry {
    pub fn size(width: u32, height: u32) -> Self {
        Self {
            size: Some((width, height)),
            ..Self::default()
        }
    }

    pub fn margin(margin: (i32, i32, i32, i32)) -> Self {
        Self {
            margin: Some(margin),
            ..Self::default()
        }
    }

    pub fn exclusive_zone(zone: i32) -> Self {
        Self {
            exclusive_zone: Some(zone),
            ..Self::default()
        }
    }

    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// Folds a later request over an earlier one still waiting to be applied, field by field. Two requests
    /// naming different fields have to *both* survive — a bar that gives up its exclusive zone and then slides
    /// out in the same loop turn must do both — and two naming the same field resolve to the newer, which is
    /// the whole point of coalescing an animation's frames into the one the driver will actually commit.
    fn merge(&mut self, next: Geometry) {
        self.size = next.size.or(self.size);
        self.margin = next.margin.or(self.margin);
        self.exclusive_zone = next.exclusive_zone.or(self.exclusive_zone);
    }
}

/// The shared state behind a `SurfaceHandle`: whether the surface has been asked to close, and any geometry
/// waiting to be pushed to the compositor.
#[derive(Default)]
pub(crate) struct SurfaceLink {
    closing: AtomicBool,
    pending: Mutex<Geometry>,
}

impl SurfaceLink {
    pub(crate) fn request_close(&self) {
        self.closing.store(true, Ordering::Relaxed);
    }

    pub(crate) fn is_closing(&self) -> bool {
        self.closing.load(Ordering::Relaxed)
    }

    pub(crate) fn request_geometry(&self, change: Geometry) {
        if let Ok(mut pending) = self.pending.lock() {
            pending.merge(change);
        }
    }

    /// The geometry to commit this turn, if any. Taking it is what keeps a surface that asks for the same size
    /// every frame from committing one every frame.
    pub(crate) fn take_geometry(&self) -> Option<Geometry> {
        let mut pending = self.pending.lock().ok()?;
        let taken = std::mem::take(&mut *pending);
        (!taken.is_empty()).then_some(taken)
    }
}

/// What a surface does when it is asked to close, and how long the driver holds it mapped afterwards so that
/// reaction reaches the screen.
///
/// Registered from inside the surface's own build, by whatever wants an exit transition — which is more than
/// one thing per surface: the hosted scaffold fades its scrim while the panel content slides back toward its
/// bar edge. So reactions accumulate and the linger is the longest of them, rather than the last registration
/// replacing the first.
#[derive(Default)]
pub(crate) struct ExitPlan {
    linger: Duration,
    reactions: Vec<Box<dyn FnOnce()>>,
}

impl ExitPlan {
    pub(crate) fn push(&mut self, linger: Duration, reaction: Box<dyn FnOnce()>) {
        self.linger = self.linger.max(linger);
        self.reactions.push(reaction);
    }

    pub(crate) fn linger(&self) -> Duration {
        self.linger
    }

    /// Whether this plan asks the driver for anything at all. An empty one — no reaction, or a zero duration
    /// because the user switched animation off — means the surface goes now, exactly as it did before any of
    /// this existed.
    pub(crate) fn is_empty(&self) -> bool {
        self.linger.is_zero() || self.reactions.is_empty()
    }

    pub(crate) fn run(self) {
        for reaction in self.reactions {
            reaction();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_later_geometry_request_folds_over_the_one_still_waiting() {
        let link = SurfaceLink::default();
        link.request_geometry(Geometry::exclusive_zone(0));
        link.request_geometry(Geometry::margin((-40, 0, 0, 0)));
        link.request_geometry(Geometry::margin((-20, 0, 0, 0)));

        let taken = link.take_geometry().expect("three requests are one commit");
        assert_eq!(
            taken.exclusive_zone,
            Some(0),
            "a field nobody asked about again must survive the ones that did — a bar giving up its zone and \
             then sliding out has to do both"
        );
        assert_eq!(
            taken.margin,
            Some((-20, 0, 0, 0)),
            "an animation's intermediate frames coalesce to the one the driver will commit"
        );
        assert!(
            link.take_geometry().is_none(),
            "taking it is what stops a surface asking for the same size every frame from committing every frame"
        );
    }

    #[test]
    fn an_exit_plan_keeps_every_reaction_and_the_longest_linger() {
        let fired = std::rc::Rc::new(std::cell::Cell::new(0u32));
        let mut plan = ExitPlan::default();
        assert!(
            plan.is_empty(),
            "nothing registered means the surface goes now"
        );

        for linger in [Duration::from_millis(120), Duration::from_millis(200)] {
            let sink = std::rc::Rc::clone(&fired);
            plan.push(linger, Box::new(move || sink.set(sink.get() + 1)));
        }
        assert_eq!(
            plan.linger(),
            Duration::from_millis(200),
            "the surface has to outlive the slowest half of its exit, not the last one registered"
        );
        assert!(!plan.is_empty());
        plan.run();
        assert_eq!(fired.get(), 2, "both halves of the exit run");
    }

    #[test]
    fn a_zero_duration_exit_is_no_exit() {
        let mut plan = ExitPlan::default();
        plan.push(Duration::ZERO, Box::new(|| {}));
        assert!(
            plan.is_empty(),
            "animation switched off must tear the surface down on the next turn, as it always did"
        );
    }
}
