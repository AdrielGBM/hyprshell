use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, SurfaceHandle, open_surface,
};
use rsx::{
    AlignItems, App, Color, Component, Container, JustifyContent, LayoutError, LayoutItem,
    LayoutStyle, ReactiveList, ReadSignal, RectStyle, SizeDimension, StyledContainer, Text,
    TextStyle, WindowConfig, memo, reset_layout_runtime, set_theme, signal, use_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Align, Config, Edge, NotificationsConfig};
use crate::shared::services::notifications::{self, Notification, SharedSnapshot, Snapshot, Urgency};
use crate::shared::theme::{FontRole, NordTheme};

fn urgency_color(urgency: Urgency, theme: &NordTheme) -> Color {
    match urgency {
        Urgency::Critical => theme.red,
        Urgency::Normal => theme.accent,
        Urgency::Low => theme.muted,
    }
}

/// The notifications to render right now: none under Do-Not-Disturb, otherwise most-recent first with `critical` urgency floated to the top, capped at `max_visible` (the rest stay queued in the daemon until a visible one clears).
fn visible(snapshot: &Snapshot, cfg: &NotificationsConfig) -> Vec<Notification> {
    if snapshot.dnd {
        return Vec::new();
    }
    let mut list = snapshot.active.clone();
    list.reverse();
    list.sort_by_key(|n| u8::from(n.urgency != Urgency::Critical));
    list.truncate(cfg.max_visible.max(1) as usize);
    list
}

fn notification_card(
    notification: &Notification,
    width: SizeDimension,
    theme: NordTheme,
    dismissible: bool,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let accent = urgency_color(notification.urgency, &theme);
    let summary = notification.summary.clone();
    let body = notification.body.clone();

    let dot = StyledContainer::new(
        LayoutStyle::new().width(8.0).height(8.0),
        move |_| RectStyle::filled(accent, 4.0),
        Vec::new(),
    )?;

    let summary_text = Text::new(
        move || summary.clone(),
        LayoutStyle::new(),
        move || {
            TextStyle::new(theme.font(FontRole::Body), theme.text)
                .with_weight(700)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;

    let mut column: Vec<Box<dyn LayoutItem>> = vec![Box::new(summary_text)];
    if !notification.body.is_empty() {
        let body_text = Text::auto(
            move || body.clone(),
            LayoutStyle::new(),
            move || {
                TextStyle::new(theme.font(FontRole::Caption), theme.muted)
                    .with_max_lines(4)
                    .with_ellipsis(true)
            },
        )?;
        column.push(Box::new(body_text));
    }
    let text_column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(3.0)
            .flex_grow(1.0)
            .width(SizeDimension::Percent(1.0)),
        column,
    )?;

    let children: Vec<Box<dyn LayoutItem>> = vec![Box::new(dot), Box::new(text_column)];
    let mut card = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .padding_all(12.0)
            .width(width),
        move |_| RectStyle::filled(theme.surface, radius),
        children,
    )?;
    if dismissible {
        let id = notification.id;
        card = card.on_press(move || notifications::close(id));
    }
    Ok(Box::new(card))
}

/// Builds the reactive card stack from a snapshot signal. Split out so tests can drive it with a fixed snapshot instead of a live subscription.
fn card_stack(
    snapshot: ReadSignal<SharedSnapshot>,
    cfg: NotificationsConfig,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let gap = cfg.gap;
    let source = {
        let cfg = cfg.clone();
        move || visible(&snapshot.get(), &cfg)
    };
    let width = cfg.width;
    let list = ReactiveList::new(
        source,
        |n: &Notification| n.id,
        move |n: Notification| notification_card(&n, width.into(), theme, false, radius),
    )?;
    // No outer padding: the surface's layer margin (`Config::panel_margin`) already floats the stack off the
    // bar and edges, so the cards sit exactly the shared panel distance from the screen — same as a drawer.
    let stack = Container::new(
        LayoutStyle::new().flex_column().gap(gap),
        vec![Box::new(list) as Box<dyn LayoutItem>],
    )?;
    Ok(Box::new(stack))
}

/// The popup surface content: subscribes to the daemon on this surface's thread and renders the live stack.
fn popup_content(
    cfg: NotificationsConfig,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let snapshot = signal(Arc::new(Snapshot::default()));
    let setter = snapshot.clone();
    // The producer hands its sender to the daemon and returns; the daemon then pushes snapshots here, updated on this surface's loop.
    platform_layershell::watch(
        notifications::subscribe,
        move |snap: SharedSnapshot| setter.set(snap),
    );
    card_stack(snapshot.read_only(), cfg, theme, radius)
}

