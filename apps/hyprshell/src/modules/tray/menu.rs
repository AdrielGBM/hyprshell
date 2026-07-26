//! The popup menu behind a tray icon.
//!
//! A real layer-shell surface rather than an in-surface overlay: a bar is only its own thickness tall, so a
//! menu drawn inside it would be clipped to a sliver. It is anchored by computing the chip's offset along the
//! bar and handing that to [`SurfacePlacement`] as a margin — which the scaffold applies as padding, so no new
//! platform capability is needed.

use std::cell::RefCell;

use rsx::{
    AlignItems, Color, Container, LayoutError, LayoutItem, LayoutStyle, Rect, RectStyle,
    SizeDimension, StyledContainer, SurfaceAlign, SurfaceAnchor, SurfacePlacement, SurfaceSize,
    Text, TextStyle, box_item, open_surface, set_theme,
};

use crate::core::config::Edge;
use crate::shared::icon::{app_icon_view, icon_view};
use crate::shared::module::SurfaceEnv;
use crate::shared::services::dbusmenu::{self, MenuItem, Toggle};
use crate::shared::services::tray::TrayItem;
use crate::shared::theme::{FontRole, NordTheme};

/// The shell's id for the menu surface. One at a time: a second tray menu on screen would be two context
/// menus at once, which no desktop does.
const SURFACE_ID: &str = "tray-menu";

/// Fixed rather than content-derived, so the anchoring maths knows the width before the menu is laid out and
/// can keep it on screen. A tray menu is a list of short labels; letting it size to its longest one would make
/// every application's menu a different width.
const MENU_WIDTH: f32 = 260.0;

const ROW_HEIGHT: f32 = 30.0;
const SEPARATOR_HEIGHT: f32 = 9.0;

thread_local! {
    /// Which item's menu is showing, so a second click on the same chip closes it while a click on another
    /// chip switches. Driver-thread only, like the rest of the surface bookkeeping.
    static OPEN_FOR: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn close() {
    OPEN_FOR.with(|o| *o.borrow_mut() = None);
    crate::core::shell::close(SURFACE_ID);
}

/// Opens `item`'s menu under its chip, or closes it if that same menu is already up.
///
/// The layout is fetched on a worker thread and the surface opened from the handler, which
/// [`platform_layershell::watch`] runs on the driver thread — the only place a surface may be opened. Doing the
/// round trip inline would stall the frame on however long another application takes to answer.
pub fn toggle(item: &TrayItem, chip: Rect, env: SurfaceEnv) {
    // Both halves matter: `OPEN_FOR` alone would still name this item after the menu was dismissed by a click
    // outside it, and the next click on the same chip would read as "close" and do nothing.
    let already_open = crate::core::shell::window_is_open(SURFACE_ID)
        && OPEN_FOR.with(|o| o.borrow().as_deref() == Some(item.key.as_str()));
    close();
    if already_open || item.menu.trim().is_empty() {
        return;
    }
    OPEN_FOR.with(|o| *o.borrow_mut() = Some(item.key.clone()));

    let key = item.key.clone();
    let label = item.label().to_string();
    let bus = item.bus.clone();
    let path = item.menu.clone();
    let event_bus = bus.clone();
    let event_path = path.clone();

    platform_layershell::watch(dbusmenu::fetch_into(bus, path), move |menu: Option<MenuItem>| {
        // The chip may have been clicked again, or another one opened, while the application was answering.
        if OPEN_FOR.with(|o| o.borrow().as_deref() != Some(key.as_str())) {
            return;
        }
        let Some(root) = menu.filter(|m| !m.children.is_empty()) else {
            tracing::info!("tray item '{label}' offers no menu to show");
            OPEN_FOR.with(|o| *o.borrow_mut() = None);
            return;
        };
        let placement = placement(&env, chip);
        let theme = env.config.resolve_theme();
        let radius = env.config.panel_radius(env.edge);
        let (bus, path) = (event_bus.clone(), event_path.clone());
        crate::core::shell::toggle_window(SURFACE_ID, move || {
            open_surface(
                placement,
                Box::new(move || {
                    set_theme(theme);
                    menu_view(&root, &bus, &path, theme, radius).expect("tray menu build failed")
                }),
            )
        });
    });
}

/// The gap between the bar and the menu, and from the screen edge — the same spacing a panel uses, so a menu
/// sits where every other surface of this shell does.
fn offsets(env: &SurfaceEnv) -> (f32, f32) {
    let gap = env.config.panel_gap(env.edge) as f32;
    (env.bar_size as f32 + gap, gap)
}

/// The logical size of the monitor this bar is on, for keeping the menu on screen.
fn output_size(env: &SurfaceEnv) -> Option<(f32, f32)> {
    let outputs = platform_layershell::outputs();
    let matched = match &env.output {
        Some(name) => outputs.iter().find(|o| o.name.as_deref() == Some(name)),
        None => outputs.first(),
    };
    matched
        .and_then(|o| o.logical_size)
        .map(|(w, h)| (w as f32, h as f32))
}

/// Where the menu sits: pushed off the bar along its own axis, and centred on the chip across it — clamped so
/// a chip near the end of the bar (which is where a tray usually lives) doesn't put its menu off screen.
fn placement(env: &SurfaceEnv, chip: Rect) -> SurfacePlacement {
    let (from_bar, edge_gap) = offsets(env);
    let (width, height) = output_size(env).unwrap_or((1920.0, 1080.0));
    let anchor = match env.edge {
        Edge::Top => SurfaceAnchor::Top,
        Edge::Bottom => SurfaceAnchor::Bottom,
        Edge::Left => SurfaceAnchor::Left,
        Edge::Right => SurfaceAnchor::Right,
    };

    let margin = if env.edge.is_vertical() {
        // The menu's height is content-derived, so the cross-axis clamp uses the chip's own top rather than a
        // centred offset it cannot compute.
        let top = chip.y.clamp(edge_gap, (height - edge_gap).max(edge_gap));
        match env.edge {
            Edge::Left => (top as i32, 0, 0, from_bar as i32),
            _ => (top as i32, from_bar as i32, 0, 0),
        }
    } else {
        let centred = chip.x + chip.width / 2.0 - MENU_WIDTH / 2.0;
        let max_left = (width - MENU_WIDTH - edge_gap).max(edge_gap);
        let left = centred.clamp(edge_gap, max_left);
        match env.edge {
            Edge::Top => (from_bar as i32, 0, 0, left as i32),
            _ => (0, 0, from_bar as i32, left as i32),
        }
    };

    SurfacePlacement::new(rsx::SurfaceRole::Drawer, anchor)
        .align(SurfaceAlign::Start)
        .size(SurfaceSize::Auto)
        .margin(margin)
        .dismiss_on_outside(true)
        .output(env.output.clone())
}

fn menu_view(
    root: &MenuItem,
    bus: &str,
    path: &str,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = rows_for(&root.children, bus, path, theme, 0)?;
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(MENU_WIDTH)
            .padding_all(6.0)
            .gap(1.0),
        move |_| RectStyle::filled(theme.surface, radius),
        rows,
    )?;
    Ok(Box::new(panel))
}

