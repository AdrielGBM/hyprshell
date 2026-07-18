use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use platform_layershell::{
    Anchor, KeyboardInteractivity, Layer, LayerConfig, SurfaceHandle, open_surface,
};
use rsx::{
    AlignItems, App, Color, Component, Container, Image, ImageData, ImageFilter, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, ObjectFit, ReactiveList, ReadSignal, RectStyle,
    SizeDimension, StyledContainer, Text, TextStyle, WindowConfig, box_item, memo,
    reset_layout_runtime, set_theme, signal, use_theme,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Align, Config, Edge, NotificationsConfig};
use crate::shared::services::notifications::{self, Notification, SharedSnapshot, Snapshot, Urgency};
use crate::shared::theme::{FontRole, NordTheme};

/// Flattens the freedesktop notification body's limited HTML markup to plain text: `<br>` becomes a newline,
/// every other tag is dropped (keeping its inner text, and an `<img>`'s `alt`), then the entities are decoded.
/// rsx `Text` renders a single style, so inline bold/italic/links can't be styled — this at least stops the
/// raw tags from showing.
fn plain_text(markup: &str) -> String {
    let mut out = String::with_capacity(markup.len());
    let mut chars = markup.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '<' {
            out.push(c);
            continue;
        }
        // Read the whole tag.
        let mut tag = String::new();
        for t in chars.by_ref() {
            if t == '>' {
                break;
            }
            tag.push(t);
        }
        let lower = tag.trim().to_ascii_lowercase();
        if lower == "br" || lower == "br/" || lower.starts_with("br ") {
            out.push('\n');
        } else if lower.starts_with("img")
            && let Some(alt) = attr_value(&tag, "alt")
        {
            out.push_str(&alt);
        }
        // All other tags (b, i, u, a, closing tags) are dropped, keeping their inner text.
    }
    decode_entities(&out)
}

/// The value of `name="..."` (or `name='...'`) within a tag body, if present.
fn attr_value(tag: &str, name: &str) -> Option<String> {
    let start = tag.find(&format!("{name}="))? + name.len() + 1;
    let rest = &tag[start..];
    let quote = rest.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let end = rest[1..].find(quote)?;
    Some(rest[1..1 + end].to_string())
}

fn decode_entities(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        let after = &rest[amp..];
        let Some(semi) = after.find(';').filter(|&s| s <= 8) else {
            out.push('&');
            rest = &after[1..];
            continue;
        };
        let entity = &after[1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" => Some('\''),
            _ => entity
                .strip_prefix('#')
                .and_then(|n| {
                    n.strip_prefix('x')
                        .and_then(|h| u32::from_str_radix(h, 16).ok())
                        .or_else(|| n.parse().ok())
                })
                .and_then(char::from_u32),
        };
        match decoded {
            Some(ch) => {
                out.push(ch);
                rest = &after[semi + 1..];
            }
            None => {
                out.push('&');
                rest = &after[1..];
            }
        }
    }
    out.push_str(rest);
    out
}

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
    // Only fresh arrivals pop up; notifications restored from persisted history stay in the panel, unpopped.
    let mut list: Vec<Notification> = snapshot.active.iter().filter(|n| n.popup).cloned().collect();
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
    let body = plain_text(&notification.body);

    let leading = leading_visual(notification, accent)?;

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
    if !body.is_empty() {
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
    // Action buttons only in the panel: popups are click-through, so their buttons wouldn't be tappable.
    if dismissible && let Some(actions) = action_buttons(notification, theme)? {
        column.push(actions);
    }
    let text_column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(3.0)
            .flex_grow(1.0)
            .width(SizeDimension::Percent(1.0)),
        column,
    )?;

    let children: Vec<Box<dyn LayoutItem>> = vec![leading, Box::new(text_column)];
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

