use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rsx::{
    AlignItems, Container, Input, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    RwSignal, SizeDimension, StyledContainer, Text, TextStyle, box_item, signal, use_theme,
};
use serde::Serialize;

use crate::core::config::{
    ActiveWindowConfig, Align, AppsConfig, AudioConfig, BackgroundConfig, BarConfig, BarsConfig,
    BatteryConfig, BluetoothConfig, BrightnessConfig, Capitalize, ClockConfig, Config,
    CornersConfig, DashboardConfig, DrawerConfig, Edge, FloatConfig, GeneralConfig, GpuConfig,
    IconsConfig, LauncherConfig, LockStatusConfig, MediaConfig, MediaScroll, ModuleEntry,
    NetworkConfig, NotificationsConfig, OsdConfig, PanelsConfig, PathsConfig, PopoutsConfig, Shape,
    ShapeConfig, StatusIconsConfig, TemperatureConfig, TemperatureUnit, ThemeConfig, TrayConfig,
    WeatherConfig, WorkspacesConfig,
};
use crate::shared::icon::icon_view;
use crate::shared::module::{icon_px, module_fg};
use crate::shared::theme::{BUILT_IN_THEMES, FontRole, NordTheme};

const EDGES: &[&str] = &["top", "bottom", "left", "right"];
const ALIGNS: &[&str] = &["start", "center", "end"];
const SHAPES: &[&str] = &["bar", "sections", "chips"];
const LANGUAGES: &[&str] = &["en", "es"];
const MEDIA_SCROLLS: &[&str] = &["volume", "track", "seek", "none"];
const CAPITALIZATIONS: &[&str] = &["none", "upper", "lower", "title"];
const TEMPERATURE_UNITS: &[&str] = &["celsius", "fahrenheit"];
const WEEKDAYS: &[&str] = &["monday", "sunday", "saturday"];

/// What the theme picker cycles: every built-in palette plus `custom`, which starts from nord for
/// `[theme.colors]` to override. Derived from [`BUILT_IN_THEMES`] so a new palette shows up here on its own.
fn theme_options() -> &'static [&'static str] {
    static OPTIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = BUILT_IN_THEMES.to_vec();
        options.push("custom");
        options
    })
}

/// The bar chip: a gear that opens the settings panel.
pub fn settings_chip() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let fg = module_fg();
    icon_view(|| "settings".to_string(), move || fg.get(), icon_px())
}

/// The settings panel: an in-shell editor for `config.toml`. Each section's fields are seeded from the current
/// file; its Save button writes just that section back with [`Config::save_section`] (format-preserving), which
/// the running shell hot-reloads and applies live. Map-valued config (`theme.colors`, `background.monitors`,
/// per-module overrides) stays hand-edited in the TOML for now.
pub fn settings_panel() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let path = Config::default_path();
    let config = Config::load_or_default(&path);
    crate::shared::services::locale::attach(config.language());

    let title = Text::auto(
        || rsx::t!("settings.title"),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Title), theme.text).with_weight(700),
    )?;

    let sections = vec![
        Box::new(title) as Box<dyn LayoutItem>,
        general_section(&config, &path, theme)?,
        theme_section(&config, &path, theme)?,
        shape_section(&config, &path, theme)?,
        bars_section(&config, &path, theme)?,
        panels_section(&config, &path, theme)?,
        popouts_section(&config, &path, theme)?,
        clock_section(&config, &path, theme)?,
        active_window_section(&config, &path, theme)?,
        media_section(&config, &path, theme)?,
        workspaces_section(&config, &path, theme)?,
        audio_section(&config, &path, theme)?,
        brightness_section(&config, &path, theme)?,
        temperature_section(&config, &path, theme)?,
        battery_section(&config, &path, theme)?,
        lock_status_section(&config, &path, theme)?,
        status_icons_section(&config, &path, theme)?,
        network_section(&config, &path, theme)?,
        bluetooth_section(&config, &path, theme)?,
        gpu_section(&config, &path, theme)?,
        weather_section(&config, &path, theme)?,
        dashboard_section(&config, &path, theme)?,
        paths_section(&config, &path, theme)?,
        tray_section(&config, &path, theme)?,
        launcher_section(&config, &path, theme)?,
        osd_section(&config, &path, theme)?,
        icons_section(&config, &path, theme)?,
        notifications_section(&config, &path, theme)?,
        background_section(&config, &path, theme)?,
        corners_section(&config, &path, theme)?,
    ];

    let panel = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(20.0)
            .width(SizeDimension::Percent(1.0)),
        sections,
    )?;
    Ok(Box::new(panel))
}

fn general_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let lang = signal(rsx::current_locale().unwrap_or_else(|| config.language()));
    let over_fullscreen = signal(config.general.show_over_fullscreen);
    let logo = signal(config.general.logo.clone());
    let apps = config.general.apps.clone();
    // `[general.apps] terminal` is the field's home now; a config still carrying the older top-level key is
    // seeded from it, so editing here moves the value rather than appearing to lose it.
    let terminal = signal(if apps.terminal.trim().is_empty() {
        config.general.terminal.clone()
    } else {
        apps.terminal.clone()
    });
    let file_manager = signal(apps.file_manager.clone());
    let audio_mixer = signal(apps.audio_mixer.clone());
    let media_player = signal(apps.media_player.clone());
    let browser = signal(apps.browser.clone());
    let editor = signal(apps.editor.clone());

    let rows = vec![
        language_field(|| rsx::t!("settings.field.language"), lang.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.show_over_fullscreen"),
            over_fullscreen.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.logo"),
            logo.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.terminal"),
            terminal.clone(),
            "xterm",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.file_manager"),
            file_manager.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.audio_mixer"),
            audio_mixer.clone(),
            "pavucontrol",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.media_player"),
            media_player.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.browser"),
            browser.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.editor"),
            editor.clone(),
            "xdg-open",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let legacy_terminal = config.general.terminal.clone();
    let save = save_button(|| rsx::t!("settings.save.general"), theme, move || {
        persist(&path, "general", &GeneralConfig {
            language: lang.peek(),
            show_over_fullscreen: over_fullscreen.peek(),
            logo: logo.peek(),
            terminal: legacy_terminal.clone(),
            apps: AppsConfig {
                terminal: terminal.peek(),
                file_manager: file_manager.peek(),
                audio_mixer: audio_mixer.peek(),
                media_player: media_player.peek(),
                browser: browser.peek(),
                editor: editor.peek(),
            },
        });
    })?;
    section(|| rsx::t!("settings.section.general"), rows, save, theme)
}

/// A cycle control over UI languages: shows the current one's native name; each press advances to the next code
/// and broadcasts the new locale to every surface via [`crate::shared::services::locale::set`].
fn language_field(
    label: impl Fn() -> String + 'static,
    value: RwSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value_text = value.read_only();
    let text = Text::auto(
        move || language_name(&value_text.get()),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Body), theme.text),
    )?;
    let control = StyledContainer::new(
        LayoutStyle::new()
            .flex_grow(1.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(text)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || {
        let current = value.peek();
        let index = LANGUAGES.iter().position(|o| *o == current).unwrap_or(0);
        let next = LANGUAGES[(index + 1) % LANGUAGES.len()].to_string();
        value.set(next.clone());
        crate::shared::services::locale::set(next);
    });
    labelled(label, Box::new(control), theme)
}

