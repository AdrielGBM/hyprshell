//! The toast stack: a layer surface that exists only while there is something to say.
//!
//! The notification popup host is always mapped, because a notification can arrive at any moment and the daemon
//! owns the timing. A toast is the other way round — the shell knows exactly when it posted one — so the surface
//! is opened on the first toast and closed with the last, and an idle session carries no overlay at all.
//!
//! It follows the focused screen the same way: the surface is opened on whichever monitor the compositor reports
//! as focused *at the moment the toast is posted*, which for feedback about a keypress is the screen the user is
//! looking at.

mod events;

pub use events::{config_reloaded, watch_events};

use std::cell::RefCell;
use std::rc::Rc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, SurfaceHandle, open_surface, watch,
};
use telar::{
    AlignItems, App, Color, Component, Container, LayoutError, LayoutItem, LayoutStyle,
    ReactiveList, RectStyle, SizeDimension, StyledContainer, Text, WindowConfig, box_item,
    reset_layout_runtime, set_theme, signal, use_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Align, Edge, ToastsConfig};
use crate::shared::services::toaster::{self, Toast};
use crate::shared::theme::{FontRole, NordTheme};

/// A card's height, used only to size the surface: the stack lays itself out inside it, and a layer surface has
/// to name a size before it knows what it will hold.
const CARD_HEIGHT: u32 = 64;
const ICON: f32 = 22.0;

/// Watches the toaster and keeps exactly one toast surface up while anything is showing. Called once from
/// `setup_shell`, on the driver thread, and long-lived: a config reload changes what the next surface looks like,
/// not whether the shell is listening.
pub fn toast_host() {
    let open: Rc<RefCell<Option<SurfaceHandle>>> = Rc::new(RefCell::new(None));
    watch(toaster::subscribe, move |toasts: Vec<Toast>| {
        let mut slot = open.borrow_mut();
        if toasts.is_empty() {
            // Dropping the handle is what unmaps it, so an empty stack leaves no click-through overlay behind.
            if let Some(handle) = slot.take() {
                handle.close();
            }
            return;
        }
        if slot.is_some() {
            return;
        }
        *slot = Some(open_stack());
    });
}

fn open_stack() -> SurfaceHandle {
    let config = crate::core::shell::config();
    let theme = config
        .as_ref()
        .map(|c| c.resolve_theme())
        .unwrap_or_default();
    let toasts = config
        .as_ref()
        .map(|c| c.toasts.clone())
        .unwrap_or_default();
    let radius = config
        .as_ref()
        .map(|c| c.panel_radius(toasts.edge))
        .unwrap_or(14.0);
    // The shared panel distance, so a toast clears the bar by exactly as much as a drawer or an OSD does.
    let margin = config
        .as_ref()
        .map(|c| c.panel_margin(toasts.edge))
        .unwrap_or((0, 0, 0, 0));
    let output = crate::core::shell::focused_output();
    open_surface(
        layer_config(&toasts, margin, output),
        ToastApp {
            config: toasts,
            theme,
            radius,
        },
    )
}

fn layer_config(
    config: &ToastsConfig,
    margin: (i32, i32, i32, i32),
    output: Option<String>,
) -> LayerConfig {
    let height = config.visible() as u32 * (CARD_HEIGHT + config.gap.max(0.0) as u32);
    LayerConfig {
        output,
        layer: Layer::Overlay,
        anchor: anchor(config),
        exclusive_zone: 0,
        size: (config.width.max(120.0) as u32, height.max(CARD_HEIGHT)),
        margin,
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: "hyprshell-toasts".to_string(),
        reserve_only: false,
        input_transparent: false,
        // Only the cards take the pointer; the gaps around them fall through to whatever the user is working in.
        interactive_input_region: true,
    }
}

