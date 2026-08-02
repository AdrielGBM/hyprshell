use std::collections::BTreeSet;
use std::io::{BufRead, BufReader};
use std::os::unix::net::UnixStream;
use std::sync::Arc;

use std::cell::RefCell;
use std::rc::Rc;

use platform_layershell::{LayerConfig, SurfaceHandle, open_surface, watch};
use telar::{
    AlignItems, App, Color, Component, Container, Image, ImageData, ImageFilter, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, Memo, ObjectFit, ReactiveList, ReadSignal, RectStyle,
    RichText, RwSignal, SizeDimension, StyledContainer, Text, TextRun, WindowConfig, box_item,
    memo, reset_layout_runtime, set_theme, signal, use_theme,
};

use config::surface_env;
use config::theme::{FontRole, NordTheme};
use config::{FullscreenPopups, NotificationsConfig};
use services::hyprland::{self, ActiveWindow, Client};
use services::notifications::{self, Notification, SharedSnapshot, Snapshot, Urgency};
use ui::placement::Placement;
use ui::surface_root::SurfaceRoot;

/// Parses the freedesktop notification body's limited HTML markup into styled runs for a [`RichText`]: `<b>`/
/// `<strong>` bold, `<i>`/`<em>` italic, `<a href>` links (painted `link_color`), `<br>` a newline, and an
/// `<img>`'s `alt` text. Each run carries its own weight/slant/colour; unknown tags are dropped, keeping their
/// inner text, and entities are decoded per segment.
fn body_runs(markup: &str, text_color: Color, link_color: Color) -> Vec<TextRun> {
    let mut runs: Vec<TextRun> = Vec::new();
    let mut current = String::new();
    let (mut bold, mut italic, mut link) = (0i32, 0i32, 0i32);
    let mut chars = markup.chars();

    while let Some(c) = chars.next() {
        if c != '<' {
            current.push(c);
            continue;
        }
        let mut tag = String::new();
        for t in chars.by_ref() {
            if t == '>' {
                break;
            }
            tag.push(t);
        }
        let lower = tag.trim().to_ascii_lowercase();
        // `<br>` and `<img>` don't change style: they stay within the current run.
        if lower == "br" || lower == "br/" || lower.starts_with("br ") {
            current.push('\n');
            continue;
        }
        if lower.starts_with("img") {
            if let Some(alt) = attr_value(&tag, "alt") {
                current.push_str(&alt);
            }
            continue;
        }
        // A style-changing tag ends the current run before the new style takes effect. `kind`: 0 bold, 1
        // italic, 2 link — chosen so the counter can be bumped after the borrow of `current` ends.
        let (kind, delta): (u8, i32) = match lower.as_str() {
            "b" | "strong" => (0, 1),
            "/b" | "/strong" => (0, -1),
            "i" | "em" => (1, 1),
            "/i" | "/em" => (1, -1),
            "/a" => (2, -1),
            _ if lower == "a" || lower.starts_with("a ") => (2, 1),
            _ => continue,
        };
        push_run(
            &mut runs,
            &mut current,
            bold > 0,
            italic > 0,
            link > 0,
            text_color,
            link_color,
        );
        match kind {
            0 => bold = (bold + delta).max(0),
            1 => italic = (italic + delta).max(0),
            _ => link = (link + delta).max(0),
        }
    }
    push_run(
        &mut runs,
        &mut current,
        bold > 0,
        italic > 0,
        link > 0,
        text_color,
        link_color,
    );
    runs
}