fn language_name(code: &str) -> String {
    match code {
        "en" => "English".to_string(),
        "es" => "Español".to_string(),
        other => other.to_uppercase(),
    }
}

fn theme_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let t = &config.theme;
    let name = signal(t.name.clone());
    let accent = signal(t.accent.clone());
    let font_family = signal(t.font_family.clone().unwrap_or_default());
    let radius = signal(opt_num(t.radius));
    let spacing = signal(opt_num(t.spacing));
    let font_size = signal(opt_num(t.font_size));
    let icon_size = signal(opt_num(t.icon_size));
    let icon_stroke = signal(opt_num(t.icon_stroke));

    let rows = vec![
        enum_field(
            || rsx::t!("settings.field.name"),
            name.clone(),
            theme_options(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.accent"),
            accent.clone(),
            "cyan",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.font_family"),
            font_family.clone(),
            "(default)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.radius"),
            radius.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.spacing"),
            spacing.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.font_size"),
            font_size.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.icon_size"),
            icon_size.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.icon_stroke"),
            icon_stroke.clone(),
            "(glyph)",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.theme"), theme, move || {
        let value = ThemeConfig {
            name: name.peek(),
            accent: accent.peek(),
            font_family: opt_string(&font_family.peek()),
            radius: opt_u32(&radius.peek()),
            spacing: opt_u32(&spacing.peek()),
            font_size: opt_f32(&font_size.peek()),
            icon_size: opt_f32(&icon_size.peek()),
            icon_stroke: opt_f32(&icon_stroke.peek()),
            colors: base.colors.clone(),
        };
        persist(&path, "theme", &value);
    })?;
    section(|| rsx::t!("settings.section.theme"), rows, save, theme)
}

fn shape_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let s = &config.shape;
    let mode = signal(shape_str(s.mode).to_string());
    let frame = signal(s.frame);
    let gap = signal(s.gap.to_string());
    let spacing = signal(opt_num(s.spacing));
    let radius = signal(opt_num(s.radius));
    let inactive = signal(s.inactive_size.to_string());

    let rows = vec![
        enum_field(|| rsx::t!("settings.field.mode"), mode.clone(), SHAPES, theme)?,
        toggle_field(|| rsx::t!("settings.field.frame_ring"), frame.clone(), theme)?,
        text_field(|| rsx::t!("settings.field.gap"), gap.clone(), "0", theme)?,
        text_field(
            || rsx::t!("settings.field.spacing"),
            spacing.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.radius"),
            radius.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.inactive_size"),
            inactive.clone(),
            "6",
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.shape"), theme, move || {
        let value = ShapeConfig {
            mode: parse_shape(&mode.peek()),
            frame: frame.peek(),
            gap: parse_u32(&gap.peek(), base.gap),
            spacing: opt_u32(&spacing.peek()),
            radius: opt_u32(&radius.peek()),
            inactive_size: parse_u32(&inactive.peek(), base.inactive_size),
        };
        persist(&path, "shape", &value);
    })?;
    section(|| rsx::t!("settings.section.shape"), rows, save, theme)
}

#[derive(Clone)]
struct BarSignals {
    size: RwSignal<String>,
    start: RwSignal<String>,
    center: RwSignal<String>,
    end: RwSignal<String>,
}

fn bar_signals(bar: &BarConfig) -> BarSignals {
    BarSignals {
        size: signal(bar.size.to_string()),
        start: signal(join_ids(&bar.start)),
        center: signal(join_ids(&bar.center)),
        end: signal(join_ids(&bar.end)),
    }
}

fn bar_rows(
    label: impl Fn() -> String + 'static,
    s: &BarSignals,
    theme: NordTheme,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    Ok(vec![
        subheader(label, theme)?,
        text_field(|| rsx::t!("settings.field.size"), s.size.clone(), "34", theme)?,
        text_field(
            || rsx::t!("settings.field.start"),
            s.start.clone(),
            "module ids",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.center"),
            s.center.clone(),
            "module ids",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.end"),
            s.end.clone(),
            "module ids",
            theme,
        )?,
    ])
}

fn bar_from(s: &BarSignals, base: &BarConfig) -> BarConfig {
    BarConfig {
        size: parse_u32(&s.size.peek(), base.size),
        start: entries_from_csv(&s.start.peek(), &base.start),
        center: entries_from_csv(&s.center.peek(), &base.center),
        end: entries_from_csv(&s.end.peek(), &base.end),
        shape: base.shape,
    }
}

fn join_ids(entries: &[ModuleEntry]) -> String {
    entries
        .iter()
        .map(|entry| entry.id.as_str())
        .collect::<Vec<_>>()
        .join(", ")
}

/// Reads a zone back from its comma-separated ids, carrying each entry's table settings across.
///
/// The field edits ids only, so an entry written as `{ id = "clock", accent = "red" }` would otherwise lose its
/// accent the first time anything else on the bar was saved. Each id claims the first not-yet-claimed entry
/// with that id, which keeps both sets of settings when a module appears twice and survives a reorder.
fn entries_from_csv(text: &str, base: &[ModuleEntry]) -> Vec<ModuleEntry> {
    let mut claimed = vec![false; base.len()];
    split_csv(text)
        .into_iter()
        .map(|id| {
            let found = base
                .iter()
                .enumerate()
                .find(|(index, entry)| !claimed[*index] && entry.id == id);
            match found {
                Some((index, entry)) => {
                    claimed[index] = true;
                    entry.clone()
                }
                None => ModuleEntry::bare(id),
            }
        })
        .collect()
}