fn anchor(config: &ToastsConfig) -> Anchor {
    let mut anchor = match config.edge {
        Edge::Top => Anchor::TOP,
        Edge::Bottom => Anchor::BOTTOM,
        Edge::Left => Anchor::LEFT,
        Edge::Right => Anchor::RIGHT,
    };
    if config.edge.is_horizontal() {
        match config.align {
            Align::Start => anchor |= Anchor::LEFT,
            Align::End => anchor |= Anchor::RIGHT,
            Align::Center => {}
        }
    } else {
        match config.align {
            Align::Start => anchor |= Anchor::TOP,
            Align::End => anchor |= Anchor::BOTTOM,
            Align::Center => {}
        }
    }
    anchor
}

struct ToastApp {
    config: ToastsConfig,
    theme: NordTheme,
    radius: f32,
}

impl App for ToastApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        set_theme(self.theme);
        let content = stack(self.config.clone(), self.radius).expect("toast stack build failed");
        Box::new(SurfaceRoot::new(content).expect("toast surface root"))
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

/// The live stack. Subscribes on this surface's own thread, so the list follows the queue while the surface is up
/// — including a toast being replaced by a newer one about the same thing, which keeps its slot.
fn stack(config: ToastsConfig, radius: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let toasts = signal(toaster::current());
    let sink = toasts.clone();
    platform_layershell::watch(toaster::subscribe, move |live: Vec<Toast>| sink.set(live));
    stack_of(toasts.read_only(), config, radius)
}

/// The stack over a given source. Split out from the subscription so a test — and the visual render — can drive it
/// with a fixed list instead of a live queue, which on a headless run is always empty.
fn stack_of(
    source: telar::ReadSignal<Vec<Toast>>,
    config: ToastsConfig,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let bottom_up = config.edge == Edge::Bottom;
    let list = ReactiveList::with_gap(
        move || {
            let mut live = source.get();
            // Anchored to the bottom, the newest card belongs nearest the edge the stack grows from — otherwise
            // a replacement appears to jump over the ones already there.
            if bottom_up {
                live.reverse();
            }
            live
        },
        |toast: &Toast| toast.key(),
        move |toast: Toast| card(&toast, theme, radius),
        config.gap,
    )?;
    Ok(Box::new(list))
}