/// Flushes the accumulated segment as one [`TextRun`] with the active weight/slant/colour, decoding entities.
#[allow(clippy::too_many_arguments)]
fn push_run(
    runs: &mut Vec<TextRun>,
    current: &mut String,
    bold: bool,
    italic: bool,
    link: bool,
    text_color: Color,
    link_color: Color,
) {
    if current.is_empty() {
        return;
    }
    let text = decode_entities(current);
    current.clear();
    if text.is_empty() {
        return;
    }
    runs.push(TextRun {
        text: Arc::from(text.as_str()),
        weight: if bold { 700 } else { 400 },
        italic,
        color: if link { link_color } else { text_color },
    });
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

/// The notifications to render right now: none under Do-Not-Disturb, otherwise most-recent first with `critical` urgency floated to the top, capped at `max_visible` (the rest stay queued in the daemon until a visible one clears). `fullscreen` reports whether the focused window is covering the screen, which `[notifications] fullscreen` decides what to do about.
fn visible(snapshot: &Snapshot, cfg: &NotificationsConfig, fullscreen: bool) -> Vec<Notification> {
    if snapshot.dnd {
        return Vec::new();
    }
    // Only fresh arrivals pop up; notifications restored from persisted history stay in the panel, unpopped.
    let mut list: Vec<Notification> = snapshot
        .active
        .iter()
        .filter(|n| n.popup)
        .cloned()
        .collect();
    if fullscreen {
        list.retain(|n| cfg.fullscreen.allows(n.urgency));
    }
    list.reverse();
    list.sort_by_key(|n| u8::from(n.urgency != Urgency::Critical));
    list.truncate(cfg.max_visible.max(1) as usize);
    list
}

/// Whether the focused window is covering the screen, as a value the popup stack re-reads.
///
/// Two subscriptions rather than one: `j/clients` carries each window's fullscreen flag but not which one has
/// focus, and `j/activewindow` the reverse. Neither is opened while the policy is `on`, since nothing would
/// read the answer — a shell that never suppresses a popup should not be listening to the compositor for it.
fn fullscreen_focus(cfg: &NotificationsConfig) -> Option<Memo<bool>> {
    if cfg.fullscreen == FullscreenPopups::On {
        return None;
    }
    let clients = signal(Vec::<Client>::new());
    let active = signal(ActiveWindow::default());
    let publish_clients = clients.clone();
    platform_layershell::watch(hyprland::subscribe_clients, move |list: Vec<Client>| {
        publish_clients.set(list)
    });
    let publish_active = active.clone();
    platform_layershell::watch(
        hyprland::subscribe_active_window,
        move |window: ActiveWindow| publish_active.set(window),
    );
    let (clients, active) = (clients.read_only(), active.read_only());
    Some(memo(move || {
        let address = active.get().address;
        !address.is_empty()
            && clients
                .get()
                .iter()
                .any(|c| c.address == address && c.fullscreen)
    }))
}

/// Everything a card needs beyond the notification itself, so the popup stack and the history panel draw the
/// same card from the same `[notifications]` settings instead of each carrying its own arguments.
#[derive(Clone, Copy)]
struct CardStyle {
    theme: NordTheme,
    radius: f32,
    /// The body's line cap, or `None` for the whole body (`open_expanded`).
    body_lines: Option<u16>,
    /// A tap on the card body invokes the notification's `default` action, when it declares one.
    action_on_click: bool,
    /// How far sideways the card must be dragged before letting go dismisses it, in px; `None` is off.
    swipe: Option<f32>,
}

impl CardStyle {
    fn new(cfg: &NotificationsConfig, theme: NordTheme, radius: f32) -> Self {
        Self {
            theme,
            radius,
            body_lines: cfg.body_max_lines(),
            action_on_click: cfg.action_on_click,
            swipe: cfg.swipe_distance(cfg.width),
        }
    }
}

/// The `default` action's key, when the notification declares one — the spec's convention for what the
/// notification is *for* (open the message, the download, the calendar entry) rather than one of its buttons.
fn default_action_key(notification: &Notification) -> Option<String> {
    notification
        .actions
        .chunks_exact(2)
        .find(|pair| pair[0] == "default")
        .map(|pair| pair[0].clone())
}

fn notification_card(
    notification: &Notification,
    width: SizeDimension,
    style: CardStyle,
    dismiss: Option<fn(u32)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let CardStyle { theme, radius, .. } = style;
    let accent = urgency_color(notification.urgency, &theme);
    let summary = notification.summary.clone();
    let body = body_runs(&notification.body, theme.muted, theme.accent);

    let leading = leading_visual(notification, accent)?;

    let summary_text = Text::auto(
        move || summary.clone(),
        LayoutStyle::new(),
        move || {
            theme
                .text_style(FontRole::Body, theme.text)
                .with_weight(700)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;

    let mut column: Vec<Box<dyn LayoutItem>> = vec![Box::new(summary_text)];
    if !body.is_empty() {
        let body_lines = style.body_lines;
        let body_text = RichText::auto(
            move || body.clone(),
            LayoutStyle::new(),
            move || {
                let base = theme.text_style(FontRole::Caption, theme.muted);
                match body_lines {
                    Some(lines) => base.with_max_lines(lines).with_ellipsis(true),
                    None => base,
                }
            },
        )?;
        column.push(Box::new(body_text));
    }
    // Shown wherever the card is interactive (the panel, and now popups via their carved input region): an
    // action pill hit-tests before the card, so tapping one invokes it while tapping elsewhere dismisses.
    if dismiss.is_some()
        && let Some(actions) = action_buttons(notification, theme)?
    {
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
    // How far the card has been swiped, in px. A signal because the card follows the finger: the transform re-reads it every frame the pointer moves, and it snaps back to zero if the gesture is abandoned.
    let swiped = signal(0.0f32);
    let offset = swiped.read_only();
    let mut card = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .gap(10.0)
            .padding_all(12.0)
            .width(width),
        move |_| RectStyle::filled(theme.surface, radius),
        children,
    )?;
    if let Some(dismiss) = dismiss {
        let id = notification.id;
        let default_action = style
            .action_on_click
            .then(|| default_action_key(notification))
            .flatten();
        card = card.on_press(move || match &default_action {
            Some(key) => notifications::invoke_action(id, key),
            None => dismiss(id),
        });
        if let Some(threshold) = style.swipe {
            card = swipe_to_dismiss(card, swiped, offset.clone(), threshold, id);
        }
    }
    Ok(Box::new(card))
}

/// Makes a card follow a sideways drag and dismiss itself if it is let go past `threshold`.
///
/// The card fades as it travels, so the gesture says what it will do before it does it — a card that slid and
/// then sprang back with no visual difference reads as a failure rather than as a cancel. Below the threshold
/// the offset is simply set back to zero, which is the snap-back.
fn swipe_to_dismiss(
    card: StyledContainer,
    swiped: RwSignal<f32>,
    offset: ReadSignal<f32>,
    threshold: f32,
    id: u32,
) -> StyledContainer {
    // The drag reports the pointer local to the card, so the *start* has to be remembered to get a delta — a press near the right edge would otherwise read as an instant swipe of nearly the card's width.
    let start: Rc<RefCell<Option<f32>>> = Rc::new(RefCell::new(None));
    let began = Rc::clone(&start);
    let tracking = swiped.clone();
    let fade = offset.clone();
    card.on_drag(move |x, _y| {
        let from = *began.borrow_mut().get_or_insert(x);
        tracking.set(x - from);
    })
    .on_drag_end(move |x, _y| {
        let from = start.borrow_mut().take().unwrap_or(x);
        if (x - from).abs() >= threshold {
            notifications::close(id);
        } else {
            swiped.set(0.0);
        }
    })
    .with_transform(move |_rect| {
        let dx = offset.get();
        (dx != 0.0).then_some([1.0, 0.0, 0.0, 1.0, dx, 0.0])
    })
    .with_opacity(move || 1.0 - (fade.get().abs() / threshold).clamp(0.0, 0.85))
}

/// A card's list key. Keyed on what it *draws*, not on the notification's identity: a sender that edits a
/// notification in place (`replaces_id`) keeps its id while the summary and body turn over entirely, and a key
/// of just the id would leave the old card on screen.
fn card_key(notification: &Notification) -> String {
    format!(
        "{}\u{1}{}\u{1}{}",
        notification.id, notification.summary, notification.body
    )
}

/// The card's leading visual, in freedesktop priority: the notification's own raw image, then its resolved
/// application icon, else the urgency dot.
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
    if let Some(icon) = app_icon_visual(&notification.app_icon)? {
        return Ok(icon);
    }
    let dot = StyledContainer::new(
        LayoutStyle::new().width(8.0).height(8.0).flex_shrink(0.0),
        move |_| RectStyle::filled(accent, 4.0),
        Vec::new(),
    )?;
    Ok(Box::new(dot))
}

/// The resolved application icon as a 36px visual — an untinted SVG (keeping the app's own colours) or its
/// raster pixels — or `None` when the reference is empty or can't be resolved.
fn app_icon_visual(reference: &str) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
    ui::icon::app_icon_view(reference, 36.0)
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
        move || theme.text_style(FontRole::Caption, theme.text),
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
    fullscreen: Option<Memo<bool>>,
    cfg: NotificationsConfig,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let gap = cfg.gap;
    let style = CardStyle::new(&cfg, theme, radius);
    let source = {
        let cfg = cfg.clone();
        move || {
            let covering = fullscreen.as_ref().is_some_and(|f| f.get());
            visible(&snapshot.get(), &cfg, covering)
        }
    };
    let width = cfg.width;
    // The gap belongs on the list itself (it lays out the cards); a gap on a wrapper holding the single list
    // node separates nothing. This spacing is also what falls through the popup's carved input region.
    let list = ReactiveList::with_style(
        popup_placement(&cfg).column(gap),
        source,
        card_key,
        move |n: Notification| {
            notification_card(&n, width.into(), style, Some(notifications::expire))
        },
    )?;
    // No outer padding: the surface's layer margin (`Config::panel_margin`) already floats the stack off the
    // bar and edges, so the cards sit exactly the shared panel distance from the screen — same as a drawer.
    Ok(Box::new(list))
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
    platform_layershell::watch(notifications::subscribe, move |snap: SharedSnapshot| {
        setter.set(snap)
    });
    let fullscreen = fullscreen_focus(&cfg);
    card_stack(snapshot.read_only(), fullscreen, cfg, theme, radius)
}

struct PopupApp {
    output: Option<String>,
}

impl App for PopupApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let config = config::config_for(self.output.as_deref());
        let theme = config.resolve_theme();
        set_theme(theme);
        let radius = config.panel_radius(config.notifications.edge);
        let content = popup_content(config.notifications.clone(), theme, radius)
            .expect("notification content");
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

/// Layer-shell config for the popup surface: anchored per `[notifications] edge`/`align` (top-right by default), sized to hold `max_visible` cards. Its input region is carved from the cards (`interactive_input_region`), so a tap dismisses a popup while the gaps around them fall through to windows beneath. `margin` is the shared [`Config::panel_margin`](config::Config), so the stack clears the bar by the same distance as a drawer or OSD.
fn popup_layer_config(
    cfg: &NotificationsConfig,
    margin: (i32, i32, i32, i32),
    output: Option<String>,
) -> LayerConfig {
    popup_placement(cfg)
        .margin(margin)
        .output(output)
        .layer_config()
}

/// Where the popup stack sits. The surface and the column of cards inside it come from this one placement, so a
/// stack holding fewer cards than it is sized for still hugs the edge it is pinned to.
fn popup_placement(cfg: &NotificationsConfig) -> Placement {
    let width = cfg.width as u32;
    let height = (cfg.max_visible.max(1) * 132).min(4000);
    Placement::stack("hyprshell-notifications", cfg.edge, cfg.align).size(width, height)
}

/// The popup surface: which screen it is on, what keeps it up, and the layer-shell state the compositor holds
/// for it — the last so a config change can be renegotiated against it rather than opening a second surface.
struct Popup {
    output: Option<String>,
    handle: SurfaceHandle,
    layer: LayerConfig,
}

thread_local! {
    static POPUP: RefCell<Option<Popup>> = const { RefCell::new(None) };
}

/// The layer-shell configuration `output`'s popup should have, from the config that screen is running.
fn popup_config(output: Option<&str>) -> LayerConfig {
    let config = config::config_for(output);
    let cfg = &config.notifications;
    // The shared panel distance, so notifications clear the bar exactly like a drawer or an OSD does.
    popup_layer_config(
        cfg,
        config.panel_margin(cfg.edge),
        output.map(str::to_string),
    )
}

/// Puts the popup on `output`, replacing whatever screen it was on.
fn show_on(output: Option<String>) {
    let layer = popup_config(output.as_deref());
    let handle = open_surface(
        layer.clone(),
        PopupApp {
            output: output.clone(),
        },
    );
    POPUP.with(|popup| {
        *popup.borrow_mut() = Some(Popup {
            output,
            handle,
            layer,
        })
    });
}

/// Follows a config change: the popup takes the new edge, size and look where it stands.
///
/// It is chrome the config describes, like a bar, and it is reconciled like one — renegotiated and rebuilt in
/// place. Reopening it would be invisible (the surface is empty until something is posted) and wrong for the
/// same reason it is wrong for a bar: the popup that is up is the popup, and a notification arriving during a
/// reload must not land on a surface that is halfway through being replaced.
pub fn reconcile() {
    POPUP.with(|popup| {
        let mut popup = popup.borrow_mut();
        let Some(popup) = popup.as_mut() else {
            return;
        };
        let next = popup_config(popup.output.as_deref());
        let change = popup.layer.delta(&next);
        if !change.is_empty() {
            popup.handle.update(change);
        }
        popup.layer = next;
        popup.handle.rebuild();
    });
}

/// Sets up the notification popup host on the driver thread (called from `setup_shell`): shows the popup on the
/// focused monitor and moves it there whenever Hyprland's focus changes. The focus stream is read off-thread via
/// `watch`. Long-lived — it persists across config reloads (notification state lives in the daemon, and the
/// surface itself follows an edit through [`reconcile`] rather than being opened again).
pub fn popup_host() {
    let dir = services::hyprland::socket_dir();
    show_on(dir.as_deref().and_then(services::hyprland::focused_monitor));

    let producer_dir = dir.clone();
    watch(
        move |tx| {
            let Some(dir) = producer_dir else {
                return;
            };
            let Ok(events) = UnixStream::connect(dir.join(".socket2.sock")) else {
                return;
            };
            for line in BufReader::new(events).lines().map_while(Result::ok) {
                if let Some(monitor) = services::hyprland::monitor_from_focus_event(&line)
                    && !tx.send(monitor)
                {
                    break;
                }
            }
        },
        move |monitor: String| {
            let elsewhere = POPUP.with(|popup| {
                popup
                    .borrow()
                    .as_ref()
                    .is_none_or(|popup| popup.output.as_deref() != Some(monitor.as_str()))
            });
            // A move between screens *is* a new surface: a layer surface names its output when it is created.
            if elsewhere {
                show_on(Some(monitor));
            }
        },
    );
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

    let fg = ui::module::module_fg();
    let theme = use_theme::<NordTheme>();
    let glyph = {
        let dnd_read = dnd_read.clone();
        memo(move || if dnd_read.get() { "bell-off" } else { "bell" })
    };
    let icon = ui::icon::icon_view(
        move || glyph.get().to_string(),
        {
            let fg = fg.clone();
            move || fg.get()
        },
        ui::module::icon_px(),
    )?;
    let badge = Text::auto(
        move || badge_text(unread_read.get()),
        LayoutStyle::new(),
        move || {
            theme
                .text_style(FontRole::Caption, fg.get())
                .with_weight(700)
        },
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
    if let Some(env) = surface_env() {
        services::locale::attach(env.config.language());
    }
    let theme = use_theme::<NordTheme>();
    let snapshot = signal(notifications::snapshot_now().unwrap_or_default());
    let setter = snapshot.clone();
    platform_layershell::watch(notifications::subscribe, move |snap: SharedSnapshot| {
        setter.set(snap)
    });
    let read = snapshot.read_only();

    // The cards sit inside the panel (drawer or float), so they carry its (bar-matching) radius.
    let radius = surfaces::drawer::content_radius();
    let cfg = surface_env().map_or_else(NotificationsConfig::default, |env| {
        env.config.notifications.clone()
    });
    let header = panel_header(read.clone(), theme)?;
    let list = history_list(read, &cfg, theme, radius)?;
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
        || telar::t!("notifications.title"),
        LayoutStyle::new(),
        move || {
            theme
                .text_style(FontRole::Title, theme.text)
                .with_weight(700)
        },
    )?;
    let dnd_label = read.clone();
    let dnd_toggle = read.clone();
    let dnd = pill_button(
        move || {
            if dnd_label.get().dnd {
                telar::t!("notifications.dnd_on")
            } else {
                telar::t!("notifications.dnd_off")
            }
        },
        move || notifications::set_dnd(!dnd_toggle.peek().dnd),
        theme,
    )?;
    let clear = pill_button(
        || telar::t!("notifications.clear_all"),
        notifications::clear_all,
        theme,
    )?;
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
        theme.text_style(FontRole::Caption, theme.text)
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

/// One row of the history panel. Grouping turns a flat list of cards into a list of *rows*, so one keyed list
/// still draws the whole panel — a header, the cards under it, and the row that reveals the rest.
enum HistoryRow {
    Group {
        app: String,
        count: usize,
        muted: bool,
        expanded: bool,
    },
    Card(Notification),
    Expander {
        app: String,
        hidden: usize,
        expanded: bool,
    },
}

/// Every row's list key, on the same rule as [`card_key`]: a header redraws when its count, mute or expansion
/// changes, so all three belong in the key.
fn row_key(row: &HistoryRow) -> String {
    match row {
        HistoryRow::Group {
            app,
            count,
            muted,
            expanded,
        } => format!("g\u{1}{app}\u{1}{count}\u{1}{muted}\u{1}{expanded}"),
        HistoryRow::Card(n) => format!("c\u{1}{}", card_key(n)),
        HistoryRow::Expander {
            app,
            hidden,
            expanded,
        } => format!("x\u{1}{app}\u{1}{hidden}\u{1}{expanded}"),
    }
}

/// Lays the history out newest-first, grouped by application when `[notifications] group_by_app` asks for it.
///
/// A group is ordered by its most recent notification rather than by name, so the application that just spoke
/// is at the top; within it the cards run newest-first like everything else. `expanded` names the groups the
/// user has opened — it is the panel's own state, not the daemon's, so closing the drawer forgets it.
fn history_rows(
    snapshot: &Snapshot,
    cfg: &NotificationsConfig,
    expanded: &BTreeSet<String>,
) -> Vec<HistoryRow> {
    let newest_first: Vec<&Notification> = snapshot.active.iter().rev().collect();
    if !cfg.group_by_app {
        return newest_first
            .into_iter()
            .cloned()
            .map(HistoryRow::Card)
            .collect();
    }
    let mut apps: Vec<&str> = Vec::new();
    for n in &newest_first {
        if !apps.contains(&n.app_name.as_str()) {
            apps.push(&n.app_name);
        }
    }
    let mut rows = Vec::new();
    for app in apps {
        let group: Vec<&Notification> = newest_first
            .iter()
            .copied()
            .filter(|n| n.app_name == app)
            .collect();
        let is_expanded = expanded.contains(app);
        let shown = if is_expanded {
            group.len()
        } else {
            cfg.group_preview().min(group.len())
        };
        rows.push(HistoryRow::Group {
            app: app.to_string(),
            count: group.len(),
            muted: snapshot.is_muted(app),
            expanded: is_expanded,
        });
        rows.extend(
            group[..shown]
                .iter()
                .map(|n| HistoryRow::Card((*n).clone())),
        );
        if group.len() > cfg.group_preview() {
            rows.push(HistoryRow::Expander {
                app: app.to_string(),
                hidden: group.len() - shown,
                expanded: is_expanded,
            });
        }
    }
    rows
}

fn history_list(
    read: ReadSignal<SharedSnapshot>,
    cfg: &NotificationsConfig,
    theme: NordTheme,
    radius: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // A signal rather than a cell, because the row list derives from it: a plain value would change without anything asking the list to rebuild.
    let expanded = signal(BTreeSet::<String>::new());
    let style = CardStyle::new(cfg, theme, radius);
    let source = {
        let cfg = cfg.clone();
        let expanded = expanded.read_only();
        // Both signals read *out* before either is used: `with` holds the runtime's borrow for as long as its closure runs, so reading the snapshot inside the expanded set's `with` panics on the second borrow.
        move || {
            let snapshot = read.get();
            let open = expanded.get();
            history_rows(&snapshot, &cfg, &open)
        }
    };
    let toggle = expanded.clone();
    let build = move |row: HistoryRow| -> Result<Box<dyn LayoutItem>, LayoutError> {
        match row {
            HistoryRow::Group {
                app, count, muted, ..
            } => group_header(app, count, muted, toggle.clone(), theme),
            HistoryRow::Card(n) => notification_card(
                &n,
                SizeDimension::Percent(1.0),
                style,
                Some(notifications::close),
            ),
            HistoryRow::Expander {
                app,
                hidden,
                expanded,
            } => expander_row(app, hidden, expanded, toggle.clone(), theme),
        }
    };
    // Gap on the list itself (which lays the cards out); the wrapper only pins the full width so the
    // percent-width cards resolve against it.
    let list = ReactiveList::with_gap(source, row_key, build, cfg.gap)?;
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(list) as Box<dyn LayoutItem>],
    )?;
    Ok(Box::new(column))
}