fn bars_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let bars = &config.bars;
    let top = bar_signals(&bars.top);
    let bottom = bar_signals(&bars.bottom);
    let left = bar_signals(&bars.left);
    let right = bar_signals(&bars.right);

    let mut rows = Vec::new();
    rows.extend(bar_rows(|| rsx::t!("settings.subheader.top"), &top, theme)?);
    rows.extend(bar_rows(
        || rsx::t!("settings.subheader.bottom"),
        &bottom,
        theme,
    )?);
    rows.extend(bar_rows(|| rsx::t!("settings.subheader.left"), &left, theme)?);
    rows.extend(bar_rows(
        || rsx::t!("settings.subheader.right"),
        &right,
        theme,
    )?);

    let base = bars.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.bars"), theme, move || {
        let value = BarsConfig {
            top: bar_from(&top, &base.top),
            bottom: bar_from(&bottom, &base.bottom),
            left: bar_from(&left, &base.left),
            right: bar_from(&right, &base.right),
        };
        persist(&path, "bars", &value);
    })?;
    section(|| rsx::t!("settings.section.bars"), rows, save, theme)
}

fn panels_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let p = &config.panels;
    let gap = signal(opt_num(p.gap));
    let drawer_w = signal(p.drawer.width.to_string());
    let drawer_h = signal(p.drawer.max_height.to_string());
    let float_w = signal(p.float.width.to_string());
    let float_h = signal(p.float.height.to_string());

    let rows = vec![
        text_field(|| rsx::t!("settings.field.gap"), gap.clone(), "(auto)", theme)?,
        text_field(
            || rsx::t!("settings.field.drawer_width"),
            drawer_w.clone(),
            "320",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.drawer_max_height"),
            drawer_h.clone(),
            "280",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.float_width"),
            float_w.clone(),
            "360",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.float_height"),
            float_h.clone(),
            "240",
            theme,
        )?,
    ];

    let base = *p;
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.panels"), theme, move || {
        let value = PanelsConfig {
            gap: opt_u32(&gap.peek()),
            drawer: DrawerConfig {
                width: parse_f32(&drawer_w.peek(), base.drawer.width),
                max_height: parse_f32(&drawer_h.peek(), base.drawer.max_height),
            },
            float: FloatConfig {
                width: parse_u32(&float_w.peek(), base.float.width),
                height: parse_u32(&float_h.peek(), base.float.height),
            },
        };
        persist(&path, "panels", &value);
    })?;
    section(|| rsx::t!("settings.section.panels"), rows, save, theme)
}

fn popouts_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let p = config.popouts;
    let enabled = signal(p.enabled);
    let open_delay = signal(p.open_delay.to_string());
    let close_delay = signal(p.close_delay.to_string());
    let width = signal(p.width.to_string());
    let max_height = signal(p.max_height.to_string());

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.open_delay"),
            open_delay.clone(),
            "280",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.close_delay"),
            close_delay.clone(),
            "200",
            theme,
        )?,
        text_field(|| rsx::t!("settings.field.width"), width.clone(), "264", theme)?,
        text_field(
            || rsx::t!("settings.field.max_height"),
            max_height.clone(),
            "300",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.popouts"), theme, move || {
        let value = PopoutsConfig {
            enabled: enabled.peek(),
            open_delay: parse_u64(&open_delay.peek(), p.open_delay),
            close_delay: parse_u64(&close_delay.peek(), p.close_delay),
            width: parse_f32(&width.peek(), p.width),
            max_height: parse_f32(&max_height.peek(), p.max_height),
        };
        persist(&path, "popouts", &value);
    })?;
    section(|| rsx::t!("settings.section.popouts"), rows, save, theme)
}

fn osd_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let o = &config.osd;
    let edge = signal(edge_str(o.edge).to_string());
    let align = signal(align_str(o.align).to_string());
    let timeout = signal(o.timeout_ms.to_string());

    let rows = vec![
        enum_field(|| rsx::t!("settings.field.edge"), edge.clone(), EDGES, theme)?,
        enum_field(
            || rsx::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "1200",
            theme,
        )?,
    ];

    let base = *o;
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.osd"), theme, move || {
        let value = OsdConfig {
            edge: parse_edge(&edge.peek()),
            align: parse_align(&align.peek()),
            timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
        };
        persist(&path, "osd", &value);
    })?;
    section(|| rsx::t!("settings.section.osd"), rows, save, theme)
}

fn icons_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let i = &config.icons;
    let provider = signal(i.provider.clone());
    let default_set = signal(i.default_set.clone());
    let app_icon_theme = signal(i.app_icon_theme.clone());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.provider"),
            provider.clone(),
            "https://api.iconify.design",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.default_set"),
            default_set.clone(),
            "lucide",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.app_icon_theme"),
            app_icon_theme.clone(),
            "auto",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.icons"), theme, move || {
        let value = IconsConfig {
            provider: provider.peek(),
            default_set: default_set.peek(),
            app_icon_theme: app_icon_theme.peek(),
        };
        persist(&path, "icons", &value);
    })?;
    section(|| rsx::t!("settings.section.icons"), rows, save, theme)
}

fn clock_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let c = &config.clock;
    let twelve_hour = signal(c.twelve_hour);
    // An empty field means "no override", which is what `Option<String>` carries; the placeholder shows what
    // the 12/24-hour switch would produce, so it is clear what leaving it blank does.
    let format = signal(c.format.clone().unwrap_or_default());
    let show_date = signal(c.show_date);
    let date_format = signal(c.date_format.clone());

    let rows = vec![
        toggle_field(
            || rsx::t!("settings.field.twelve_hour"),
            twelve_hour.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.time_format"),
            format.clone(),
            "%H:%M:%S",
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.show_date"),
            show_date.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.date_format"),
            date_format.clone(),
            "%a %d %b",
            theme,
        )?,
    ];

    let base = c.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.clock"), theme, move || {
        let typed = format.peek();
        let value = ClockConfig {
            twelve_hour: twelve_hour.peek(),
            format: (!typed.trim().is_empty()).then_some(typed),
            show_date: show_date.peek(),
            date_format: {
                let typed = date_format.peek();
                if typed.trim().is_empty() {
                    base.date_format.clone()
                } else {
                    typed
                }
            },
        };
        persist(&path, "clock", &value);
    })?;
    section(|| rsx::t!("settings.section.clock"), rows, save, theme)
}

