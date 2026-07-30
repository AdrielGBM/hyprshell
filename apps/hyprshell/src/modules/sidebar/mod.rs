//! The notification centre: a full-height surface that is the home for what has arrived and what can be switched.
//!
//! The bell drawer is a *glance* — it hangs off its chip, it is as tall as its content, and it closes when you
//! look away. This is the other thing: it takes the whole edge, it scrolls, and it is where a user goes to work
//! through a morning's notifications. It hosts the utilities panel's own toggles rather than a second set of
//! them, which is the whole reason the two were built together: two independent copies of "turn Wi-Fi off" would
//! drift the day one of them gained a toggle.

use std::sync::Arc;

use platform_layershell::{Anchor, KeyboardInteractivity, Layer, LayerConfig, open_surface};
use telar::{
    AlignItems, App, Color, Component, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutScrollArea, LayoutStyle, RectStyle, SizeDimension, StyledContainer, SurfaceToken, Text,
    WindowConfig, box_item, reset_layout_runtime, set_theme, use_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Config, Edge};
use crate::shared::module::{SurfaceEnv, set_surface_env};
use crate::shared::theme::{FontRole, NordTheme};

pub const ID: &str = "sidebar";

/// Opens the centre, or closes it if it is up. Registered with the shell's surface registry under [`ID`], so a
/// press on the bell, `hyprshell notifs center` and a keybind all reach the same surface rather than stacking
/// copies of it.
pub fn toggle() {
    crate::core::shell::toggle_window(ID, open_sidebar);
}

pub fn open() {
    if !crate::core::shell::window_is_open(ID) {
        toggle();
    }
}

pub fn close() {
    crate::core::shell::close(ID);
}

pub fn is_open() -> bool {
    crate::core::shell::window_is_open(ID)
}

fn open_sidebar() -> SurfaceToken {
    let config = crate::core::shell::config().unwrap_or_else(|| Arc::new(Config::default()));
    let output = crate::core::shell::focused_output();
    // `open_surface` on the platform crate rather than `telar::open_surface`: this is a full-height docked surface
    // with its own layer and anchor, not one of the placements the surface host describes.
    let handle = open_surface(
        layer_config(&config, output.clone()),
        SidebarApp {
            config: Arc::clone(&config),
            output,
        },
    );
    // Wrapped as a token so the shell's own registry owns it like any other panel: `SurfaceHandle` already
    // implements the control trait the token wants, which is what lets a platform surface be toggled by id.
    SurfaceToken::new(Box::new(handle))
}

fn layer_config(config: &Config, output: Option<String>) -> LayerConfig {
    let sidebar = &config.sidebar;
    let thickness = sidebar.thickness();
    let (anchor, size) = match sidebar.edge {
        Edge::Left => (Anchor::LEFT | Anchor::TOP | Anchor::BOTTOM, (thickness, 0)),
        Edge::Right => (Anchor::RIGHT | Anchor::TOP | Anchor::BOTTOM, (thickness, 0)),
        Edge::Top => (Anchor::TOP | Anchor::LEFT | Anchor::RIGHT, (0, thickness)),
        Edge::Bottom => (
            Anchor::BOTTOM | Anchor::LEFT | Anchor::RIGHT,
            (0, thickness),
        ),
    };
    LayerConfig {
        output,
        layer: Layer::Overlay,
        anchor,
        // Zero, not -1: the compositor has already cleared the bars for us, and the shared panel margin is the
        // only extra distance a panel of any kind puts between itself and them.
        exclusive_zone: 0,
        size,
        margin: config.panel_margin(sidebar.edge),
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: "hyprshell-sidebar".to_string(),
        reserve_only: false,
        input_transparent: false,
        interactive_input_region: false,
    }
}

struct SidebarApp {
    config: Arc<Config>,
    output: Option<String>,
}

impl App for SidebarApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let theme = self.config.resolve_theme();
        set_theme(theme);
        crate::shared::services::locale::attach(self.config.language());
        // The panels this surface hosts read their settings off the surface env, exactly as they do inside a bar's
        // drawer — without it the history would fall back to defaults while the drawer used the user's config.
        set_surface_env(SurfaceEnv {
            edge: self.config.sidebar.edge,
            bar_size: self.config.bars.get(self.config.sidebar.edge).size,
            output: self.output.clone(),
            config: Arc::clone(&self.config),
        });
        crate::modules::drawer::set_content_radius(
            self.config.panel_radius(self.config.sidebar.edge),
        );
        let content = body(&self.config).expect("sidebar build failed");
        Box::new(SurfaceRoot::new(content).expect("sidebar surface root"))
    }

    fn clear_color(&self) -> Option<Color> {
        None
    }

    fn window_config(&self) -> Option<WindowConfig> {
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }
}