struct PopupApp {
    cfg: NotificationsConfig,
    theme: NordTheme,
    radius: f32,
}

impl App for PopupApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        set_theme(self.theme);
        let content =
            popup_content(self.cfg.clone(), self.theme, self.radius).expect("notification content");
        Box::new(SurfaceRoot::new(content).expect("notification surface root"))
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

/// Layer-shell config for the popup surface: anchored per `[notifications] edge`/`align` (top-right by default), input-transparent so it never steals clicks from windows beneath, sized to hold `max_visible` cards. `margin` is the shared [`Config::panel_margin`](crate::Config), so the stack clears the bar by the same distance as a drawer or OSD.
fn popup_layer_config(
    cfg: &NotificationsConfig,
    margin: (i32, i32, i32, i32),
    output: Option<String>,
) -> LayerConfig {
    let width = cfg.width as u32;
    let height = (cfg.max_visible.max(1) * 132).min(4000);
    LayerConfig {
        output,
        layer: Layer::Overlay,
        anchor: popup_anchor(cfg),
        exclusive_zone: 0,
        size: (width, height),
        margin,
        keyboard_interactivity: KeyboardInteractivity::None,
        namespace: "hyprshell-notifications".to_string(),
        reserve_only: false,
        input_transparent: true,
    }
}

fn popup_anchor(cfg: &NotificationsConfig) -> Anchor {
    let mut anchor = match cfg.edge {
        Edge::Top => Anchor::TOP,
        Edge::Bottom => Anchor::BOTTOM,
        Edge::Left => Anchor::LEFT,
        Edge::Right => Anchor::RIGHT,
    };
    if cfg.edge.is_horizontal() {
        match cfg.align {
            Align::Start => anchor |= Anchor::LEFT,
            Align::End => anchor |= Anchor::RIGHT,
            Align::Center => {}
        }
    } else {
        match cfg.align {
            Align::Start => anchor |= Anchor::TOP,
            Align::End => anchor |= Anchor::BOTTOM,
            Align::Center => {}
        }
    }
    anchor
}

fn open_popup(
    cfg: &NotificationsConfig,
    theme: NordTheme,
    margin: (i32, i32, i32, i32),
    radius: f32,
    output: Option<String>,
) -> SurfaceHandle {
    open_surface(
        popup_layer_config(cfg, margin, output),
        PopupApp {
            cfg: cfg.clone(),
            theme,
            radius,
        },
    )
}

/// Spawns the persistent popup host: shows the notification surface on the focused monitor and moves it there whenever Hyprland's focus changes. The surface lives for the whole process (surviving bar config reloads); notification state lives in the daemon, so recreating the surface on a monitor switch loses nothing.
pub fn spawn_popup_host(config: Arc<Config>) {
    let _ = std::thread::Builder::new()
        .name("hyprshell-notif-host".to_string())
        .spawn(move || {
            let theme = config.resolve_theme();
            let cfg = config.notifications.clone();
            // The shared panel distance and bar-matching radius for the popup's edge, so notifications clear the bar and round their corners exactly like a drawer/OSD.
            let margin = config.panel_margin(cfg.edge);
            let radius = config.panel_radius(cfg.edge);
            let dir = crate::shared::services::hyprland::socket_dir();

            let mut output = dir
                .as_deref()
                .and_then(crate::shared::services::hyprland::focused_monitor);
            let mut handle = open_popup(&cfg, theme, margin, radius, output.clone());

            let events = dir
                .as_ref()
                .and_then(|d| UnixStream::connect(d.join(".socket2.sock")).ok());
            let Some(events) = events else {
                // No Hyprland event stream: keep the single surface up.
                loop {
                    std::thread::park();
                }
            };
            for line in BufReader::new(events).lines().map_while(Result::ok) {
                let Some(monitor) =
                    crate::shared::services::hyprland::monitor_from_focus_event(&line)
                else {
                    continue;
                };
                if output.as_deref() != Some(monitor.as_str()) {
                    output = Some(monitor);
                    handle.close();
                    handle = open_popup(&cfg, theme, margin, radius, output.clone());
                }
            }
        });
}

