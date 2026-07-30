//! Placing a surface against the bar chip that opened it.
//!
//! A bar is only its own thickness tall, so anything larger than a chip has to become a surface of its own.
//! The tray's context menus and the hover popouts anchor the same way: the chip's laid-out rect decides where
//! the surface sits along the bar, and the bar's edge decides which side it hangs off. No platform capability
//! beyond an anchor and a margin is involved, which is why both work on all four edges unchanged.

use rsx::{Rect, SurfaceAlign, SurfaceAnchor, SurfacePlacement, SurfaceRole, SurfaceSize};

use crate::core::config::Edge;
use crate::shared::module::SurfaceEnv;

/// Stand-in for an output the compositor has not reported a logical size for yet. Only ever feeds the clamp
/// arithmetic, which needs a finite screen to clamp against.
const ASSUMED_OUTPUT: (f32, f32) = (1920.0, 1080.0);

/// The logical size of the monitor this bar is on, for keeping an anchored surface on screen.
pub fn output_size(env: &SurfaceEnv) -> (f32, f32) {
    let outputs = platform_layershell::outputs();
    let matched = match &env.output {
        Some(name) => outputs.iter().find(|o| o.name.as_deref() == Some(name)),
        None => outputs.first(),
    };
    matched
        .and_then(|o| o.logical_size)
        .map(|(w, h)| (w as f32, h as f32))
        .unwrap_or(ASSUMED_OUTPUT)
}

pub fn anchor_for(edge: Edge) -> SurfaceAnchor {
    match edge {
        Edge::Top => SurfaceAnchor::Top,
        Edge::Bottom => SurfaceAnchor::Bottom,
        Edge::Left => SurfaceAnchor::Left,
        Edge::Right => SurfaceAnchor::Right,
    }
}

/// How far along the bar the surface starts, clamped so it stays on the output.
///
/// A horizontal bar centres the surface on its chip, which is what a menu hanging under an icon should do. A
/// vertical one lines the surface's top up with the chip's instead: `span` there is the surface's *height*,
/// which is content-derived for a menu and therefore often unknown, and a centre that cannot be computed is
/// worse than an edge that can.
fn along(env: &SurfaceEnv, chip: Rect, span: Option<f32>) -> f32 {
    let gap = env.config.panel_gap(env.edge) as f32;
    let (width, height) = output_size(env);
    let (start, extent) = if env.edge.is_vertical() {
        (chip.y, height)
    } else {
        (
            chip.x + chip.width / 2.0 - span.unwrap_or_default() / 2.0,
            width,
        )
    };
    let far = (extent - span.unwrap_or_default() - gap).max(gap);
    start.clamp(gap, far)
}

/// The margin `(top, right, bottom, left)` for a surface hanging `off_bar` px off the bar and lined up with
/// `chip` along it. `span` is the surface's own extent along the bar, when it is known — without it the
/// surface can still be positioned, only not kept clear of the far end.
pub fn chip_margin(
    env: &SurfaceEnv,
    chip: Rect,
    off_bar: f32,
    span: Option<f32>,
) -> (i32, i32, i32, i32) {
    let along = along(env, chip, span) as i32;
    let off_bar = off_bar as i32;
    match env.edge {
        Edge::Top => (off_bar, 0, 0, along),
        Edge::Bottom => (0, 0, off_bar, along),
        Edge::Left => (along, 0, 0, off_bar),
        Edge::Right => (along, off_bar, 0, 0),
    }
}

/// A placement for a surface hanging off the bar under `chip`, dismissed by a press outside it — the tray's
/// context menus.
///
/// The distance off the bar is the shared [`panel_gap`](crate::Config::panel_gap) and nothing more: the
/// surface underneath uses `exclusive_zone = 0`, so the compositor has already positioned it past the bar's
/// reserved zone, exactly as a drawer is positioned.
pub fn chip_placement(env: &SurfaceEnv, chip: Rect, span: Option<f32>) -> SurfacePlacement {
    let off_bar = env.config.panel_gap(env.edge) as f32;
    SurfacePlacement::new(SurfaceRole::Drawer, anchor_for(env.edge))
        .align(SurfaceAlign::Start)
        .size(SurfaceSize::Auto)
        .margin(chip_margin(env, chip, off_bar, span))
        .dismiss_on_outside(true)
        .output(env.output.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    const SPAN: f32 = 260.0;

    fn env(edge: Edge) -> SurfaceEnv {
        SurfaceEnv {
            edge,
            bar_size: 34,
            output: None,
            config: Arc::new(crate::core::config::Config::starter()),
        }
    }

    fn chip(x: f32, y: f32) -> Rect {
        Rect {
            x,
            y,
            width: 30.0,
            height: 30.0,
        }
    }

    #[test]
    fn a_surface_under_a_top_bar_clears_the_bar_and_centres_on_its_chip() {
        let env = env(Edge::Top);
        let gap = env.config.panel_gap(Edge::Top) as i32;
        let (top, _, _, left) = chip_margin(&env, chip(500.0, 0.0), gap as f32, Some(SPAN));
        assert_eq!(
            top, gap,
            "the surface hangs the standard panel gap off the bar"
        );
        assert_eq!(
            left,
            (500.0 + 15.0 - SPAN / 2.0) as i32,
            "and centres on the chip"
        );
    }

    #[test]
    fn a_chip_at_the_end_of_the_bar_keeps_its_surface_on_screen() {
        let env = env(Edge::Top);
        let (width, _) = output_size(&env);
        let (_, _, _, left) = chip_margin(&env, chip(width - 20.0, 0.0), 8.0, Some(SPAN));
        assert!(
            (left as f32) + SPAN <= width,
            "the surface was pushed off the right edge: left {left} + {SPAN} > {width}"
        );
    }

    #[test]
    fn a_bottom_bar_puts_its_surface_above_itself() {
        let env = env(Edge::Bottom);
        let (top, _, bottom, _) = chip_margin(&env, chip(100.0, 0.0), 12.0, Some(SPAN));
        assert_eq!(bottom, 12);
        assert_eq!(top, 0, "an upward surface is pinned from the bottom only");
    }

    #[test]
    fn a_vertical_bar_puts_its_surface_beside_itself() {
        let env = env(Edge::Left);
        let (top, _, _, left) = chip_margin(&env, chip(0.0, 300.0), 12.0, None);
        assert_eq!(left, 12, "the surface clears the bar's width");
        assert_eq!(top, 300, "and lines up with the chip it belongs to");
    }

    #[test]
    fn a_chip_near_the_bottom_of_a_vertical_bar_keeps_a_known_span_on_screen() {
        let env = env(Edge::Left);
        let (_, height) = output_size(&env);
        let (top, _, _, _) = chip_margin(&env, chip(0.0, height - 40.0), 12.0, Some(SPAN));
        assert!(
            (top as f32) + SPAN <= height,
            "a surface whose height is known is kept clear of the bottom edge"
        );
    }

    #[test]
    fn the_placement_carries_the_bar_edge_and_dismisses_on_an_outside_press() {
        for edge in Edge::ALL {
            let env = env(edge);
            let placement = chip_placement(&env, chip(200.0, 200.0), Some(SPAN));
            assert_eq!(placement.anchor, anchor_for(edge));
            assert_eq!(placement.align, SurfaceAlign::Start);
            assert!(
                placement.dismiss_on_outside,
                "a click outside closes a menu"
            );
        }
    }
}