/// Renders a level of the menu. A submenu is expanded inline, one indent deeper, rather than flying out into a
/// second surface: it keeps every row reachable in one place, and a tray menu is rarely more than two deep.
fn rows_for(
    items: &[MenuItem],
    bus: &str,
    path: &str,
    theme: NordTheme,
    depth: usize,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let mut out: Vec<Box<dyn LayoutItem>> = Vec::new();
    for item in items {
        if item.separator {
            out.push(separator_row(theme)?);
            continue;
        }
        out.push(row(item, bus, path, theme, depth)?);
        if item.has_submenu() {
            out.extend(rows_for(&item.children, bus, path, theme, depth + 1)?);
        }
    }
    Ok(out)
}

fn separator_row(theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let line = StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(1.0),
        move |_| RectStyle::filled(theme.overlay, 0.0),
        vec![],
    )?;
    let holder = Container::new(
        LayoutStyle::new()
            .flex_column()
            .justify_content(rsx::JustifyContent::CENTER)
            .width(SizeDimension::Percent(1.0))
            .height(SEPARATOR_HEIGHT),
        vec![box_item(line)],
    )?;
    Ok(Box::new(holder))
}

/// The tick a toggled row carries. A radio and a checkbox are different promises — one of a set versus on/off —
/// so they get different glyphs rather than one shared dot.
fn toggle_glyph(toggle: Toggle) -> Option<&'static str> {
    match toggle {
        Toggle::None => None,
        Toggle::Checkmark(true) => Some("mdi:checkbox-marked-outline"),
        Toggle::Checkmark(false) => Some("mdi:checkbox-blank-outline"),
        Toggle::Radio(true) => Some("mdi:radiobox-marked"),
        Toggle::Radio(false) => Some("mdi:radiobox-blank"),
    }
}