fn workspaces_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let w = &config.workspaces;
    let shown = signal(w.shown.to_string());
    let per_monitor = signal(w.per_monitor);
    let show_special = signal(w.show_special);
    let window_icons = signal(w.window_icons);
    let max_icons = signal(w.max_window_icons.to_string());
    let occupied = signal(w.occupied_background);
    let indicator = signal(w.indicator);
    let scroll = signal(w.scroll);
    let label = signal(w.label.clone());
    let occupied_label = signal(w.occupied_label.clone());
    let active_label = signal(w.active_label.clone());
    let capitalize = signal(capitalize_str(w.capitalize).to_string());

    let rows = vec![
        text_field(|| rsx::t!("settings.field.shown"), shown.clone(), "0", theme)?,
        toggle_field(
            || rsx::t!("settings.field.per_monitor"),
            per_monitor.clone(),
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.show_special"),
            show_special.clone(),
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.window_icons"),
            window_icons.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_window_icons"),
            max_icons.clone(),
            "4",
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.occupied_background"),
            occupied.clone(),
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.indicator"),
            indicator.clone(),
            theme,
        )?,
        toggle_field(|| rsx::t!("settings.field.scroll"), scroll.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.label"),
            label.clone(),
            "{id}",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.occupied_label"),
            occupied_label.clone(),
            "(label)",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.active_label"),
            active_label.clone(),
            "(label)",
            theme,
        )?,
        enum_field(
            || rsx::t!("settings.field.capitalize"),
            capitalize.clone(),
            CAPITALIZATIONS,
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.workspaces"), theme, move || {
        let typed = label.peek();
        let value = WorkspacesConfig {
            shown: parse_u32(&shown.peek(), base.shown),
            per_monitor: per_monitor.peek(),
            show_special: show_special.peek(),
            window_icons: window_icons.peek(),
            max_window_icons: parse_u32(&max_icons.peek(), base.max_window_icons),
            occupied_background: occupied.peek(),
            indicator: indicator.peek(),
            scroll: scroll.peek(),
            label: if typed.trim().is_empty() {
                base.label.clone()
            } else {
                typed
            },
            // Empty is meaningful here: it means "render like every other pill".
            occupied_label: occupied_label.peek().trim().to_string(),
            active_label: active_label.peek().trim().to_string(),
            capitalize: parse_capitalize(&capitalize.peek()),
            // Map-valued, so it stays hand-edited in the TOML; carrying it through means saving here does not
            // silently drop the user's scratchpad icons.
            special_icons: base.special_icons.clone(),
        };
        persist(&path, "workspaces", &value);
    })?;
    section(|| rsx::t!("settings.section.workspaces"), rows, save, theme)
}

fn media_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let m = &config.media;
    let preferred = signal(m.preferred_player.clone());
    let max_chars = signal(m.max_chars.to_string());
    let scroll = signal(media_scroll_str(m.scroll).to_string());
    let marquee = signal(m.marquee);
    let marquee_speed = signal(m.marquee_speed_ms.to_string());
    let seek_seconds = signal(m.seek_seconds.to_string());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.preferred_player"),
            preferred.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_chars"),
            max_chars.clone(),
            "40",
            theme,
        )?,
        enum_field(
            || rsx::t!("settings.field.scroll"),
            scroll.clone(),
            MEDIA_SCROLLS,
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.seek_seconds"),
            seek_seconds.clone(),
            "5",
            theme,
        )?,
        toggle_field(|| rsx::t!("settings.field.marquee"), marquee.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.marquee_speed_ms"),
            marquee_speed.clone(),
            "220",
            theme,
        )?,
    ];

    // Aliases are map-valued, so they stay hand-edited in the TOML for now, like `theme.colors`; carrying the
    // existing map through means saving this section does not silently drop them.
    let base = m.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.media"), theme, move || {
        let value = MediaConfig {
            preferred_player: preferred.peek(),
            max_chars: parse_u32(&max_chars.peek(), base.max_chars),
            scroll: parse_media_scroll(&scroll.peek()),
            marquee: marquee.peek(),
            marquee_speed_ms: parse_u32(&marquee_speed.peek(), base.marquee_speed_ms),
            seek_seconds: parse_u32(&seek_seconds.peek(), base.seek_seconds),
            aliases: base.aliases.clone(),
        };
        persist(&path, "media", &value);
    })?;
    section(|| rsx::t!("settings.section.media"), rows, save, theme)
}

fn audio_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let a = config.audio;
    let increment = signal(a.increment.to_string());
    let max_volume = signal(a.max_volume.to_string());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.increment"),
            increment.clone(),
            "5",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_volume"),
            max_volume.clone(),
            "150",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.audio"), theme, move || {
        let value = AudioConfig {
            increment: parse_i32(&increment.peek(), a.increment),
            max_volume: parse_i32(&max_volume.peek(), a.max_volume),
        };
        persist(&path, "audio", &value);
    })?;
    section(|| rsx::t!("settings.section.audio"), rows, save, theme)
}

fn brightness_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let b = config.brightness;
    let increment = signal(b.increment.to_string());

    let rows = vec![text_field(
        || rsx::t!("settings.field.increment"),
        increment.clone(),
        "5",
        theme,
    )?];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.brightness"), theme, move || {
        let value = BrightnessConfig {
            increment: parse_i32(&increment.peek(), b.increment),
        };
        persist(&path, "brightness", &value);
    })?;
    section(|| rsx::t!("settings.section.brightness"), rows, save, theme)
}

fn temperature_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let t = &config.temperature;
    let unit = signal(temperature_unit_str(t.unit).to_string());
    let sensor = signal(t.sensor.clone());
    let warn = signal(t.warn.to_string());
    let critical = signal(t.critical.to_string());

    let rows = vec![
        enum_field(
            || rsx::t!("settings.field.unit"),
            unit.clone(),
            TEMPERATURE_UNITS,
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.sensor"),
            sensor.clone(),
            "(hottest)",
            theme,
        )?,
        text_field(|| rsx::t!("settings.field.warn"), warn.clone(), "70", theme)?,
        text_field(
            || rsx::t!("settings.field.critical"),
            critical.clone(),
            "85",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.temperature"), theme, move || {
        let value = TemperatureConfig {
            unit: parse_temperature_unit(&unit.peek()),
            sensor: sensor.peek().trim().to_string(),
            warn: parse_f32(&warn.peek(), base.warn),
            critical: parse_f32(&critical.peek(), base.critical),
        };
        persist(&path, "temperature", &value);
    })?;
    section(|| rsx::t!("settings.section.temperature"), rows, save, theme)
}