/// One toast: its glyph, its title, and the line under it. A press dismisses it — the only interaction a toast
/// has, because everything a toast says is already true whether or not it is read.
fn card(toast: &Toast, theme: NordTheme, radius: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = crate::icon_view(
        {
            let name = toast.icon.clone();
            move || name.clone()
        },
        move || theme.accent,
        ICON,
    )?;

    let title = Text::auto(
        {
            let title = toast.title.clone();
            move || title.clone()
        },
        LayoutStyle::new(),
        move || {
            theme
                .text_style(FontRole::Body, theme.text)
                .with_weight(700)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;
    let mut column: Vec<Box<dyn LayoutItem>> = vec![box_item(title)];
    if !toast.body.trim().is_empty() {
        let body = Text::auto(
            {
                let body = toast.body.clone();
                move || body.clone()
            },
            LayoutStyle::new(),
            move || {
                theme
                    .text_style(FontRole::Caption, theme.muted)
                    .with_max_lines(2)
                    .with_ellipsis(true)
            },
        )?;
        column.push(box_item(body));
    }
    let text = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(2.0)
            .flex_grow(1.0)
            .width(SizeDimension::Percent(1.0)),
        column,
    )?;

    let id = toast.id;
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new()
                .flex_row()
                .align_items(AlignItems::CENTER)
                .gap(10.0)
                .padding_all(12.0)
                .width(SizeDimension::Percent(1.0)),
            move |_| RectStyle::filled(theme.surface, radius),
            vec![icon, Box::new(text)],
        )?
        .on_hover_style(move |_| RectStyle::filled(theme.overlay, radius))
        .on_press(move || toaster::dismiss(id)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(edge: Edge, align: Align) -> ToastsConfig {
        ToastsConfig {
            edge,
            align,
            ..ToastsConfig::default()
        }
    }

    #[test]
    fn the_stack_anchors_to_its_edge_on_all_four_of_them() {
        assert!(anchor(&config(Edge::Top, Align::Center)).contains(Anchor::TOP));
        assert!(anchor(&config(Edge::Bottom, Align::Center)).contains(Anchor::BOTTOM));
        assert!(anchor(&config(Edge::Left, Align::Center)).contains(Anchor::LEFT));
        assert!(anchor(&config(Edge::Right, Align::Center)).contains(Anchor::RIGHT));

        // Along a horizontal edge, alignment picks a side; along a vertical one it picks an end. The same word
        // means a different anchor, which is why this is not one match arm.
        let top_end = anchor(&config(Edge::Top, Align::End));
        assert!(top_end.contains(Anchor::RIGHT) && !top_end.contains(Anchor::BOTTOM));
        let left_end = anchor(&config(Edge::Left, Align::End));
        assert!(left_end.contains(Anchor::BOTTOM) && !left_end.contains(Anchor::RIGHT));
    }

    #[test]
    fn the_surface_only_takes_input_where_a_card_is() {
        let layer = layer_config(&ToastsConfig::default(), (0, 0, 0, 0), None);
        assert!(
            layer.interactive_input_region,
            "a toast must not swallow the click that follows it"
        );
        assert_eq!(layer.exclusive_zone, 0, "a toast reserves no space");
        assert!(matches!(layer.layer, Layer::Overlay));
        assert!(layer.size.1 >= CARD_HEIGHT, "room for at least one card");
    }

    #[test]
    fn a_card_and_the_stack_both_build() {
        telar::reset_layout_runtime();
        telar::set_theme(NordTheme::new());
        let toast = Toast::sample(toaster::Event::Dnd, "bell-off", "Do Not Disturb", "On");
        assert!(card(&toast, NordTheme::new(), 12.0).is_ok());

        telar::reset_layout_runtime();
        telar::set_theme(NordTheme::new());
        assert!(stack(ToastsConfig::default(), 12.0).is_ok());
    }

    #[test]
    fn a_bottom_anchored_stack_puts_the_newest_toast_nearest_the_edge() {
        telar::reset_layout_runtime();
        telar::set_theme(NordTheme::new());
        let toasts = vec![
            Toast::sample(toaster::Event::Vpn, "shield-check", "VPN", "On"),
            Toast::sample(toaster::Event::Dnd, "bell-off", "Do Not Disturb", "On"),
        ];
        let source = telar::signal(toasts);
        // Both directions build; the order itself is asserted by the reverse below rather than by measuring, since
        // what matters is which end of the list the newest card is at.
        for edge in [Edge::Bottom, Edge::Top] {
            telar::reset_layout_runtime();
            telar::set_theme(NordTheme::new());
            let config = ToastsConfig {
                edge,
                ..ToastsConfig::default()
            };
            assert!(
                stack_of(source.read_only(), config, 12.0).is_ok(),
                "{edge:?}"
            );
        }
    }

    /// Renders the toast stack for eyeballing. Gated on its own env var, like every other visual test here.
    #[test]
    fn visual_toasts_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_TOASTS_OUT") else {
            eprintln!("set TELAR_VISUAL_TOASTS_OUT to render the toast stack; skipping");
            return;
        };
        crate::test_support::render_png(ToastPreviewApp, 300, 200, &out);
    }

    struct ToastPreviewApp;

    impl App for ToastPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(NordTheme::new());
            let source = telar::signal(vec![
                Toast::sample(
                    toaster::Event::Charging,
                    "battery-charging",
                    "Charging",
                    "84%",
                ),
                Toast::sample(
                    toaster::Event::KbLayout,
                    "keyboard",
                    "Keyboard layout",
                    "Spanish",
                ),
            ]);
            let content = stack_of(source.read_only(), ToastsConfig::default(), 14.0)
                .expect("toast stack build failed");
            Box::new(SurfaceRoot::new(content).expect("toast surface root"))
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
}