/// The card's leading visual: the notification's own image when it carried one, else the urgency dot.
fn leading_visual(
    notification: &Notification,
    accent: Color,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if let Some(img) = &notification.image {
        let data = Arc::new(ImageData::new(img.rgba.clone(), img.width, img.height));
        let image = Image::new(
            LayoutStyle::new().width(36.0).height(36.0).flex_shrink(0.0),
            move || data.clone(),
            || ImageFilter::Linear,
            || ObjectFit::Cover,
        )?;
        return Ok(Box::new(image));
    }
    let dot = StyledContainer::new(
        LayoutStyle::new().width(8.0).height(8.0).flex_shrink(0.0),
        move |_| RectStyle::filled(accent, 4.0),
        Vec::new(),
    )?;
    Ok(Box::new(dot))
}

/// A wrapping row of the notification's non-default actions, or `None` when it has none. Tapping one invokes
/// it (emitting `ActionInvoked`) and closes the notification.
fn action_buttons(
    notification: &Notification,
    theme: NordTheme,
) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
    let buttons: Vec<Box<dyn LayoutItem>> = notification
        .actions
        .chunks_exact(2)
        .filter(|pair| pair[0] != "default")
        .map(|pair| action_pill(notification.id, pair[0].clone(), pair[1].clone(), theme))
        .collect::<Result<_, _>>()?;
    if buttons.is_empty() {
        return Ok(None);
    }
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .width(SizeDimension::Percent(1.0)),
        buttons,
    )?;
    Ok(Some(Box::new(row)))
}

fn action_pill(
    id: u32,
    key: String,
    label: String,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(
        move || label.clone(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Caption), theme.text),
    )?;
    let pill = StyledContainer::new(
        LayoutStyle::new()
            .padding_horizontal(10.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.overlay, 8.0),
        vec![box_item(text)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay.darken(0.12), 8.0))
    .on_press(move || notifications::invoke_action(id, &key));
    Ok(Box::new(pill))
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
    fn plain_text_strips_markup_and_decodes_entities() {
        assert_eq!(plain_text("<b>Bold</b> &amp; <i>italic</i>"), "Bold & italic");
        assert_eq!(plain_text("line1<br>line2"), "line1\nline2");
        assert_eq!(plain_text(r#"<a href="http://x">click</a>"#), "click");
        assert_eq!(plain_text(r#"<img src="a.png" alt="pic"/>"#), "pic");
        assert_eq!(plain_text("&lt;tag&gt; &#65;&#x42;"), "<tag> AB");
        assert_eq!(plain_text("plain"), "plain");
        // A stray, unterminated entity is left as a literal ampersand.
        assert_eq!(plain_text("Q&A"), "Q&A");
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
            popup: true,
            image: None,
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

    #[test]
    fn restored_history_stays_in_the_panel_and_never_pops_up() {
        let mk = |id: u32, popup: bool| Notification {
            id,
            app_name: "a".into(),
            app_icon: String::new(),
            summary: format!("n{id}"),
            body: String::new(),
            actions: Vec::new(),
            urgency: Urgency::Normal,
            popup,
            image: None,
        };
        // One restored (non-popping) and one fresh notification: only the fresh one becomes a popup, while the
        // history panel (which reads all of `active`) still holds both.
        let snap = Snapshot {
            active: vec![mk(1, false), mk(2, true)],
            unread: 1,
            dnd: false,
        };
        let shown = visible(&snap, &NotificationsConfig::default());
        assert_eq!(shown.len(), 1, "only the fresh notification pops up");
        assert_eq!(shown[0].id, 2);
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
            popup: true,
            image: None,
        };
        Snapshot {
            active: vec![
                Notification {
                    actions: vec!["reply".into(), "Reply".into(), "archive".into(), "Archive".into()],
                    ..mk(1, "Slack", "Ada Lovelace", "Still on for the review at <b>3pm</b>? &amp; bring notes", Urgency::Normal)
                },
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
            popup: true,
            image: None,
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