fn launcher_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let l = &config.launcher;
    let width = signal(l.width.to_string());
    let height = signal(l.height.to_string());
    let max_results = signal(l.max_results.to_string());
    let fuzzy = signal(l.fuzzy);
    let calculator = signal(l.calculator);
    let dangerous = signal(l.enable_dangerous_actions);
    let favourites = signal(join_csv(&l.favourites));
    let hidden = signal(join_csv(&l.hidden));

    let rows = vec![
        text_field(|| rsx::t!("settings.field.width"), width.clone(), "640", theme)?,
        text_field(
            || rsx::t!("settings.field.height"),
            height.clone(),
            "420",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_results"),
            max_results.clone(),
            "12",
            theme,
        )?,
        toggle_field(|| rsx::t!("settings.field.fuzzy"), fuzzy.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.calculator"),
            calculator.clone(),
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.enable_dangerous_actions"),
            dangerous.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.favourites"),
            favourites.clone(),
            "desktop ids",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.hidden"),
            hidden.clone(),
            "desktop ids",
            theme,
        )?,
    ];

    let base = l.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.launcher"), theme, move || {
        let value = LauncherConfig {
            width: parse_u32(&width.peek(), base.width),
            height: parse_u32(&height.peek(), base.height),
            radius: base.radius,
            max_results: parse_u32(&max_results.peek(), base.max_results),
            fuzzy: fuzzy.peek(),
            calculator: calculator.peek(),
            favourites: split_csv(&favourites.peek()),
            hidden: split_csv(&hidden.peek()),
            // A list of tables, so it stays hand-edited in the TOML; carrying it through means saving here
            // does not silently drop the user's actions.
            actions: base.actions.clone(),
            enable_dangerous_actions: dangerous.peek(),
        };
        persist(&path, "launcher", &value);
    })?;
    section(|| rsx::t!("settings.section.launcher"), rows, save, theme)
}

fn battery_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let b = &config.battery;
    let enabled = signal(b.enabled);
    let critical_level = signal(b.critical_level.to_string());
    let critical_action = signal(b.critical_action.clone());

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.critical_level"),
            critical_level.clone(),
            "0",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.critical_action"),
            critical_action.clone(),
            "suspend",
            theme,
        )?,
    ];

    // `warn_levels` is a list of tables, so it stays hand-edited in the TOML like `theme.colors`; carrying it
    // through means saving here does not silently drop the user's thresholds.
    let base = b.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.battery"), theme, move || {
        let value = BatteryConfig {
            enabled: enabled.peek(),
            warn_levels: base.warn_levels.clone(),
            critical_level: parse_i32(&critical_level.peek(), base.critical_level),
            critical_action: critical_action.peek().trim().to_string(),
        };
        persist(&path, "battery", &value);
    })?;
    section(|| rsx::t!("settings.section.battery"), rows, save, theme)
}

fn lock_status_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let l = config.lock_status;
    let caps = signal(l.caps);
    let num = signal(l.num);
    let hide_inactive = signal(l.hide_inactive);

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.caps"), caps.clone(), theme)?,
        toggle_field(|| rsx::t!("settings.field.num"), num.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.hide_inactive"),
            hide_inactive.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.lock_status"), theme, move || {
        let value = LockStatusConfig {
            caps: caps.peek(),
            num: num.peek(),
            hide_inactive: hide_inactive.peek(),
        };
        persist(&path, "lock_status", &value);
    })?;
    section(|| rsx::t!("settings.section.lock_status"), rows, save, theme)
}

fn gpu_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let g = &config.gpu;
    let enabled = signal(g.enabled);
    let backend = signal(g.backend.clone());
    let card = signal(g.card.clone());

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.backend"),
            backend.clone(),
            "auto",
            theme,
        )?,
        text_field(|| rsx::t!("settings.field.card"), card.clone(), "card1", theme)?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.gpu"), theme, move || {
        let value = GpuConfig {
            enabled: enabled.peek(),
            backend: backend.peek(),
            card: card.peek(),
        };
        persist(&path, "gpu", &value);
    })?;
    section(|| rsx::t!("settings.section.gpu"), rows, save, theme)
}

fn weather_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let w = &config.weather;
    let enabled = signal(w.enabled);
    let location = signal(w.location.clone());
    let latitude = signal(w.latitude.map(|v| v.to_string()).unwrap_or_default());
    let longitude = signal(w.longitude.map(|v| v.to_string()).unwrap_or_default());
    let refresh = signal(w.refresh_minutes.to_string());
    let days = signal(w.forecast_days.to_string());

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.location"),
            location.clone(),
            "Madrid",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.latitude"),
            latitude.clone(),
            "40.4168",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.longitude"),
            longitude.clone(),
            "-3.7038",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.refresh_minutes"),
            refresh.clone(),
            "15",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.forecast_days"),
            days.clone(),
            "7",
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.weather"), theme, move || {
        // A blank coordinate is "not set", not zero: a stray empty field must fall back to the place name
        // rather than pinning the forecast to the Gulf of Guinea.
        let optional = |raw: String| raw.trim().parse::<f32>().ok();
        let value = WeatherConfig {
            enabled: enabled.peek(),
            location: location.peek(),
            latitude: optional(latitude.peek()),
            longitude: optional(longitude.peek()),
            refresh_minutes: parse_u32(&refresh.peek(), base.refresh_minutes),
            forecast_days: parse_u32(&days.peek(), base.forecast_days),
        };
        persist(&path, "weather", &value);
    })?;
    section(|| rsx::t!("settings.section.weather"), rows, save, theme)
}

fn dashboard_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let d = &config.dashboard;
    let tabs = signal(join_csv(&d.tabs));
    let media = signal(d.media_update_interval.to_string());
    let resources = signal(d.resource_update_interval.to_string());
    let first_day = signal(d.first_day_of_week.clone());
    let avatar = signal(d.avatar.clone());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.tabs"),
            tabs.clone(),
            "dash, media, performance, weather",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.media_update_interval"),
            media.clone(),
            "500",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.resource_update_interval"),
            resources.clone(),
            "1000",
            theme,
        )?,
        enum_field(
            || rsx::t!("settings.field.first_day_of_week"),
            first_day.clone(),
            WEEKDAYS,
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.avatar"),
            avatar.clone(),
            "~/.face",
            theme,
        )?,
    ];

    let base = d.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.dashboard"), theme, move || {
        let value = DashboardConfig {
            tabs: split_csv(&tabs.peek()),
            media_update_interval: parse_u64(&media.peek(), base.media_update_interval),
            resource_update_interval: parse_u64(&resources.peek(), base.resource_update_interval),
            first_day_of_week: first_day.peek(),
            avatar: avatar.peek(),
        };
        persist(&path, "dashboard", &value);
    })?;
    section(|| rsx::t!("settings.section.dashboard"), rows, save, theme)
}

