//! Every preview, laid out on all four edges in all three shape modes, measured rather than merely built.
//!
//! `cargo telar test` renders each preview once and fails on a panic or a layout error, which is
//! `assert!(build().is_ok())` — and that passes for every layout bug this tree has actually had: a result list
//! laid out 612×**0** because a `max_height` is not a height, a wallpaper row measuring shorter than the tiles
//! inside it, an avatar drawn outside its scrolling viewport. Each was found by looking at a picture, and each
//! is a fact about *geometry*, which is why this measures the draw commands instead of comparing stored images:
//! a baseline would also have to be told apart from a font that shaped differently on the machine running it,
//! and a suite that cries wolf is one people delete.
//!
//! The twelve combinations are the shell's own hard rule — every surface and every module works on all four
//! edges and in `bar`, `sections` and `chips` — so a module only ever looked at on a top bar is exactly where a
//! shape bug survives.
//!
//! **What this deliberately does not ask**, having measured it and found the question unsound:
//!
//! - *"Did the preview draw anything at all?"* 79 of 456 draw nothing here, and correctly: `battery`,
//!   `brightness`, `mic`, `network`, `volume` and `lockstatus` are chips whose service has no reading on a
//!   machine with no battery, no backlight and no PipeWire, and a chip with nothing to say draws nothing. The
//!   answer would change with the hardware under the test, which is the definition of a flaky assertion. Seeding
//!   each of those previews the way [`modules::preview`] seeds the tray and the workspaces is what would make it
//!   askable.
//! - *"Did anything draw outside its surface?"* — the avatar bug. Draw rects are pre-transform, and a scroll
//!   offset is a `PushMatrix` rather than a relayout, so answering needs the matrix stack simulated; and even
//!   then content below the fold of a scroll area is legitimately outside the viewport. Both halves have to be
//!   solved together or the check reports the normal case as a fault.

#![cfg(test)]

use std::sync::Arc;

use telar::{
    AvailableSpace, ComponentList, Container, DrawCommand, LayoutError, LayoutStyle, PreviewEntry,
    Rect, compute_layout, new_container, reset_layout_runtime, set_theme,
};

use config::{BarConfig, Config, Edge, Shape};

/// The page a preview is measured on when it is a tree rather than a surface. Wide enough that a bar-width
/// module is not the thing under test.
const PAGE: (f32, f32) = (1000.0, 760.0);

/// Below this a rect is not something a user can see. Text shaping and fractional layout both land a fraction
/// under a whole pixel routinely, so the question asked is "did this collapse", not "is this exact".
const COLLAPSED: f32 = 0.5;

const MODES: [Shape; 3] = [Shape::Bar, Shape::Sections, Shape::Chips];

/// The world one combination builds against. Deliberately [`Config::starter`] rather than the user's file: a
/// sweep that read `~/.config/hyprshell/config.toml` would measure a different shell on every machine.
fn seed_world(edge: Edge, mode: Shape) {
    let mut config = Config::starter();
    // `starter` puts its modules on the top bar and `drawn_edge` reports the first non-empty one, so moving them
    // wholesale is what makes a chip believe it is on the edge under test.
    let bar = std::mem::take(&mut config.bars.top);
    *match edge {
        Edge::Top => &mut config.bars.top,
        Edge::Bottom => &mut config.bars.bottom,
        Edge::Left => &mut config.bars.left,
        Edge::Right => &mut config.bars.right,
    } = bar;
    if edge != Edge::Top {
        config.bars.top = BarConfig::default();
    }
    config.shape.mode = mode;

    let config = Arc::new(config);
    services::locale::init(config.language());
    telar::set_default_font_family(config.theme.font_family.clone());
    ui::icon::init_store(&config.icons);
    set_theme(config.resolve_theme());
    config::set_config(config);
    crate::install_hooks();
}