/// The bar chip: a bell whose glyph flips to `bell-off` under Do-Not-Disturb, with an unread-count badge. Subscribes to the daemon like any other module reflecting a shared service; registered with `.opens()` so a click drops the history panel.
pub fn bell_module() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let unread = signal(0u32);
    let dnd = signal(false);
    let unread_read = unread.read_only();
    let dnd_read = dnd.read_only();
    platform_layershell::watch(notifications::subscribe, move |snap: SharedSnapshot| {
        unread.set(snap.unread);
        dnd.set(snap.dnd);
    });

    let fg = crate::module_fg();
    let theme = use_theme::<NordTheme>();
    let glyph = {
        let dnd_read = dnd_read.clone();
        memo(move || if dnd_read.get() { "bell-off" } else { "bell" })
    };
    let icon = crate::icon_view(
        move || glyph.get().to_string(),
        {
            let fg = fg.clone();
            move || fg.get()
        },
        crate::icon_px(),
    )?;
    let badge = Text::auto(
        move || badge_text(unread_read.get()),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Caption), fg.get()).with_weight(700),
    )?;
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(4.0),
        vec![icon, Box::new(badge)],
    )?;
    Ok(Box::new(row))
}

fn badge_text(unread: u32) -> String {
    match unread {
        0 => String::new(),
        1..=99 => unread.to_string(),
        _ => "99+".to_string(),
    }
}

/// The drawer panel: a header (title, Do-Not-Disturb toggle, clear-all) over the full history, newest first, each card click-to-dismiss. Opening it marks the history read.
pub fn bell_panel() -> Result<Box<dyn LayoutItem>, LayoutError> {
    notifications::mark_read();
    let theme = use_theme::<NordTheme>();
    let snapshot = signal(notifications::snapshot_now().unwrap_or_default());
    let setter = snapshot.clone();
    platform_layershell::watch(notifications::subscribe, move |snap: SharedSnapshot| {
        setter.set(snap)
    });
    let read = snapshot.read_only();

    // The cards sit inside the panel (drawer or float), so they carry its (bar-matching) radius.
    let radius = crate::modules::drawer::content_radius();
    let header = panel_header(read.clone(), theme)?;
    let list = history_list(read, theme, radius)?;
    let panel = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(12.0)
            .width(SizeDimension::Percent(1.0)),
        vec![header, list],
    )?;
    Ok(Box::new(panel))
}

fn panel_header(
    read: ReadSignal<SharedSnapshot>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::auto(
        || "Notifications".to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Title), theme.text).with_weight(700),
    )?;
    let dnd_label = read.clone();
    let dnd_toggle = read.clone();
    let dnd = pill_button(
        move || {
            if dnd_label.get().dnd {
                "DND on".to_string()
            } else {
                "DND off".to_string()
            }
        },
        move || notifications::set_dnd(!dnd_toggle.peek().dnd),
        theme,
    )?;
    let clear = pill_button(|| "Clear all".to_string(), notifications::clear_all, theme)?;
    let actions = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(6.0),
        vec![dnd, clear],
    )?;
    let header = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(title), Box::new(actions)],
    )?;
    Ok(Box::new(header))
}

fn pill_button(
    label: impl Fn() -> String + 'static,
    on_press: impl Fn() + 'static,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(label, LayoutStyle::new(), move || {
        TextStyle::new(theme.font(FontRole::Caption), theme.text)
    })?;
    let pill = StyledContainer::new(
        LayoutStyle::new()
            .padding_horizontal(10.0)
            .padding_vertical(5.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![Box::new(text) as Box<dyn LayoutItem>],
    )?
    .on_press(on_press);
    Ok(Box::new(pill))
}