fn paths_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let p = &config.paths;
    let wallpapers = signal(p.wallpapers.clone());
    let lyrics = signal(p.lyrics.clone());
    let recordings = signal(p.recordings.clone());
    let screenshots = signal(p.screenshots.clone());
    let assets = signal(p.assets.clone());

    // Each placeholder is the directory the shell would use if the field is left empty, resolved against this
    // machine — so the form shows where things actually land rather than a generic example.
    let show = |dir: PathBuf| dir.to_string_lossy().into_owned();
    let rows = vec![
        text_field(
            || rsx::t!("settings.field.wallpapers"),
            wallpapers.clone(),
            &show(config.wallpaper_dir()),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.lyrics"),
            lyrics.clone(),
            &show(config.lyrics_dir()),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.recordings"),
            recordings.clone(),
            &show(config.recordings_dir()),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.screenshots"),
            screenshots.clone(),
            &show(config.screenshot_dir()),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.assets"),
            assets.clone(),
            "",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.paths"), theme, move || {
        let value = PathsConfig {
            wallpapers: wallpapers.peek(),
            lyrics: lyrics.peek(),
            recordings: recordings.peek(),
            screenshots: screenshots.peek(),
            assets: assets.peek(),
        };
        persist(&path, "paths", &value);
    })?;
    section(|| rsx::t!("settings.section.paths"), rows, save, theme)
}

fn network_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let n = config.network;
    let enabled = signal(n.enabled);
    let rescan = signal(n.rescan_seconds.to_string());
    let max_networks = signal(n.max_networks.to_string());
    let show_hidden = signal(n.show_hidden);

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.rescan_seconds"),
            rescan.clone(),
            "300",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_networks"),
            max_networks.clone(),
            "20",
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.show_hidden"),
            show_hidden.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.network"), theme, move || {
        let value = NetworkConfig {
            enabled: enabled.peek(),
            rescan_seconds: parse_u32(&rescan.peek(), n.rescan_seconds),
            max_networks: parse_u32(&max_networks.peek(), n.max_networks),
            show_hidden: show_hidden.peek(),
        };
        persist(&path, "network", &value);
    })?;
    section(|| rsx::t!("settings.section.network"), rows, save, theme)
}

fn bluetooth_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let b = config.bluetooth;
    let enabled = signal(b.enabled);
    let scan_on_open = signal(b.scan_on_open);
    let max_devices = signal(b.max_devices.to_string());
    let show_unnamed = signal(b.show_unnamed);

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.scan_on_open"),
            scan_on_open.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_devices"),
            max_devices.clone(),
            "12",
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.show_unnamed"),
            show_unnamed.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.bluetooth"), theme, move || {
        let value = BluetoothConfig {
            enabled: enabled.peek(),
            scan_on_open: scan_on_open.peek(),
            max_devices: parse_u32(&max_devices.peek(), b.max_devices),
            show_unnamed: show_unnamed.peek(),
        };
        persist(&path, "bluetooth", &value);
    })?;
    section(|| rsx::t!("settings.section.bluetooth"), rows, save, theme)
}

fn status_icons_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let s = &config.status_icons;
    let icons = signal(join_csv(&s.icons));
    let spacing = signal(s.spacing.to_string());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.icons"),
            icons.clone(),
            "volume, mic, network, battery",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.spacing"),
            spacing.clone(),
            "0.35",
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.status_icons"), theme, move || {
        let value = StatusIconsConfig {
            icons: split_csv(&icons.peek()),
            spacing: parse_f32(&spacing.peek(), base.spacing),
        };
        persist(&path, "status_icons", &value);
    })?;
    section(|| rsx::t!("settings.section.status_icons"), rows, save, theme)
}

fn tray_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let t = &config.tray;
    let enabled = signal(t.enabled);
    let compact = signal(t.compact);
    let recolour = signal(t.recolour);
    let background = signal(t.background);
    let hidden = signal(t.hidden.join(", "));

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(|| rsx::t!("settings.field.compact"), compact.clone(), theme)?,
        toggle_field(|| rsx::t!("settings.field.recolour"), recolour.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.background"),
            background.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.hidden"),
            hidden.clone(),
            "steam_app_*",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.tray"), theme, move || {
        let value = TrayConfig {
            enabled: enabled.peek(),
            compact: compact.peek(),
            recolour: recolour.peek(),
            background: background.peek(),
            hidden: hidden
                .peek()
                .split(',')
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect(),
            // Map-valued, so it stays hand-edited in the TOML like `theme.colors`; carrying it through means
            // saving here does not silently drop the user's icon substitutions.
            icon_subs: base.icon_subs.clone(),
        };
        persist(&path, "tray", &value);
    })?;
    section(|| rsx::t!("settings.section.tray"), rows, save, theme)
}

fn media_scroll_str(scroll: MediaScroll) -> &'static str {
    match scroll {
        MediaScroll::Volume => "volume",
        MediaScroll::Track => "track",
        MediaScroll::Seek => "seek",
        MediaScroll::None => "none",
    }
}

fn parse_media_scroll(raw: &str) -> MediaScroll {
    match raw {
        "track" => MediaScroll::Track,
        "seek" => MediaScroll::Seek,
        "none" => MediaScroll::None,
        _ => MediaScroll::Volume,
    }
}

fn active_window_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let w = config.active_window;
    let compact = signal(w.compact);
    let show_icon = signal(w.show_icon);
    let max_chars = signal(w.max_chars.to_string());

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.compact"), compact.clone(), theme)?,
        toggle_field(
            || rsx::t!("settings.field.show_icon"),
            show_icon.clone(),
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_chars"),
            max_chars.clone(),
            "300",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.active_window"), theme, move || {
        let value = ActiveWindowConfig {
            compact: compact.peek(),
            show_icon: show_icon.peek(),
            max_chars: parse_u32(&max_chars.peek(), w.max_chars),
        };
        persist(&path, "active_window", &value);
    })?;
    section(|| rsx::t!("settings.section.active_window"), rows, save, theme)
}

fn notifications_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let n = &config.notifications;
    let edge = signal(edge_str(n.edge).to_string());
    let align = signal(align_str(n.align).to_string());
    let max_visible = signal(n.max_visible.to_string());
    let timeout = signal(n.timeout_ms.to_string());
    let critical = signal(n.critical_sticky);
    let width = signal(n.width.to_string());
    let gap = signal(n.gap.to_string());

    let rows = vec![
        enum_field(|| rsx::t!("settings.field.edge"), edge.clone(), EDGES, theme)?,
        enum_field(
            || rsx::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.max_visible"),
            max_visible.clone(),
            "4",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "5000",
            theme,
        )?,
        toggle_field(
            || rsx::t!("settings.field.critical_sticky"),
            critical.clone(),
            theme,
        )?,
        text_field(|| rsx::t!("settings.field.width"), width.clone(), "380", theme)?,
        text_field(|| rsx::t!("settings.field.gap"), gap.clone(), "10", theme)?,
    ];

    let base = n.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.notifications"), theme, move || {
        let value = NotificationsConfig {
            edge: parse_edge(&edge.peek()),
            align: parse_align(&align.peek()),
            max_visible: parse_u32(&max_visible.peek(), base.max_visible),
            timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
            critical_sticky: critical.peek(),
            width: parse_f32(&width.peek(), base.width),
            gap: parse_f32(&gap.peek(), base.gap),
        };
        persist(&path, "notifications", &value);
    })?;
    section(|| rsx::t!("settings.section.notifications"), rows, save, theme)
}