fn row(
    item: &MenuItem,
    bus: &str,
    path: &str,
    theme: NordTheme,
    depth: usize,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let enabled = item.enabled;
    let fg = if enabled { theme.text } else { theme.muted };
    let icon_size = 16.0;
    let mut content: Vec<Box<dyn LayoutItem>> = Vec::new();

    if let Some(glyph) = toggle_glyph(item.toggle) {
        content.push(icon_view(move || glyph.to_string(), move || fg, icon_size)?);
    } else if !item.icon_name.is_empty()
        && let Some(icon) = app_icon_view(&item.icon_name, icon_size)?
    {
        content.push(icon);
    }

    let label = item.label.clone();
    let text = Text::auto(
        move || label.clone(),
        LayoutStyle::new().flex_grow(1.0),
        move || TextStyle::new(theme.font(FontRole::Body), fg),
    )?;
    content.push(box_item(text));

    if item.has_submenu() {
        content.push(icon_view(
            || "mdi:chevron-down".to_string(),
            move || theme.muted,
            icon_size,
        )?);
    }

    let indent = 8.0 + depth as f32 * 14.0;
    let style = LayoutStyle::new()
        .flex_row()
        .align_items(AlignItems::CENTER)
        .gap(8.0)
        .width(SizeDimension::Percent(1.0))
        .height(ROW_HEIGHT)
        .padding_left(indent)
        .padding_right(8.0);

    let mut container = StyledContainer::new(
        style,
        move |_| RectStyle::filled(Color::TRANSPARENT, 6.0),
        content,
    )?;

    // A row that opens a submenu is already showing it (submenus expand inline), and a disabled one is a
    // label — neither is pressable, so neither gets hover feedback that promises otherwise.
    if item.is_actionable() {
        let (bus, path, id) = (bus.to_string(), path.to_string(), item.id);
        container = container
            .on_hover_style(move |_| RectStyle::filled(theme.overlay, 6.0))
            .on_press(move || {
                dbusmenu::activate(&bus, &path, id);
                close();
            });
    }
    Ok(Box::new(container))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(edge: Edge) -> SurfaceEnv {
        SurfaceEnv {
            edge,
            bar_size: 34,
            output: None,
            config: std::sync::Arc::new(crate::core::config::Config::starter()),
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
    fn a_menu_under_a_top_bar_clears_the_bar_and_centres_on_its_chip() {
        let env = env(Edge::Top);
        let (from_bar, _) = offsets(&env);
        let placement = placement(&env, chip(500.0, 0.0));
        let (top, _, _, left) = placement.margin;
        assert_eq!(top, from_bar as i32, "the menu hangs below the bar, not under it");
        assert_eq!(
            left,
            (500.0 + 15.0 - MENU_WIDTH / 2.0) as i32,
            "the menu centres on the chip"
        );
        assert_eq!(placement.anchor, SurfaceAnchor::Top);
        assert_eq!(placement.align, SurfaceAlign::Start);
        assert!(placement.dismiss_on_outside, "a click outside closes a menu");
    }

    #[test]
    fn a_chip_at_the_end_of_the_bar_keeps_its_menu_on_screen() {
        let env = env(Edge::Top);
        let (width, _) = output_size(&env).unwrap_or((1920.0, 1080.0));
        // A chip hard against the right edge would centre its menu well past the screen.
        let placement = placement(&env, chip(width - 20.0, 0.0));
        let (_, _, _, left) = placement.margin;
        assert!(
            (left as f32) + MENU_WIDTH <= width,
            "the menu was pushed off the right edge: left {left} + {MENU_WIDTH} > {width}"
        );
    }

    #[test]
    fn a_bottom_bar_puts_its_menu_above_itself() {
        let env = env(Edge::Bottom);
        let (from_bar, _) = offsets(&env);
        let placement = placement(&env, chip(100.0, 0.0));
        let (top, _, bottom, _) = placement.margin;
        assert_eq!(bottom, from_bar as i32);
        assert_eq!(top, 0, "an upward menu is pinned from the bottom only");
        assert_eq!(placement.anchor, SurfaceAnchor::Bottom);
    }

    #[test]
    fn a_vertical_bar_puts_its_menu_beside_itself() {
        let env = env(Edge::Left);
        let (from_bar, _) = offsets(&env);
        let placement = placement(&env, chip(0.0, 300.0));
        let (top, _, _, left) = placement.margin;
        assert_eq!(left, from_bar as i32, "the menu clears the bar's width");
        assert_eq!(top, 300, "and lines up with the chip it belongs to");
        assert_eq!(placement.anchor, SurfaceAnchor::Left);
    }

    #[test]
    fn a_separator_never_renders_as_a_pressable_row() {
        let separator = MenuItem {
            separator: true,
            enabled: true,
            ..MenuItem::default()
        };
        assert!(!separator.is_actionable());
    }
}