/// What the entry put on screen, in the coordinates its own draw commands carry.
fn measure(entry: &PreviewEntry) -> Result<Vec<DrawCommand>, LayoutError> {
    let (width, height) = entry
        .surface
        .map(|surface| (surface.width, surface.height))
        .unwrap_or(PAGE);
    let page = || LayoutStyle::new().flex_column().width(width).height(height);

    let built = (entry.build)()?;
    let root_node = new_container(page(), &[built.layout_node()])?;
    let tree = ComponentList::new(Container::new(page(), vec![built])?);
    compute_layout(
        root_node,
        AvailableSpace::Definite(width),
        AvailableSpace::Definite(height),
    )?;
    Ok(tree.commands().to_vec())
}

/// The rect a command is answerable for, and whether it has any content to put there — an empty `Text` shapes
/// to nothing and is the one zero-area draw that is not a fault. `Line` and `Path` carry their own geometry
/// rather than a box, and the matrix and layer markers cover nothing of their own.
fn painted_rect(command: &DrawCommand) -> Option<Rect> {
    match command {
        DrawCommand::Rect { rect, .. } | DrawCommand::Image { rect, .. } => Some(*rect),
        DrawCommand::Text { rect, text, .. } => (!text.is_empty()).then_some(*rect),
        DrawCommand::RichText { rect, runs, .. } => {
            runs.iter().any(|run| !run.text.is_empty()).then_some(*rect)
        }
        // A viewport clipped to nothing is the canonical shape of the bug this file exists for: the content
        // inside keeps its own honest rects and is cut away wholesale, so only the clip itself shows the fault.
        DrawCommand::PushClip { rect, .. } => Some(*rect),
        _ => None,
    }
}

fn sweep(mut each: impl FnMut(&PreviewEntry, Edge, Shape, Result<Vec<DrawCommand>, LayoutError>)) {
    for edge in Edge::ALL {
        for mode in MODES {
            for entry in crate::preview_entries() {
                reset_layout_runtime();
                seed_world(edge, mode);
                let measured = measure(&entry);
                each(&entry, edge, mode, measured);
            }
        }
    }
}

/// Twelve times what `cargo telar test` covers: it renders each preview in whatever shape the config happens to
/// carry, and every combination it does not render is one where a build or a layout can fail unseen.
#[test]
fn every_preview_lays_out_on_every_edge_and_shape() {
    let (mut checked, mut broken) = (0usize, Vec::new());
    sweep(|entry, edge, mode, measured| {
        checked += 1;
        if let Err(e) = measured {
            broken.push(format!(
                "{}::{} on {edge:?}/{mode:?} — {e}",
                entry.component_name, entry.preview_name
            ));
        }
    });
    assert!(checked > 0, "the sweep found no previews to measure");
    assert!(
        broken.is_empty(),
        "{} of {checked} combinations failed to lay out:\n  {}",
        broken.len(),
        broken.join("\n  ")
    );
}

/// The regression net proper. A draw command with no area is content the user cannot see, and it is what every
/// layout bug in this tree's history looked like from underneath — the tree built, the pass succeeded, and the
/// thing measured to nothing.
#[test]
fn nothing_a_preview_draws_collapses_to_nothing() {
    let mut collapsed = Vec::new();
    sweep(|entry, edge, mode, measured| {
        let Ok(commands) = measured else { return };
        for command in commands {
            let Some(rect) = painted_rect(&command) else {
                continue;
            };
            if rect.width >= COLLAPSED && rect.height >= COLLAPSED {
                continue;
            }
            collapsed.push(format!(
                "{}::{} on {edge:?}/{mode:?} — {} at {}x{}",
                entry.component_name,
                entry.preview_name,
                kind(&command),
                rect.width,
                rect.height
            ));
        }
    });
    assert!(
        collapsed.is_empty(),
        "{} draw(s) landed with no area, so nothing of them reaches the screen:\n  {}",
        collapsed.len(),
        collapsed.join("\n  ")
    );
}

fn kind(command: &DrawCommand) -> &'static str {
    match command {
        DrawCommand::Rect { .. } => "a rect",
        DrawCommand::Text { .. } => "text",
        DrawCommand::RichText { .. } => "rich text",
        DrawCommand::Image { .. } => "an image",
        DrawCommand::PushClip { .. } => "a viewport",
        _ => "a draw",
    }
}