fn background_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let b = &config.background;
    let enabled = signal(b.enabled);
    let image = signal(
        b.image
            .as_ref()
            .map(|p| p.display().to_string())
            .unwrap_or_default(),
    );

    let rows = vec![
        toggle_field(|| rsx::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || rsx::t!("settings.field.image"),
            image.clone(),
            "~/wall.png",
            theme,
        )?,
    ];

    let base = b.clone();
    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.background"), theme, move || {
        let value = BackgroundConfig {
            enabled: enabled.peek(),
            image: opt_string(&image.peek()).map(PathBuf::from),
            monitors: base.monitors.clone(),
        };
        persist(&path, "background", &value);
    })?;
    section(|| rsx::t!("settings.section.background"), rows, save, theme)
}

fn corners_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let c = &config.corners;
    let tl = signal(c.top_left.clone().unwrap_or_default());
    let tr = signal(c.top_right.clone().unwrap_or_default());
    let bl = signal(c.bottom_left.clone().unwrap_or_default());
    let br = signal(c.bottom_right.clone().unwrap_or_default());

    let rows = vec![
        text_field(
            || rsx::t!("settings.field.top_left"),
            tl.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.top_right"),
            tr.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.bottom_left"),
            bl.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || rsx::t!("settings.field.bottom_right"),
            br.clone(),
            "module id",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.corners"), theme, move || {
        let value = CornersConfig {
            top_left: opt_string(&tl.peek()),
            top_right: opt_string(&tr.peek()),
            bottom_left: opt_string(&bl.peek()),
            bottom_right: opt_string(&br.peek()),
        };
        persist(&path, "corners", &value);
    })?;
    section(|| rsx::t!("settings.section.corners"), rows, save, theme)
}

fn persist<T: Serialize>(path: &Path, name: &str, value: &T) {
    if let Err(e) = Config::save_section(path, name, value) {
        tracing::warn!("settings: could not save [{name}]: {e}");
    }
}

fn section(
    title: impl Fn() -> String + 'static,
    mut rows: Vec<Box<dyn LayoutItem>>,
    save: Box<dyn LayoutItem>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut children = vec![section_label(title, theme)?];
    children.append(&mut rows);
    children.push(save);
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        children,
    )?;
    Ok(Box::new(column))
}

fn section_label(
    label: impl Fn() -> String + 'static,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(
        move || label(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Body), theme.text).with_weight(700),
    )?;
    Ok(Box::new(text))
}

fn subheader(
    label: impl Fn() -> String + 'static,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(
        move || label(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Caption), theme.muted).with_weight(700),
    )?;
    Ok(Box::new(text))
}

fn labelled(
    label: impl Fn() -> String + 'static,
    control: Box<dyn LayoutItem>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label_text = Text::auto(
        move || label(),
        LayoutStyle::new().width(120.0),
        move || TextStyle::new(theme.font(FontRole::Body), theme.subtle),
    )?;
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(label_text), control],
    )?;
    Ok(Box::new(row))
}

fn text_field(
    label: impl Fn() -> String + 'static,
    value: RwSignal<String>,
    placeholder: &str,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let input = Input::new(
        value,
        LayoutStyle::new()
            .flex_grow(1.0)
            .height(theme.font(FontRole::Body) * 1.6),
        move || TextStyle::new(theme.font(FontRole::Body), theme.text),
    )?
    .placeholder(placeholder.to_string());
    let boxed = StyledContainer::new(
        LayoutStyle::new()
            .flex_grow(1.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(input)],
    )?;
    labelled(label, Box::new(boxed), theme)
}

fn toggle_field(
    label: impl Fn() -> String + 'static,
    value: RwSignal<bool>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let on_fg = theme.accent.most_readable(&[theme.text, theme.base]);
    let value_text = value.read_only();
    let value_fill = value.read_only();
    let value_color = value.read_only();
    let text = Text::auto(
        move || {
            if value_text.get() {
                rsx::t!("common.on")
            } else {
                rsx::t!("common.off")
            }
        },
        LayoutStyle::new(),
        move || {
            let fg = if value_color.get() { on_fg } else { theme.text };
            TextStyle::new(theme.font(FontRole::Caption), fg).with_weight(700)
        },
    )?;
    let control = StyledContainer::new(
        LayoutStyle::new()
            .width(56.0)
            .padding_vertical(5.0)
            .justify_content(JustifyContent::CENTER),
        move |_| {
            let fill = if value_fill.get() {
                theme.accent
            } else {
                theme.overlay
            };
            RectStyle::filled(fill, 8.0)
        },
        vec![box_item(text)],
    )?
    .on_press(move || value.set(!value.peek()));
    labelled(label, Box::new(control), theme)
}

/// A cycle control: shows the current option; each press advances to the next.
fn enum_field(
    label: impl Fn() -> String + 'static,
    value: RwSignal<String>,
    options: &'static [&'static str],
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value_text = value.read_only();
    let text = Text::auto(
        move || value_text.get(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Body), theme.text),
    )?;
    let control = StyledContainer::new(
        LayoutStyle::new()
            .flex_grow(1.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(text)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || {
        let current = value.peek();
        let index = options.iter().position(|o| *o == current).unwrap_or(0);
        value.set(options[(index + 1) % options.len()].to_string());
    });
    labelled(label, Box::new(control), theme)
}

fn save_button(
    label: impl Fn() -> String + 'static,
    theme: NordTheme,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let fg = theme.accent.most_readable(&[theme.text, theme.base]);
    let text = Text::auto(
        move || label(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Body), fg).with_weight(700),
    )?;
    let button = StyledContainer::new(
        LayoutStyle::new()
            .padding_horizontal(14.0)
            .padding_vertical(8.0)
            .justify_content(JustifyContent::CENTER),
        move |_| RectStyle::filled(theme.accent, 8.0),
        vec![box_item(text)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.accent.darken(0.08), 8.0))
    .on_active_style(move |_| RectStyle::filled(theme.accent.darken(0.16), 8.0))
    .on_press(on_press);
    Ok(Box::new(button))
}

fn opt_num<T: ToString>(value: Option<T>) -> String {
    value.map(|v| v.to_string()).unwrap_or_default()
}

fn opt_string(s: &str) -> Option<String> {
    let s = s.trim();
    (!s.is_empty()).then(|| s.to_string())
}

fn opt_u32(s: &str) -> Option<u32> {
    s.trim().parse().ok()
}