fn body(config: &Config) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let radius = config.panel_radius(config.sidebar.edge);

    let mut children: Vec<Box<dyn LayoutItem>> = vec![header(theme)?];
    if config.sidebar.show_toggles {
        children.push(crate::modules::utilities::toggles_grid(theme)?);
    }
    if config.sidebar.show_history {
        children.push(crate::modules::notifications::bell_panel()?);
    }

    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(14.0)
            .width(SizeDimension::Percent(1.0)),
        children,
    )?;
    // Scrolled, because a morning's notifications are taller than any screen — the one thing the bell drawer,
    // which sizes to its content, cannot do.
    let scroll = LayoutScrollArea::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        Box::new(column),
    )?;
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .padding_all(16.0)
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0)),
        move |_| RectStyle::filled(crate::modules::drawer::panel_fill(), radius),
        vec![Box::new(scroll)],
    )?))
}

/// The title and the way out.
///
/// The close button is not decoration: a surface docked to an edge has no "outside" for a press to land in, and
/// this one takes no keyboard on purpose — a centre held open while the user works must not keep focus away from
/// what they are typing in — so Escape never reaches it either. Without the ✕ the only way to dismiss it is the
/// IPC command that opened it, which is not a way a user has.
fn header(theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::auto(
        || telar::t!("sidebar.title"),
        LayoutStyle::new().flex_grow(1.0),
        move || {
            theme
                .text_style(FontRole::Title, theme.text)
                .with_weight(700)
        },
    )?;
    let glyph = crate::icon_view(|| "x".to_string(), move || theme.text, 18.0)?;
    let close_button = StyledContainer::new(
        LayoutStyle::new()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .padding_all(6.0)
            .flex_shrink(0.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![glyph],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    // Through the registry rather than `request_close`, so `panel list` and a second `notifs center` agree with
    // what is on screen the moment the button is pressed.
    .on_press(close);

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![box_item(title), Box::new(close_button)],
    )?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(edge: Edge) -> Config {
        Config {
            sidebar: crate::core::config::SidebarConfig {
                edge,
                ..crate::core::config::SidebarConfig::default()
            },
            ..Config::default()
        }
    }

    #[test]
    fn the_centre_docks_full_length_on_every_edge() {
        for edge in Edge::ALL {
            let layer = layer_config(&config(edge), None);
            let (across, along) = if edge.is_vertical() {
                (layer.size.0, layer.size.1)
            } else {
                (layer.size.1, layer.size.0)
            };
            assert_eq!(across, 400, "{edge:?} is as thick as it was configured");
            assert_eq!(along, 0, "{edge:?} spans the whole edge");
            assert_eq!(
                layer.exclusive_zone, 0,
                "the compositor has already cleared the bars; reserving again would double the gap"
            );
        }
    }

    #[test]
    fn a_hand_edited_size_cannot_cover_the_screen_or_vanish() {
        let tiny = crate::core::config::SidebarConfig {
            size: 10,
            ..crate::core::config::SidebarConfig::default()
        };
        assert_eq!(tiny.thickness(), 240);
        let huge = crate::core::config::SidebarConfig { size: 9000, ..tiny };
        assert_eq!(huge.thickness(), 1200);
    }

    /// The regression this exists for: the centre shipped with no way to dismiss it. It is docked to an edge, so
    /// there is no outside to press, and it takes no keyboard, so Escape never arrives — the ✕ is the only way
    /// out a user has, and it must be in the tree.
    #[test]
    fn the_header_carries_the_only_way_out() {
        telar::reset_layout_runtime();
        telar::set_theme(NordTheme::new());
        assert!(header(NordTheme::new()).is_ok());

        let layer = layer_config(&config(Edge::Right), None);
        assert!(
            matches!(layer.keyboard_interactivity, KeyboardInteractivity::None),
            "a centre held open while the user types must not hold their keyboard — which is exactly why it \
             cannot rely on Escape and needs the button above"
        );
    }

    #[test]
    fn the_body_builds_with_the_toggles_the_history_and_neither() {
        for (toggles, history) in [(true, true), (true, false), (false, false)] {
            telar::reset_layout_runtime();
            telar::set_theme(NordTheme::new());
            let config = Config {
                sidebar: crate::core::config::SidebarConfig {
                    show_toggles: toggles,
                    show_history: history,
                    ..crate::core::config::SidebarConfig::default()
                },
                ..Config::default()
            };
            assert!(
                body(&config).is_ok(),
                "toggles={toggles} history={history} builds"
            );
        }
    }
}