/// Flips one group open or shut. The panel's own state, keyed by application name — the same key the rows are
/// grouped by, so a group that disappears takes its entry with it the next time the panel is built.
fn toggle_group(expanded: &RwSignal<BTreeSet<String>>, app: &str) {
    let app = app.to_string();
    expanded.update(|open| {
        if !open.remove(&app) {
            open.insert(app);
        }
    });
}

/// A group's header: which application, how many it has waiting, and the two things worth doing to all of them
/// at once — muting the sender, and clearing the group. Tapping the header itself opens or shuts the group.
fn group_header(
    app: String,
    count: usize,
    muted: bool,
    toggle: RwSignal<BTreeSet<String>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let name = if app.trim().is_empty() {
        telar::t!("notifications.unknown_app")
    } else {
        app.clone()
    };
    let label = Text::auto(
        move || name.clone(),
        LayoutStyle::new().flex_grow(1.0),
        move || {
            theme
                .text_style(FontRole::Caption, theme.muted)
                .with_weight(700)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;
    // A count of one is what a header without a badge already says.
    let badge = Text::auto(
        move || {
            if count > 1 {
                count.to_string()
            } else {
                String::new()
            }
        },
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.muted),
    )?;
    let mute_app = app.clone();
    let mute = icon_button(
        if muted { "bell-off" } else { "bell" },
        if muted { theme.red } else { theme.muted },
        theme,
        move || notifications::set_app_muted(&mute_app, !muted),
    )?;
    let clear_app = app.clone();
    let clear = icon_button("trash-2", theme.muted, theme, move || {
        notifications::clear_app(&clear_app)
    })?;

    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0)
            .padding_horizontal(4.0)
            .width(SizeDimension::Percent(1.0)),
        |_| RectStyle::default(),
        vec![
            Box::new(label) as Box<dyn LayoutItem>,
            Box::new(badge),
            mute,
            clear,
        ],
    )?
    .on_press(move || toggle_group(&toggle, &app));
    Ok(Box::new(row))
}