fn opt_f32(s: &str) -> Option<f32> {
    s.trim().parse().ok()
}

fn parse_u32(s: &str, fallback: u32) -> u32 {
    s.trim().parse().unwrap_or(fallback)
}

fn parse_i32(s: &str, fallback: i32) -> i32 {
    s.trim().parse().unwrap_or(fallback)
}

fn parse_u64(s: &str, fallback: u64) -> u64 {
    s.trim().parse().unwrap_or(fallback)
}

fn parse_f32(s: &str, fallback: f32) -> f32 {
    s.trim().parse().unwrap_or(fallback)
}

fn join_csv(items: &[String]) -> String {
    items.join(", ")
}

fn split_csv(s: &str) -> Vec<String> {
    s.split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect()
}

fn edge_str(edge: Edge) -> &'static str {
    edge.as_str()
}

fn parse_edge(s: &str) -> Edge {
    match s {
        "bottom" => Edge::Bottom,
        "left" => Edge::Left,
        "right" => Edge::Right,
        _ => Edge::Top,
    }
}

fn align_str(align: Align) -> &'static str {
    match align {
        Align::Start => "start",
        Align::Center => "center",
        Align::End => "end",
    }
}

fn parse_align(s: &str) -> Align {
    match s {
        "start" => Align::Start,
        "end" => Align::End,
        _ => Align::Center,
    }
}

fn shape_str(shape: Shape) -> &'static str {
    match shape {
        Shape::Bar => "bar",
        Shape::Sections => "sections",
        Shape::Chips => "chips",
    }
}

fn parse_shape(s: &str) -> Shape {
    match s {
        "sections" => Shape::Sections,
        "chips" => Shape::Chips,
        _ => Shape::Bar,
    }
}

fn capitalize_str(capitalize: Capitalize) -> &'static str {
    match capitalize {
        Capitalize::None => "none",
        Capitalize::Upper => "upper",
        Capitalize::Lower => "lower",
        Capitalize::Title => "title",
    }
}

fn parse_capitalize(s: &str) -> Capitalize {
    match s {
        "upper" => Capitalize::Upper,
        "lower" => Capitalize::Lower,
        "title" => Capitalize::Title,
        _ => Capitalize::None,
    }
}

fn temperature_unit_str(unit: TemperatureUnit) -> &'static str {
    match unit {
        TemperatureUnit::Celsius => "celsius",
        TemperatureUnit::Fahrenheit => "fahrenheit",
    }
}

fn parse_temperature_unit(s: &str) -> TemperatureUnit {
    match s {
        "fahrenheit" => TemperatureUnit::Fahrenheit,
        _ => TemperatureUnit::Celsius,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::app::SurfaceRoot;
    use rsx::{App, Color, Component, WindowConfig, reset_layout_runtime, set_theme};

    // Switching the locale after the panel is built re-renders its labels live: the section titles are
    // reactive `t!` closures, so the rendered text changes from English to Spanish without a rebuild.
    #[test]
    fn labels_live_switch_locale() {
        use rsx::{ComponentList, DrawCommand, Event};

        fn has_text(tree: &ComponentList, needle: &str) -> bool {
            tree.commands().iter().any(|c| matches!(c, DrawCommand::Text { text, .. } if text.contains(needle)))
        }

        reset_layout_runtime();
        set_theme(NordTheme::new());
        let panel = settings_panel().expect("settings panel");
        let mut tree = ComponentList::new(SurfaceRoot::new(panel).expect("root"));
        tree.on_event(&Event::WindowResized { width: 380, height: 1200 });

        // Force the locale after building so the assertion is independent of the machine's system locale; the
        // labels are reactive `t!` closures, so `commands()` re-renders in whatever locale is active now.
        rsx::set_locale("en");
        assert!(has_text(&tree, "Settings"), "English title before switch");
        assert!(!has_text(&tree, "Ajustes"));

        rsx::set_locale("es");
        assert!(has_text(&tree, "Ajustes"), "Spanish title after live switch");
        assert!(!has_text(&tree, "Settings"), "English title gone after switch");
    }

    #[test]
    fn csv_round_trips_and_trims() {
        assert_eq!(split_csv("workspaces,  clock ,notes"), vec![
            "workspaces".to_string(),
            "clock".to_string(),
            "notes".to_string(),
        ]);
        assert_eq!(split_csv("  ,, "), Vec::<String>::new());
        assert_eq!(join_csv(&["a".to_string(), "b".to_string()]), "a, b");
    }

    #[test]
    fn saving_a_bar_keeps_each_entrys_own_settings() {
        let base = vec![
            ModuleEntry::bare("workspaces"),
            ModuleEntry {
                id: "clock".to_string(),
                accent: Some("red".to_string()),
                variant: None,
            },
            ModuleEntry {
                id: "clock".to_string(),
                variant: Some(crate::Variant::Filled),
                accent: None,
            },
        ];
        let same = entries_from_csv("workspaces, clock, clock", &base);
        assert_eq!(same, base, "an untouched field writes back what it read");

        let moved = entries_from_csv("clock, clock, workspaces", &base);
        assert_eq!(moved, vec![base[1].clone(), base[2].clone(), base[0].clone()]);

        let added = entries_from_csv("clock, clock, clock", &base);
        assert_eq!(added[2], ModuleEntry::bare("clock"));
    }

    #[test]
    fn enum_helpers_round_trip() {
        for e in Edge::ALL {
            assert_eq!(parse_edge(edge_str(e)), e);
        }
        for (s, a) in [("start", Align::Start), ("center", Align::Center), ("end", Align::End)] {
            assert_eq!(align_str(a), s);
            assert_eq!(parse_align(s), a);
        }
        for (s, sh) in [("bar", Shape::Bar), ("sections", Shape::Sections), ("chips", Shape::Chips)] {
            assert_eq!(shape_str(sh), s);
            assert_eq!(parse_shape(s), sh);
        }
    }

    struct SettingsPreview;

    impl App for SettingsPreview {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(NordTheme::new());
            let panel = settings_panel().expect("settings panel build failed");
            Box::new(SurfaceRoot::new(panel).expect("settings root"))
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

    /// Renders the settings panel end-to-end. Point config at a scratch dir so it never touches the real file:
    /// `XDG_CONFIG_HOME=/tmp/x RSX_VISUAL_SETTINGS_OUT=/tmp/s.png cargo test -p hyprshell --lib visual_settings -- --nocapture`.
    #[test]
    fn visual_settings_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_SETTINGS_OUT") else {
            eprintln!("set RSX_VISUAL_SETTINGS_OUT to render the settings panel; skipping");
            return;
        };
        crate::test_support::render_png(SettingsPreview, 380, 900, &out);
    }
}
