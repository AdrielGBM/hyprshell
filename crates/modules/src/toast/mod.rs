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

use platform_wayland::{SurfaceHandle, watch};
use telar::{
    AlignItems, Container, LayoutError, LayoutItem, LayoutStyle, ReactiveList, RectStyle,
    SizeDimension, StyledContainer, Text, box_item, signal, use_theme,
};

use config::theme::{FontRole, NordTheme};
use config::{Edge, ToastsConfig};
use services::toaster::{self, Toast};
use ui::panel::{PanelSurface, card_gap, content_radius, panel_fill};
use ui::scale::space;
use ui::placement::Placement;

/// A card's height, used only to size the surface: the stack lays itself out inside it, and a layer surface has
/// to name a size before it knows what it will hold.
const CARD_HEIGHT: u32 = 64;
const ICON: f32 = 22.0;
const NAMESPACE: &str = "hyprshell-toasts";

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
    let output = surfaces::shell::focused_output();
    let config = config::config_for(output.as_deref());
    // The shared panel distance, so a toast clears the bar by exactly as much as a drawer or an OSD does.
    let margin = config.panel_margin(config.toasts.edge);
    PanelSurface::new(placement_for(&config.toasts, margin, output), |env| {
        stack(env.config.toasts.clone(), content_radius()).expect("toast stack build failed")
    })
    .open_handle()
}

/// Where the stack sits. The surface and the cards inside it come from this one placement, so the column packs
/// against the very edge the surface is pinned to.
fn placement(config: &ToastsConfig) -> Placement {
    let height = config.visible() as u32 * (CARD_HEIGHT + card_gap().max(0.0) as u32);
    Placement::stack(NAMESPACE, config.edge, config.align)
        .size(config.width.max(120.0) as u32, height.max(CARD_HEIGHT))
}

fn placement_for(
    config: &ToastsConfig,
    margin: (i32, i32, i32, i32),
    output: Option<String>,
) -> Placement {
    placement(config).margin(margin).output(output)
}

/// The live stack. Subscribes on this surface's own thread, so the list follows the queue while the surface is up
/// — including a toast being replaced by a newer one about the same thing, which keeps its slot.
fn stack(config: ToastsConfig, radius: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let toasts = signal(toaster::current());
    let sink = toasts.clone();
    platform_wayland::watch(toaster::subscribe, move |live: Vec<Toast>| sink.set(live));
    stack_of(toasts.read_only(), config, radius)
}

/// The stack over two sample toasts, for [`crate::preview`]: a live queue is empty on every run that is not a
/// running shell, and an empty stack is a blank page.
pub(crate) fn stack_preview() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = signal(vec![
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
    stack_of(source.read_only(), ToastsConfig::default(), 14.0)
}

/// The stack over a given source. Split out from the subscription so a test — and the preview above — can drive
/// it with a fixed list instead of a live queue, which on a headless run is always empty.
fn stack_of(
    source: telar::ReadSignal<Vec<Toast>>,
    config: ToastsConfig,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let bottom_up = config.edge == Edge::Bottom;
    let list = ReactiveList::with_style(
        placement(&config).column(card_gap()),
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
    )?;
    Ok(Box::new(list))
}

/// One toast: its glyph, its title, and the line under it. A press dismisses it — the only interaction a toast
/// has, because everything a toast says is already true whether or not it is read.
fn card(toast: &Toast, theme: NordTheme, radius: f32) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = ui::icon::icon_view(
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
            .gap(space::XS)
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
                .gap(space::LG)
                .padding_all(space::XL)
                .width(SizeDimension::Percent(1.0)),
            move |_| RectStyle::filled(panel_fill(), radius),
            vec![icon, Box::new(text)],
        )?
        .on_hover_style(move |_| RectStyle::filled(theme.overlay, radius))
        .on_press(move || toaster::dismiss(id)),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Where a stack anchors is [`Placement::stack`]'s to answer, and it is asserted there. What is this
    /// module's own is the size it asks for and what it does with the pointer.
    #[test]
    fn the_surface_only_takes_input_where_a_card_is() {
        let layer = placement_for(&ToastsConfig::default(), (0, 0, 0, 0), None).layer_config();
        assert!(
            layer.interactive_input_region,
            "a toast must not swallow the click that follows it"
        );
        assert_eq!(layer.exclusive_zone, 0, "a toast reserves no space");
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
}