fn history_list(
    read: ReadSignal<SharedSnapshot>,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = move || {
        let mut list = read.get().active.clone();
        list.reverse();
        list
    };
    let list = ReactiveList::new(
        source,
        |n: &Notification| n.id,
        move |n: Notification| notification_card(&n, SizeDimension::Percent(1.0), theme, true, radius),
    )?;
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(list) as Box<dyn LayoutItem>],
    )?;
    Ok(Box::new(column))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn badge_caps_at_99_plus() {
        assert_eq!(badge_text(0), "");
        assert_eq!(badge_text(3), "3");
        assert_eq!(badge_text(99), "99");
        assert_eq!(badge_text(250), "99+");
    }

    #[test]
    fn dnd_hides_everything_and_max_visible_caps_the_rest() {
        let mk = |id: u32, urgency: Urgency| Notification {
            id,
            app_name: "a".into(),
            app_icon: String::new(),
            summary: format!("n{id}"),
            body: String::new(),
            actions: Vec::new(),
            urgency,
        };
        let cfg = NotificationsConfig {
            max_visible: 2,
            ..NotificationsConfig::default()
        };

        let snap = Snapshot {
            active: vec![
                mk(1, Urgency::Normal),
                mk(2, Urgency::Critical),
                mk(3, Urgency::Normal),
            ],
            unread: 3,
            dnd: false,
        };
        let shown = visible(&snap, &cfg);
        assert_eq!(shown.len(), 2, "capped at max_visible");
        assert_eq!(shown[0].id, 2, "critical floats to the top");

        let dnd = Snapshot {
            dnd: true,
            ..snap
        };
        assert!(visible(&dnd, &cfg).is_empty(), "DND suppresses all popups");
    }

    struct PreviewApp {
        snapshot: Snapshot,
        cfg: NotificationsConfig,
        theme: NordTheme,
    }

    impl App for PreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(self.theme);
            let signal = signal(Arc::new(self.snapshot.clone()));
            let content = card_stack(signal.read_only(), self.cfg.clone(), self.theme, 12.0)
                .expect("card stack");
            Box::new(SurfaceRoot::new(content).expect("preview root"))
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            None
        }
    }

    struct PanelPreviewApp {
        snapshot: Snapshot,
        theme: NordTheme,
    }

    impl App for PanelPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(self.theme);
            let signal = signal(Arc::new(self.snapshot.clone()));
            let read = signal.read_only();
            let header = panel_header(read.clone(), self.theme).expect("header");
            let list = history_list(read, self.theme, 12.0).expect("list");
            let panel = Container::new(
                LayoutStyle::new()
                    .flex_column()
                    .gap(12.0)
                    .padding_all(16.0)
                    .width(SizeDimension::Percent(1.0)),
                vec![header, list],
            )
            .expect("panel");
            Box::new(SurfaceRoot::new(Box::new(panel)).expect("panel root"))
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            Some(NordTheme::new().surface)
        }
    }

    fn sample_snapshot() -> Snapshot {
        let mk = |id: u32, app: &str, summary: &str, body: &str, urgency: Urgency| Notification {
            id,
            app_name: app.into(),
            app_icon: String::new(),
            summary: summary.into(),
            body: body.into(),
            actions: Vec::new(),
            urgency,
        };
        Snapshot {
            active: vec![
                mk(1, "Slack", "Ada Lovelace", "Are we still on for the review at 3?", Urgency::Normal),
                mk(2, "Battery", "Battery low", "12% remaining — plug in soon.", Urgency::Critical),
                mk(3, "Calendar", "Standup in 5 minutes", "", Urgency::Low),
            ],
            unread: 3,
            dnd: false,
        }
    }

    /// Renders the history panel. `RSX_VISUAL_PANEL_OUT=/tmp/p.png cargo test -p hyprshell --lib visual_panel -- --nocapture`.
    #[test]
    fn visual_panel_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_PANEL_OUT") else {
            eprintln!("set RSX_VISUAL_PANEL_OUT to render the panel; skipping");
            return;
        };
        crate::test_support::render_png(
            PanelPreviewApp {
                snapshot: sample_snapshot(),
                theme: NordTheme::new().with_accent("teal"),
            },
            340,
            360,
            &out,
        );
    }

    /// Renders the popup stack for eyeballing. `RSX_VISUAL_NOTIF_OUT=/tmp/n.png cargo test -p hyprshell --lib visual_notifications -- --nocapture`.
    #[test]
    fn visual_notifications_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_NOTIF_OUT") else {
            eprintln!("set RSX_VISUAL_NOTIF_OUT to render notifications; skipping");
            return;
        };
        let mk = |id: u32, app: &str, summary: &str, body: &str, urgency: Urgency| Notification {
            id,
            app_name: app.into(),
            app_icon: String::new(),
            summary: summary.into(),
            body: body.into(),
            actions: Vec::new(),
            urgency,
        };
        let snapshot = Snapshot {
            active: vec![
                mk(1, "Slack", "Ada Lovelace", "Are we still on for the review at 3?", Urgency::Normal),
                mk(2, "Battery", "Battery low", "12% remaining — plug in soon.", Urgency::Critical),
                mk(3, "Calendar", "Standup in 5 minutes", "", Urgency::Low),
            ],
            unread: 3,
            dnd: false,
        };
        crate::test_support::render_png(
            PreviewApp {
                snapshot,
                cfg: NotificationsConfig::default(),
                theme: NordTheme::new().with_accent("teal"),
            },
            NotificationsConfig::default().width as u32,
            360,
            &out,
        );
    }
}