/// The row under a group that has more than it is showing: `+N` while collapsed, and the way back while open.
fn expander_row(
    app: String,
    hidden: usize,
    expanded: bool,
    toggle: RwSignal<BTreeSet<String>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = move || {
        if expanded {
            telar::t!("notifications.show_less")
        } else {
            telar::t!("notifications.show_more", count = hidden.to_string())
        }
    };
    let text = Text::auto(label, LayoutStyle::new(), move || {
        theme.text_style(FontRole::Caption, theme.accent)
    })?;
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .justify_content(JustifyContent::CENTER)
            .padding_vertical(2.0)
            .width(SizeDimension::Percent(1.0)),
        |_| RectStyle::default(),
        vec![Box::new(text) as Box<dyn LayoutItem>],
    )?
    .on_press(move || toggle_group(&toggle, &app));
    Ok(Box::new(row))
}

/// A glyph that does one thing, sized to the caption text it sits beside. Its own box so the tap target is
/// bigger than the glyph, and so a press on it hit-tests before the header row it sits inside.
fn icon_button(
    glyph: &'static str,
    tint: Color,
    theme: NordTheme,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = ui::icon::icon_view(move || glyph.to_string(), move || tint, 14.0)?;
    let button = StyledContainer::new(
        LayoutStyle::new().padding_all(4.0),
        |_| RectStyle::default(),
        vec![icon],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 6.0))
    .on_press(on_press);
    Ok(Box::new(button))
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
    fn body_runs_parses_inline_markup_into_styled_runs() {
        let text = Color::rgb(1.0, 1.0, 1.0);
        let link = Color::rgb(0.0, 0.0, 1.0);

        let runs = body_runs("<b>Bold</b> &amp; <i>italic</i>", text, link);
        assert_eq!(runs.len(), 3);
        assert_eq!(
            (&*runs[0].text, runs[0].weight, runs[0].italic),
            ("Bold", 700, false)
        );
        assert_eq!((&*runs[1].text, runs[1].weight), (" & ", 400));
        assert_eq!((&*runs[2].text, runs[2].italic), ("italic", true));

        // A link carries the link colour; `<br>` stays within the run as a newline.
        let linked = body_runs(r#"a <a href="http://x">click</a> b"#, text, link);
        let click = linked.iter().find(|r| &*r.text == "click").unwrap();
        assert_eq!(click.color.to_rgba8(), link.to_rgba8());

        let br = body_runs("line1<br>line2", text, link);
        assert_eq!((br.len(), &*br[0].text), (1, "line1\nline2"));

        // `<img>` alt text, decoded entities, and unknown tags (kept inner, tag dropped).
        assert_eq!(
            &*body_runs(r#"<img src="a.png" alt="pic"/>"#, text, link)[0].text,
            "pic"
        );
        assert_eq!(
            &*body_runs("&lt;tag&gt; &#65;&#x42;", text, link)[0].text,
            "<tag> AB"
        );
        assert_eq!(&*body_runs("Q&A", text, link)[0].text, "Q&A");
    }

    /// One notification, with the fields a presentation test actually varies.
    fn note(id: u32, app: &str, urgency: Urgency) -> Notification {
        Notification {
            id,
            app_name: app.into(),
            app_icon: String::new(),
            summary: format!("n{id}"),
            body: String::new(),
            actions: Vec::new(),
            urgency,
            popup: true,
            image: None,
        }
    }

    fn snapshot_of(active: Vec<Notification>) -> Snapshot {
        Snapshot {
            unread: active.len() as u32,
            active,
            dnd: false,
            muted_apps: Vec::new(),
        }
    }

    #[test]
    fn dnd_hides_everything_and_max_visible_caps_the_rest() {
        let cfg = NotificationsConfig {
            max_visible: 2,
            ..NotificationsConfig::default()
        };

        let snap = snapshot_of(vec![
            note(1, "a", Urgency::Normal),
            note(2, "a", Urgency::Critical),
            note(3, "a", Urgency::Normal),
        ]);
        let shown = visible(&snap, &cfg, false);
        assert_eq!(shown.len(), 2, "capped at max_visible");
        assert_eq!(shown[0].id, 2, "critical floats to the top");

        let dnd = Snapshot { dnd: true, ..snap };
        assert!(
            visible(&dnd, &cfg, false).is_empty(),
            "DND suppresses all popups"
        );
    }

    #[test]
    fn restored_history_stays_in_the_panel_and_never_pops_up() {
        // One restored (non-popping) and one fresh notification: only the fresh one becomes a popup, while the
        // history panel (which reads all of `active`) still holds both.
        let restored = Notification {
            popup: false,
            ..note(1, "a", Urgency::Normal)
        };
        let snap = snapshot_of(vec![restored, note(2, "a", Urgency::Normal)]);
        let shown = visible(&snap, &NotificationsConfig::default(), false);
        assert_eq!(shown.len(), 1, "only the fresh notification pops up");
        assert_eq!(shown[0].id, 2);
    }

    #[test]
    fn the_fullscreen_policy_decides_what_still_reaches_the_screen() {
        let snap = snapshot_of(vec![
            note(1, "a", Urgency::Normal),
            note(2, "a", Urgency::Critical),
        ]);
        let with = |policy| NotificationsConfig {
            fullscreen: policy,
            ..NotificationsConfig::default()
        };

        for policy in [
            FullscreenPopups::On,
            FullscreenPopups::Off,
            FullscreenPopups::Never,
        ] {
            assert_eq!(visible(&snap, &with(policy), false).len(), 2, "{policy:?}");
        }

        assert_eq!(
            visible(&snap, &with(FullscreenPopups::On), true).len(),
            2,
            "'on' never suppresses"
        );
        let urgent = visible(&snap, &with(FullscreenPopups::Off), true);
        assert_eq!(urgent.len(), 1, "'off' keeps only what is critical");
        assert_eq!(urgent[0].urgency, Urgency::Critical);
        assert!(
            visible(&snap, &with(FullscreenPopups::Never), true).is_empty(),
            "'never' holds back the critical ones too"
        );
        // Suppression is about the popup, never the record: the history reads `active`, which is untouched.
        assert_eq!(snap.active.len(), 2);
    }

    #[test]
    fn the_history_groups_by_app_newest_group_first_and_previews_each() {
        let cfg = NotificationsConfig {
            group_preview_num: 2,
            ..NotificationsConfig::default()
        };
        // Oldest first, as the daemon holds them: Slack spoke first, Calendar last.
        let snap = snapshot_of(vec![
            note(1, "Slack", Urgency::Normal),
            note(2, "Slack", Urgency::Normal),
            note(3, "Slack", Urgency::Normal),
            note(4, "Calendar", Urgency::Normal),
        ]);

        let rows = history_rows(&snap, &cfg, &BTreeSet::new());
        let shape: Vec<String> = rows.iter().map(describe_row).collect();
        assert_eq!(
            shape,
            [
                "group:Calendar:1",
                "card:4",
                "group:Slack:3",
                "card:3",
                "card:2",
                "expander:1",
            ],
            "the app that spoke last leads, and each group previews newest-first"
        );

        let open = BTreeSet::from(["Slack".to_string()]);
        let expanded: Vec<String> = history_rows(&snap, &cfg, &open)
            .iter()
            .map(describe_row)
            .collect();
        assert_eq!(
            expanded,
            [
                "group:Calendar:1",
                "card:4",
                "group:Slack:3",
                "card:3",
                "card:2",
                "card:1",
                "expander:0",
            ],
            "expanding one group reveals the rest of it and leaves the other collapsed"
        );

        let single = history_rows(
            &snapshot_of(vec![note(1, "Slack", Urgency::Normal)]),
            &cfg,
            &BTreeSet::new(),
        );
        assert_eq!(
            single.iter().map(describe_row).collect::<Vec<_>>(),
            ["group:Slack:1", "card:1"],
            "a group with nothing hidden gets no expander row to press"
        );
    }

    #[test]
    fn grouping_off_is_the_flat_newest_first_list_it_always_was() {
        let cfg = NotificationsConfig {
            group_by_app: false,
            ..NotificationsConfig::default()
        };
        let snap = snapshot_of(vec![
            note(1, "Slack", Urgency::Normal),
            note(2, "Calendar", Urgency::Normal),
        ]);
        let rows: Vec<String> = history_rows(&snap, &cfg, &BTreeSet::new())
            .iter()
            .map(describe_row)
            .collect();
        assert_eq!(rows, ["card:2", "card:1"]);
    }

    #[test]
    fn a_group_header_reports_the_mute_its_snapshot_carries() {
        let snap = Snapshot {
            muted_apps: vec!["Slack".to_string()],
            ..snapshot_of(vec![
                note(1, "Slack", Urgency::Normal),
                note(2, "Calendar", Urgency::Normal),
            ])
        };
        let muted: Vec<bool> =
            history_rows(&snap, &NotificationsConfig::default(), &BTreeSet::new())
                .iter()
                .filter_map(|row| match row {
                    HistoryRow::Group { app, muted, .. } => Some((app.clone(), *muted)),
                    _ => None,
                })
                .map(|(_, muted)| muted)
                .collect();
        assert_eq!(
            muted,
            [false, true],
            "Calendar leads, and only Slack is muted"
        );
    }

    #[test]
    fn a_card_is_keyed_on_what_it_draws_not_on_the_id_it_keeps() {
        let before = note(7, "Slack", Urgency::Normal);
        let edited_in_place = Notification {
            summary: "edited".into(),
            ..before.clone()
        };
        assert_ne!(
            card_key(&before),
            card_key(&edited_in_place),
            "a `replaces_id` edit keeps the id, so a key of just the id would leave the old card on screen"
        );
        assert_eq!(
            card_key(&before),
            card_key(&before.clone()),
            "and an unchanged notification keeps its node"
        );
    }

    #[test]
    fn the_default_action_is_the_only_one_a_body_tap_runs() {
        let with = |actions: &[&str]| Notification {
            actions: actions.iter().map(|s| s.to_string()).collect(),
            ..note(1, "a", Urgency::Normal)
        };
        assert_eq!(
            default_action_key(&with(&["default", "Open", "archive", "Archive"])).as_deref(),
            Some("default")
        );
        assert_eq!(
            default_action_key(&with(&["archive", "Archive"])),
            None,
            "an action that is not the default never fires from the card body"
        );
        assert_eq!(default_action_key(&with(&[])), None);
    }

    /// Every row type runs closures nothing else does — a group header reads the theme and the locale, an
    /// expander formats a count through `t!`. Only building the panel runs them, which is the only way the
    /// re-entrant-borrow trap (a second signal read inside another's `with`) ever shows up.
    #[test]
    fn the_history_panel_builds_grouped_and_flat() {
        let snapshot = signal(Arc::new(Snapshot {
            muted_apps: vec!["Slack".to_string()],
            ..sample_snapshot()
        }));
        for group_by_app in [true, false] {
            for open in [BTreeSet::new(), BTreeSet::from(["Slack".to_string()])] {
                telar::reset_layout_runtime();
                telar::set_theme(NordTheme::new());
                let cfg = NotificationsConfig {
                    group_by_app,
                    group_preview_num: 1,
                    ..NotificationsConfig::default()
                };
                // The rows the list will build, laid out here so a failure names the row rather than the list.
                for row in history_rows(&snapshot.peek(), &cfg, &open) {
                    let built = match row {
                        HistoryRow::Group {
                            app, count, muted, ..
                        } => group_header(
                            app.clone(),
                            count,
                            muted,
                            signal(open.clone()),
                            NordTheme::new(),
                        )
                        .map(|_| format!("group:{app}")),
                        HistoryRow::Card(n) => notification_card(
                            &n,
                            SizeDimension::Percent(1.0),
                            CardStyle::new(&cfg, NordTheme::new(), 12.0),
                            Some(notifications::close),
                        )
                        .map(|_| format!("card:{}", n.id)),
                        HistoryRow::Expander {
                            app,
                            hidden,
                            expanded,
                        } => expander_row(
                            app.clone(),
                            hidden,
                            expanded,
                            signal(open.clone()),
                            NordTheme::new(),
                        )
                        .map(|_| format!("expander:{app}")),
                    };
                    assert!(
                        built.is_ok(),
                        "a row failed to build with group_by_app={group_by_app}"
                    );
                }
                assert!(history_list(snapshot.read_only(), &cfg, NordTheme::new(), 12.0).is_ok());
            }
        }
    }

    /// A row reduced to what a shape assertion cares about, so a failure names the row rather than dumping it.
    fn describe_row(row: &HistoryRow) -> String {
        match row {
            HistoryRow::Group { app, count, .. } => format!("group:{app}:{count}"),
            HistoryRow::Card(n) => format!("card:{}", n.id),
            HistoryRow::Expander { hidden, .. } => format!("expander:{hidden}"),
        }
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
            let content = card_stack(signal.read_only(), None, self.cfg.clone(), self.theme, 12.0)
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
            let list = history_list(read, &NotificationsConfig::default(), self.theme, 12.0)
                .expect("list");
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
        snapshot_of(vec![
            Notification {
                actions: vec![
                    "reply".into(),
                    "Reply".into(),
                    "archive".into(),
                    "Archive".into(),
                ],
                ..mk(
                    1,
                    "Slack",
                    "Ada Lovelace",
                    "Still on for the review at <b>3pm</b>? &amp; bring notes",
                    Urgency::Normal,
                )
            },
            mk(
                2,
                "Slack",
                "Grace Hopper",
                "Pushed the fix.",
                Urgency::Normal,
            ),
            mk(
                3,
                "Battery",
                "Battery low",
                "12% remaining — plug in soon.",
                Urgency::Critical,
            ),
            mk(4, "Calendar", "Standup in 5 minutes", "", Urgency::Low),
        ])
    }

    /// Renders the history panel. `TELAR_VISUAL_PANEL_OUT=/tmp/p.png cargo test -p hyprshell --lib visual_panel -- --nocapture`.
    #[test]
    fn visual_panel_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_PANEL_OUT") else {
            eprintln!("set TELAR_VISUAL_PANEL_OUT to render the panel; skipping");
            return;
        };
        visual::render_png(
            PanelPreviewApp {
                snapshot: sample_snapshot(),
                theme: NordTheme::new().with_accent("teal"),
            },
            340,
            360,
            &out,
        );
    }

    /// Renders the popup stack for eyeballing. `TELAR_VISUAL_NOTIF_OUT=/tmp/n.png cargo test -p hyprshell --lib visual_notifications -- --nocapture`.
    #[test]
    fn visual_notifications_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_NOTIF_OUT") else {
            eprintln!("set TELAR_VISUAL_NOTIF_OUT to render notifications; skipping");
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
        let snapshot = snapshot_of(vec![
            mk(
                1,
                "Slack",
                "Ada Lovelace",
                "Are we still on for the review at 3?",
                Urgency::Normal,
            ),
            mk(
                2,
                "Battery",
                "Battery low",
                "12% remaining — plug in soon.",
                Urgency::Critical,
            ),
            mk(3, "Calendar", "Standup in 5 minutes", "", Urgency::Low),
        ]);
        visual::render_png(
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
