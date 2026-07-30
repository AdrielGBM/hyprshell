mod pages;

use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use telar::{
    AlignItems, Container, Input, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ReactiveList, RectStyle, RwSignal, SizeDimension, StyledContainer, Text, box_item, signal,
    use_theme,
};
use serde::Serialize;

use crate::core::config::{
    ActiveWindowConfig, Align, AnimationConfig, AppsConfig, AudioConfig, BackgroundConfig,
    BackgroundVisualiserConfig, BarConfig, BarsConfig, BatteryConfig, BluetoothConfig,
    BrightnessConfig, Capitalize, ClockConfig, Config, CornersConfig, DashboardConfig,
    DesktopClockConfig, DrawerConfig, Edge, FloatConfig, FullscreenPopups, GeneralConfig,
    GpuConfig, IconsConfig, KeyNavConfig, LauncherConfig, LockStatusConfig, LyricsConfig,
    MediaConfig, MediaScroll, ModuleEntry, NetworkConfig, NotificationsConfig, OsdConfig,
    PanelsConfig, PathsConfig, Placement, PopoutsConfig, RecorderConfig, ScaleConfig,
    ScreenshotConfig, Shape, ShapeConfig, SidebarConfig, StatusIconsConfig, TemperatureConfig,
    TemperatureUnit, ThemeConfig, ToastEvents, ToastsConfig, TrayConfig, UtilitiesConfig,
    VisualiserConfig, WallpaperConfig, WallpaperTransition, WeatherConfig, WorkspacesConfig,
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
const FULLSCREEN_POPUPS: &[&str] = &["on", "off", "never"];
const MODES: &[&str] = &["auto", "dark", "light"];
const VARIANTS: &[&str] = &["vibrant", "content", "expressive", "fidelity", "muted"];
const TRANSITIONS: &[&str] = &["fade", "wipe", "none"];
const SHOT_BACKENDS: &[&str] = &["auto", "screencopy", "grim"];
const RECORDER_BACKENDS: &[&str] = &["auto", "wf-recorder", "gpu-screen-recorder"];
const CURVES: &[&str] = &["gentle", "snappy", "bouncy"];
const EASINGS: &[&str] = &["linear", "ease-in", "ease-out", "ease-in-out"];

/// The nav pane's width, the gap to the forms beside it, and how wide the search box is. Wide enough for the
/// longest page label in either catalogue without wrapping, which is what stops the nav reflowing as the
/// language changes under it.
const NAV_WIDTH: f32 = 190.0;
const NAV_GAP: f32 = 24.0;
const SEARCH_WIDTH: f32 = 220.0;
const PLACEMENTS: &[&str] = &[
    "center",
    "top_left",
    "top_center",
    "top_right",
    "center_left",
    "center_right",
    "bottom_left",
    "bottom_center",
    "bottom_right",
];

/// What the theme picker cycles: every built-in palette, `custom` (which starts from nord for `[theme.colors]`
/// to override) and `dynamic` (the wallpaper's own). Derived from [`BUILT_IN_THEMES`] so a new palette shows up
/// here on its own.
fn theme_options() -> &'static [&'static str] {
    static OPTIONS: OnceLock<Vec<&'static str>> = OnceLock::new();
    OPTIONS.get_or_init(|| {
        let mut options = BUILT_IN_THEMES.to_vec();
        options.push("custom");
        options.push(crate::shared::scheme::DYNAMIC);
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
    let path = Arc::new(Config::default_path());
    let config = Arc::new(Config::load_or_default(&path));
    crate::shared::services::locale::attach(config.language());

    // The selection and the query are the whole state of the application. Both are plain signals on this
    // surface: a settings window reopened from scratch should start on the first page, not on wherever the
    // last one was left, which is a preference nobody asked to have remembered.
    let selected = signal(0usize);
    let query = signal(String::new());

    let body = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(NAV_GAP)
            .width(SizeDimension::Percent(1.0)),
        vec![
            nav_pane(selected.clone(), query.read_only(), theme)?,
            page_stack(selected.read_only(), query.read_only(), config, path, theme)?,
        ],
    )?;

    let panel = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(16.0)
            .width(SizeDimension::Percent(1.0)),
        vec![header(query, theme)?, Box::new(body)],
    )?;
    Ok(Box::new(panel))
}

/// The title and the search box, which is the one control that reaches every page.
fn header(query: RwSignal<String>, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = Text::auto(
        || telar::t!("settings.title"),
        LayoutStyle::new().flex_grow(1.0),
        move || {
            theme
                .text_style(FontRole::Title, theme.text)
                .with_weight(700)
        },
    )?;

    let input = Input::new(
        query,
        LayoutStyle::new()
            .flex_grow(1.0)
            .height(theme.font(FontRole::Body) * 1.6),
        move || theme.text_style(FontRole::Body, theme.text),
    )?
    .placeholder(telar::t!("settings.search"));
    let boxed = StyledContainer::new(
        LayoutStyle::new()
            .width(SEARCH_WIDTH)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(input)],
    )?;

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(12.0)
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(title), Box::new(boxed)],
    )?))
}

/// The nav: one row per page, the selected one filled, the ones a search excludes dimmed.
fn nav_pane(
    selected: RwSignal<usize>,
    query: telar::ReadSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(pages::PAGES.len());
    for (index, page) in pages::PAGES.iter().enumerate() {
        rows.push(nav_row(
            index,
            page,
            selected.clone(),
            query.clone(),
            theme,
        )?);
    }
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(2.0)
            .width(NAV_WIDTH)
            .flex_shrink(0.0),
        rows,
    )?))
}

fn nav_row(
    index: usize,
    page: &'static pages::Page,
    selected: RwSignal<usize>,
    query: telar::ReadSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let on_fg = theme.accent.most_readable(&[theme.text, theme.base]);
    // Read out of the two signals in one place: a row's colour depends on both, and the ink has to match
    // whatever fill the same frame drew.
    let ink = {
        let (selected, query) = (selected.read_only(), query.clone());
        move || {
            if selected.get() == index {
                on_fg
            } else if page.matches(&query.get()) {
                theme.text
            } else {
                theme.muted
            }
        }
    };
    let label_ink = ink.clone();
    let label = Text::auto(
        move || pages::label("settings.page", page.label),
        LayoutStyle::new().flex_grow(1.0),
        move || theme.text_style(FontRole::Body, label_ink()),
    )?;
    let glyph = icon_view(
        move || page.icon.to_string(),
        ink,
        theme.font(FontRole::Body) * 1.15,
    )?;

    let fill = selected.read_only();
    let press = selected;
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(10.0)
            .padding_horizontal(10.0)
            .padding_vertical(7.0)
            .width(SizeDimension::Percent(1.0)),
        move |_| {
            if fill.get() == index {
                RectStyle::filled(theme.accent, 8.0)
            } else {
                RectStyle::default()
            }
        },
        vec![glyph, Box::new(label)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.surface, 8.0))
    .on_press(move || press.set(index));
    Ok(Box::new(row))
}

/// The forms for the selected page, narrowed by the search.
///
/// A keyed list rather than a rebuilt column: the key is the page *and* the query, because narrowing a page
/// changes which forms are on it, and a list keyed on the page alone would keep showing the ones it had.
fn page_stack(
    selected: telar::ReadSignal<usize>,
    query: telar::ReadSignal<String>,
    config: Arc<Config>,
    path: Arc<PathBuf>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let height = config.settings_page_height();
    // The nav is outside this scroll area on purpose: a nav pane that scrolls away with the page it selects is
    // a list of links you have to scroll back up to use.
    let scroll = telar::LayoutScrollArea::new_with(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            // `min_width(0)` against flexbox's `auto` default: a form's rows are `width: 100%` of whatever they
            // are given, and a flex item that may not shrink below its content asks for the widest row it has,
            // which is how the page area ends up wider than the surface it is in.
            .min_width(0.0)
            .height(height),
        move |_viewport| {
            let (config, path) = (config.clone(), path.clone());
            let source = move || {
                // Both read out first: `visible` translates labels, which reads the locale signal, and a
                // nested read inside another signal's borrow is the re-entrant panic that only fires when the
                // widget is built.
                let index = selected.get();
                let text = query.get();
                pages::visible(index, &text)
                    .into_iter()
                    .map(|section| (text.clone(), section))
                    .collect()
            };
            let build = move |(_, section): (String, &'static pages::Section)| {
                (section.build)(&config, &path, theme)
            };
            Ok(Box::new(ReactiveList::with_style(
                LayoutStyle::new()
                    .flex_column()
                    .gap(20.0)
                    .width(SizeDimension::Percent(1.0)),
                source,
                // Keyed on the query as well as the form, because narrowing changes which forms are here — and
                // a form rebuilt is a form re-seeded from the file, which is what a user who has just saved
                // another one expects to see.
                |(query, section): &(String, &'static pages::Section)| {
                    (query.clone(), section.label)
                },
                build,
            )?) as Box<dyn LayoutItem>)
        },
    )?;
    Ok(Box::new(scroll))
}

fn general_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let lang = signal(telar::current_locale().unwrap_or_else(|| config.language()));
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
        language_field(|| telar::t!("settings.field.language"), lang.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.show_over_fullscreen"),
            over_fullscreen.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.logo"),
            logo.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.terminal"),
            terminal.clone(),
            "xterm",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.file_manager"),
            file_manager.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.audio_mixer"),
            audio_mixer.clone(),
            "pavucontrol",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.media_player"),
            media_player.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.browser"),
            browser.clone(),
            "xdg-open",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.editor"),
            editor.clone(),
            "xdg-open",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let legacy_terminal = config.general.terminal.clone();
    let save = save_button(
        || telar::t!("settings.save.general"),
        theme,
        move || {
            persist(
                &path,
                "general",
                &GeneralConfig {
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
                },
            );
        },
    )?;
    section(|| telar::t!("settings.section.general"), rows, save, theme)
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
        move || theme.text_style(FontRole::Body, theme.text),
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
    let mode = signal(t.mode.clone());
    let variant = signal(t.variant.clone());
    let fallback = signal(t.fallback.clone());
    let accent = signal(t.accent.clone());
    let font_family = signal(t.font_family.clone().unwrap_or_default());
    let radius = signal(opt_num(t.radius));
    let spacing = signal(opt_num(t.spacing));
    let font_size = signal(opt_num(t.font_size));
    let icon_size = signal(opt_num(t.icon_size));
    let icon_stroke = signal(opt_num(t.icon_stroke));
    let scale_rounding = signal(t.scale.rounding.to_string());
    let scale_spacing = signal(t.scale.spacing.to_string());
    let scale_font = signal(t.scale.font.to_string());
    let scale_icon = signal(t.scale.icon.to_string());

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.name"),
            name.clone(),
            theme_options(),
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.color_mode"),
            mode.clone(),
            MODES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.variant"),
            variant.clone(),
            VARIANTS,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.fallback"),
            fallback.clone(),
            BUILT_IN_THEMES,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.accent"),
            accent.clone(),
            "cyan",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.font_family"),
            font_family.clone(),
            "(default)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.radius"),
            radius.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.spacing"),
            spacing.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.font_size"),
            font_size.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.icon_size"),
            icon_size.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.icon_stroke"),
            icon_stroke.clone(),
            "(glyph)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.scale_rounding"),
            scale_rounding.clone(),
            "1",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.scale_spacing"),
            scale_spacing.clone(),
            "1",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.scale_font"),
            scale_font.clone(),
            "1",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.scale_icon"),
            scale_icon.clone(),
            "1",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.theme"),
        theme,
        move || {
            let value = ThemeConfig {
                name: name.peek(),
                mode: mode.peek(),
                variant: variant.peek(),
                fallback: fallback.peek(),
                accent: accent.peek(),
                font_family: opt_string(&font_family.peek()),
                radius: opt_u32(&radius.peek()),
                spacing: opt_u32(&spacing.peek()),
                font_size: opt_f32(&font_size.peek()),
                icon_size: opt_f32(&icon_size.peek()),
                icon_stroke: opt_f32(&icon_stroke.peek()),
                scale: ScaleConfig {
                    rounding: parse_f32(&scale_rounding.peek(), base.scale.rounding),
                    spacing: parse_f32(&scale_spacing.peek(), base.scale.spacing),
                    font: parse_f32(&scale_font.peek(), base.scale.font),
                    icon: parse_f32(&scale_icon.peek(), base.scale.icon),
                },
                // Carried through unchanged, like `colors`: per-role overrides and the export switches are nested tables the flat panel has no rows for, and rewriting the section must not drop them.
                fonts: base.fonts,
                export: base.export.clone(),
                colors: base.colors.clone(),
            };
            persist(&path, "theme", &value);
        },
    )?;
    section(|| telar::t!("settings.section.theme"), rows, save, theme)
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
        enum_field(
            || telar::t!("settings.field.mode"),
            mode.clone(),
            SHAPES,
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.frame_ring"),
            frame.clone(),
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "0", theme)?,
        text_field(
            || telar::t!("settings.field.spacing"),
            spacing.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.radius"),
            radius.clone(),
            "(theme)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.inactive_size"),
            inactive.clone(),
            "6",
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.shape"),
        theme,
        move || {
            let value = ShapeConfig {
                mode: parse_shape(&mode.peek()),
                frame: frame.peek(),
                gap: parse_u32(&gap.peek(), base.gap),
                spacing: opt_u32(&spacing.peek()),
                radius: opt_u32(&radius.peek()),
                inactive_size: parse_u32(&inactive.peek(), base.inactive_size),
            };
            persist(&path, "shape", &value);
        },
    )?;
    section(|| telar::t!("settings.section.shape"), rows, save, theme)
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
        text_field(
            || telar::t!("settings.field.size"),
            s.size.clone(),
            "34",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.start"),
            s.start.clone(),
            "module ids",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.center"),
            s.center.clone(),
            "module ids",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.end"),
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
    rows.extend(bar_rows(|| telar::t!("settings.subheader.top"), &top, theme)?);
    rows.extend(bar_rows(
        || telar::t!("settings.subheader.bottom"),
        &bottom,
        theme,
    )?);
    rows.extend(bar_rows(
        || telar::t!("settings.subheader.left"),
        &left,
        theme,
    )?);
    rows.extend(bar_rows(
        || telar::t!("settings.subheader.right"),
        &right,
        theme,
    )?);

    let base = bars.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.bars"),
        theme,
        move || {
            let value = BarsConfig {
                // Carried through unchanged: the panel edits the four zones, and rewriting the section must not drop a screen exclusion it has no field for.
                excluded_screens: base.excluded_screens.clone(),
                top: bar_from(&top, &base.top),
                bottom: bar_from(&bottom, &base.bottom),
                left: bar_from(&left, &base.left),
                right: bar_from(&right, &base.right),
            };
            persist(&path, "bars", &value);
        },
    )?;
    section(|| telar::t!("settings.section.bars"), rows, save, theme)
}

fn panels_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let p = &config.panels;
    let gap = signal(opt_num(p.gap));
    let drag_threshold = signal(p.drag_threshold.to_string());
    let opacity = signal(p.opacity.to_string());
    let drawer_w = signal(p.drawer.width.to_string());
    let drawer_h = signal(p.drawer.max_height.to_string());
    let float_w = signal(p.float.width.to_string());
    let float_h = signal(p.float.height.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.gap"),
            gap.clone(),
            "(auto)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.drawer_width"),
            drawer_w.clone(),
            "320",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.drawer_max_height"),
            drawer_h.clone(),
            "280",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.float_width"),
            float_w.clone(),
            "360",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.float_height"),
            float_h.clone(),
            "240",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.drag_threshold"),
            drag_threshold.clone(),
            "48",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.opacity"),
            opacity.clone(),
            "1",
            theme,
        )?,
    ];

    let base = *p;
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.panels"),
        theme,
        move || {
            let value = PanelsConfig {
                gap: opt_u32(&gap.peek()),
                drag_threshold: parse_f32(&drag_threshold.peek(), base.drag_threshold),
                opacity: parse_f32(&opacity.peek(), base.opacity),
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
        },
    )?;
    section(|| telar::t!("settings.section.panels"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.open_delay"),
            open_delay.clone(),
            "280",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.close_delay"),
            close_delay.clone(),
            "200",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "264",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_height"),
            max_height.clone(),
            "300",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.popouts"),
        theme,
        move || {
            let value = PopoutsConfig {
                enabled: enabled.peek(),
                open_delay: parse_u64(&open_delay.peek(), p.open_delay),
                close_delay: parse_u64(&close_delay.peek(), p.close_delay),
                width: parse_f32(&width.peek(), p.width),
                max_height: parse_f32(&max_height.peek(), p.max_height),
            };
            persist(&path, "popouts", &value);
        },
    )?;
    section(|| telar::t!("settings.section.popouts"), rows, save, theme)
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
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "1200",
            theme,
        )?,
    ];

    let base = *o;
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.osd"),
        theme,
        move || {
            let value = OsdConfig {
                edge: parse_edge(&edge.peek()),
                align: parse_align(&align.peek()),
                timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
            };
            persist(&path, "osd", &value);
        },
    )?;
    section(|| telar::t!("settings.section.osd"), rows, save, theme)
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
            || telar::t!("settings.field.provider"),
            provider.clone(),
            "https://api.iconify.design",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.default_set"),
            default_set.clone(),
            "lucide",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.app_icon_theme"),
            app_icon_theme.clone(),
            "auto",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.icons"),
        theme,
        move || {
            let value = IconsConfig {
                provider: provider.peek(),
                default_set: default_set.peek(),
                app_icon_theme: app_icon_theme.peek(),
            };
            persist(&path, "icons", &value);
        },
    )?;
    section(|| telar::t!("settings.section.icons"), rows, save, theme)
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
            || telar::t!("settings.field.twelve_hour"),
            twelve_hour.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.time_format"),
            format.clone(),
            "%H:%M:%S",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_date"),
            show_date.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.date_format"),
            date_format.clone(),
            "%a %d %b",
            theme,
        )?,
    ];

    let base = c.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.clock"),
        theme,
        move || {
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
        },
    )?;
    section(|| telar::t!("settings.section.clock"), rows, save, theme)
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
    let indicator_trail = signal(w.indicator_trail.to_string());
    let scroll = signal(w.scroll);
    let label = signal(w.label.clone());
    let occupied_label = signal(w.occupied_label.clone());
    let active_label = signal(w.active_label.clone());
    let capitalize = signal(capitalize_str(w.capitalize).to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.shown"),
            shown.clone(),
            "0",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.per_monitor"),
            per_monitor.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_special"),
            show_special.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.window_icons"),
            window_icons.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_window_icons"),
            max_icons.clone(),
            "4",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.occupied_background"),
            occupied.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.indicator"),
            indicator.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.indicator_trail"),
            indicator_trail.clone(),
            "0.35",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.scroll"), scroll.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.label"),
            label.clone(),
            "{id}",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.occupied_label"),
            occupied_label.clone(),
            "(label)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.active_label"),
            active_label.clone(),
            "(label)",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.capitalize"),
            capitalize.clone(),
            CAPITALIZATIONS,
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.workspaces"),
        theme,
        move || {
            let typed = label.peek();
            let value = WorkspacesConfig {
                shown: parse_u32(&shown.peek(), base.shown),
                per_monitor: per_monitor.peek(),
                show_special: show_special.peek(),
                window_icons: window_icons.peek(),
                max_window_icons: parse_u32(&max_icons.peek(), base.max_window_icons),
                occupied_background: occupied.peek(),
                indicator: indicator.peek(),
                indicator_trail: parse_f32(&indicator_trail.peek(), base.indicator_trail),
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
        },
    )?;
    section(|| telar::t!("settings.section.workspaces"), rows, save, theme)
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
    let visualiser = signal(m.visualiser);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.preferred_player"),
            preferred.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_chars"),
            max_chars.clone(),
            "40",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.scroll"),
            scroll.clone(),
            MEDIA_SCROLLS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.seek_seconds"),
            seek_seconds.clone(),
            "5",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.marquee"), marquee.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.marquee_speed_ms"),
            marquee_speed.clone(),
            "220",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.cover_visualiser"),
            visualiser.clone(),
            theme,
        )?,
    ];

    // Aliases are map-valued, so they stay hand-edited in the TOML for now, like `theme.colors`; carrying the
    // existing map through means saving this section does not silently drop them.
    let base = m.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.media"),
        theme,
        move || {
            let value = MediaConfig {
                preferred_player: preferred.peek(),
                max_chars: parse_u32(&max_chars.peek(), base.max_chars),
                scroll: parse_media_scroll(&scroll.peek()),
                marquee: marquee.peek(),
                marquee_speed_ms: parse_u32(&marquee_speed.peek(), base.marquee_speed_ms),
                seek_seconds: parse_u32(&seek_seconds.peek(), base.seek_seconds),
                visualiser: visualiser.peek(),
                aliases: base.aliases.clone(),
            };
            persist(&path, "media", &value);
        },
    )?;
    section(|| telar::t!("settings.section.media"), rows, save, theme)
}

fn lyrics_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let l = &config.lyrics;
    let enabled = signal(l.enabled);
    let online = signal(l.online);

    // The folder is `[paths] lyrics`, edited with the other paths rather than duplicated here.
    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.lyrics_online"),
            online.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.lyrics"),
        theme,
        move || {
            let value = LyricsConfig {
                enabled: enabled.peek(),
                online: online.peek(),
            };
            persist(&path, "lyrics", &value);
        },
    )?;
    section(|| telar::t!("settings.section.lyrics"), rows, save, theme)
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
            || telar::t!("settings.field.increment"),
            increment.clone(),
            "5",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_volume"),
            max_volume.clone(),
            "150",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.audio"),
        theme,
        move || {
            let value = AudioConfig {
                increment: parse_i32(&increment.peek(), a.increment),
                max_volume: parse_i32(&max_volume.peek(), a.max_volume),
            };
            persist(&path, "audio", &value);
        },
    )?;
    section(|| telar::t!("settings.section.audio"), rows, save, theme)
}

fn visualiser_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let v = config.visualiser;
    let bars = signal(v.bars.to_string());
    let smoothing = signal(v.smoothing.to_string());
    let floor_db = signal(v.floor_db.to_string());
    let gain = signal(v.gain.to_string());
    let beat = signal(v.beat_sensitivity.to_string());
    let frame_rate = signal(v.frame_rate.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.visualiser_bars"),
            bars.clone(),
            "48",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.smoothing"),
            smoothing.clone(),
            "0.6",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.floor_db"),
            floor_db.clone(),
            "-60",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gain"), gain.clone(), "1", theme)?,
        text_field(
            || telar::t!("settings.field.beat_sensitivity"),
            beat.clone(),
            "1.35",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.frame_rate"),
            frame_rate.clone(),
            "60",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.visualiser"),
        theme,
        move || {
            let value = VisualiserConfig {
                bars: parse_u32(&bars.peek(), v.bars),
                smoothing: parse_f32(&smoothing.peek(), v.smoothing),
                floor_db: parse_f32(&floor_db.peek(), v.floor_db),
                gain: parse_f32(&gain.peek(), v.gain),
                beat_sensitivity: parse_f32(&beat.peek(), v.beat_sensitivity),
                frame_rate: parse_u32(&frame_rate.peek(), v.frame_rate),
            };
            persist(&path, "visualiser", &value);
        },
    )?;
    section(|| telar::t!("settings.section.visualiser"), rows, save, theme)
}

fn background_visualiser_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let v = config.background.visualiser;
    let enabled = signal(v.enabled);
    let edge = signal(v.edge.as_str().to_string());
    let reach = signal(v.reach.to_string());
    let gap = signal(v.gap.to_string());
    let radius = signal(v.radius.to_string());
    let opacity = signal(v.opacity.to_string());
    let hide = signal(v.hide_when_silent);
    let accent = signal(v.accent);
    let margin = signal(v.margin.to_string());

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.reach"),
            reach.clone(),
            "140",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "3", theme)?,
        text_field(
            || telar::t!("settings.field.radius"),
            radius.clone(),
            "3",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.bar_opacity"),
            opacity.clone(),
            "0.75",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.margin"),
            margin.clone(),
            "0",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.hide_when_silent"),
            hide.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.accent"), accent.clone(), theme)?,
    ];

    let base = config.background.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.background_visualiser"),
        theme,
        move || {
            let visualiser = BackgroundVisualiserConfig {
                enabled: enabled.peek(),
                edge: parse_edge(&edge.peek()),
                reach: parse_u32(&reach.peek(), base.visualiser.reach),
                gap: parse_f32(&gap.peek(), base.visualiser.gap),
                radius: parse_f32(&radius.peek(), base.visualiser.radius),
                opacity: parse_f32(&opacity.peek(), base.visualiser.opacity),
                hide_when_silent: hide.peek(),
                accent: accent.peek(),
                margin: parse_u32(&margin.peek(), base.visualiser.margin),
            };
            persist(
                &path,
                "background",
                &BackgroundConfig {
                    visualiser,
                    ..base.clone()
                },
            );
        },
    )?;
    section(
        || telar::t!("settings.section.background_visualiser"),
        rows,
        save,
        theme,
    )
}

fn brightness_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let b = config.brightness;
    let increment = signal(b.increment.to_string());
    let external = signal(b.external);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.increment"),
            increment.clone(),
            "5",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.external_monitors"),
            external.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.brightness"),
        theme,
        move || {
            let value = BrightnessConfig {
                increment: parse_i32(&increment.peek(), b.increment),
                external: external.peek(),
            };
            persist(&path, "brightness", &value);
        },
    )?;
    section(|| telar::t!("settings.section.brightness"), rows, save, theme)
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
            || telar::t!("settings.field.unit"),
            unit.clone(),
            TEMPERATURE_UNITS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.sensor"),
            sensor.clone(),
            "(hottest)",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.warn"), warn.clone(), "70", theme)?,
        text_field(
            || telar::t!("settings.field.critical"),
            critical.clone(),
            "85",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.temperature"),
        theme,
        move || {
            let value = TemperatureConfig {
                unit: parse_temperature_unit(&unit.peek()),
                sensor: sensor.peek().trim().to_string(),
                warn: parse_f32(&warn.peek(), base.warn),
                critical: parse_f32(&critical.peek(), base.critical),
            };
            persist(&path, "temperature", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.temperature"),
        rows,
        save,
        theme,
    )
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
    let qalc = signal(l.qalc);
    let dangerous = signal(l.enable_dangerous_actions);
    let favourites = signal(join_csv(&l.favourites));
    let hidden = signal(join_csv(&l.hidden));

    let rows = vec![
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "640",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.height"),
            height.clone(),
            "420",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_results"),
            max_results.clone(),
            "12",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.fuzzy"), fuzzy.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.calculator"),
            calculator.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.qalc"), qalc.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.enable_dangerous_actions"),
            dangerous.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.favourites"),
            favourites.clone(),
            "desktop ids",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.hidden"),
            hidden.clone(),
            "desktop ids",
            theme,
        )?,
    ];

    let base = l.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.launcher"),
        theme,
        move || {
            let value = LauncherConfig {
                width: parse_u32(&width.peek(), base.width),
                height: parse_u32(&height.peek(), base.height),
                radius: base.radius,
                max_results: parse_u32(&max_results.peek(), base.max_results),
                fuzzy: fuzzy.peek(),
                calculator: calculator.peek(),
                qalc: qalc.peek(),
                favourites: split_csv(&favourites.peek()),
                hidden: split_csv(&hidden.peek()),
                // A list of tables, so it stays hand-edited in the TOML; carrying it through means saving here
                // does not silently drop the user's actions.
                actions: base.actions.clone(),
                enable_dangerous_actions: dangerous.peek(),
            };
            persist(&path, "launcher", &value);
        },
    )?;
    section(|| telar::t!("settings.section.launcher"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.critical_level"),
            critical_level.clone(),
            "0",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.critical_action"),
            critical_action.clone(),
            "suspend",
            theme,
        )?,
    ];

    // `warn_levels` is a list of tables, so it stays hand-edited in the TOML like `theme.colors`; carrying it
    // through means saving here does not silently drop the user's thresholds.
    let base = b.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.battery"),
        theme,
        move || {
            let value = BatteryConfig {
                enabled: enabled.peek(),
                warn_levels: base.warn_levels.clone(),
                critical_level: parse_i32(&critical_level.peek(), base.critical_level),
                critical_action: critical_action.peek().trim().to_string(),
            };
            persist(&path, "battery", &value);
        },
    )?;
    section(|| telar::t!("settings.section.battery"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.caps"), caps.clone(), theme)?,
        toggle_field(|| telar::t!("settings.field.num"), num.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.hide_inactive"),
            hide_inactive.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.lock_status"),
        theme,
        move || {
            let value = LockStatusConfig {
                caps: caps.peek(),
                num: num.peek(),
                hide_inactive: hide_inactive.peek(),
            };
            persist(&path, "lock_status", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.lock_status"),
        rows,
        save,
        theme,
    )
}

fn lock_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let l = &config.lock;
    let pam_service = signal(l.pam_service.clone());
    let max_tries = signal(l.max_tries.to_string());
    let lockout_seconds = signal(l.lockout_seconds.to_string());
    let lock_before_sleep = signal(l.lock_before_sleep);
    let fingerprint = signal(l.fingerprint);
    let howdy_command = signal(l.howdy_command.clone());
    let show_avatar = signal(l.show_avatar);
    let show_media = signal(l.show_media);
    let show_notifications = signal(l.show_notifications);
    let hide_notifs = signal(l.hide_notifs);

    let rows = vec![
        text_field(
            || telar::t!("settings.field.pam_service"),
            pam_service.clone(),
            "login",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_tries"),
            max_tries.clone(),
            "5",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.lockout_seconds"),
            lockout_seconds.clone(),
            "30",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.lock_before_sleep"),
            lock_before_sleep.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.fingerprint"),
            fingerprint.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.howdy_command"),
            howdy_command.clone(),
            "howdy compare",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_avatar"),
            show_avatar.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_media"),
            show_media.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_notifications"),
            show_notifications.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.hide_notifs"),
            hide_notifs.clone(),
            theme,
        )?,
    ];

    // The keys not on the form — the library path, the biometric budgets, the weather and resource rows — are
    // carried through unchanged, so saving here never quietly drops a setting the panel has no row for.
    let base = l.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.lock"),
        theme,
        move || {
            let value = crate::core::config::LockConfig {
                pam_service: pam_service.peek().trim().to_string(),
                max_tries: parse_i32(&max_tries.peek(), base.max_tries as i32).max(0) as u32,
                lockout_seconds: parse_i32(&lockout_seconds.peek(), base.lockout_seconds as i32)
                    .max(0) as u64,
                lock_before_sleep: lock_before_sleep.peek(),
                fingerprint: fingerprint.peek(),
                howdy_command: howdy_command.peek().trim().to_string(),
                show_avatar: show_avatar.peek(),
                show_media: show_media.peek(),
                show_notifications: show_notifications.peek(),
                hide_notifs: hide_notifs.peek(),
                ..base.clone()
            };
            persist(&path, "lock", &value);
        },
    )?;
    section(|| telar::t!("settings.section.lock"), rows, save, theme)
}

fn idle_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let i = &config.idle;
    let enabled = signal(i.enabled);
    let inhibit_when_audio = signal(i.inhibit_when_audio);
    let inhibit_when_charging = signal(i.inhibit_when_charging);
    let respect_inhibitors = signal(i.respect_inhibitors);

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.inhibit_when_audio"),
            inhibit_when_audio.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.inhibit_when_charging"),
            inhibit_when_charging.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.respect_inhibitors"),
            respect_inhibitors.clone(),
            theme,
        )?,
    ];

    // `stages` is a list of tables, so it stays hand-edited in the TOML — K13. Carried through, so switching
    // idle on from here does not wipe the timeouts it is switching on.
    let base = i.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.idle"),
        theme,
        move || {
            let value = crate::core::config::IdleConfig {
                enabled: enabled.peek(),
                stages: base.stages.clone(),
                inhibit_when_audio: inhibit_when_audio.peek(),
                inhibit_when_charging: inhibit_when_charging.peek(),
                respect_inhibitors: respect_inhibitors.peek(),
            };
            persist(&path, "idle", &value);
        },
    )?;
    section(|| telar::t!("settings.section.idle"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            "auto",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.card"),
            card.clone(),
            "card1",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.gpu"),
        theme,
        move || {
            let value = GpuConfig {
                enabled: enabled.peek(),
                backend: backend.peek(),
                card: card.peek(),
            };
            persist(&path, "gpu", &value);
        },
    )?;
    section(|| telar::t!("settings.section.gpu"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.location"),
            location.clone(),
            "Madrid",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.latitude"),
            latitude.clone(),
            "40.4168",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.longitude"),
            longitude.clone(),
            "-3.7038",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.refresh_minutes"),
            refresh.clone(),
            "15",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.forecast_days"),
            days.clone(),
            "7",
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.weather"),
        theme,
        move || {
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
        },
    )?;
    section(|| telar::t!("settings.section.weather"), rows, save, theme)
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
            || telar::t!("settings.field.tabs"),
            tabs.clone(),
            "dash, media, performance, weather",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.media_update_interval"),
            media.clone(),
            "500",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.resource_update_interval"),
            resources.clone(),
            "1000",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.first_day_of_week"),
            first_day.clone(),
            WEEKDAYS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.avatar"),
            avatar.clone(),
            "~/.face",
            theme,
        )?,
    ];

    let base = d.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.dashboard"),
        theme,
        move || {
            let value = DashboardConfig {
                tabs: split_csv(&tabs.peek()),
                media_update_interval: parse_u64(&media.peek(), base.media_update_interval),
                resource_update_interval: parse_u64(
                    &resources.peek(),
                    base.resource_update_interval,
                ),
                first_day_of_week: first_day.peek(),
                avatar: avatar.peek(),
            };
            persist(&path, "dashboard", &value);
        },
    )?;
    section(|| telar::t!("settings.section.dashboard"), rows, save, theme)
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
            || telar::t!("settings.field.wallpapers"),
            wallpapers.clone(),
            &show(config.wallpaper_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.lyrics"),
            lyrics.clone(),
            &show(config.lyrics_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.recordings"),
            recordings.clone(),
            &show(config.recordings_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.screenshots"),
            screenshots.clone(),
            &show(config.screenshot_dir()),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.assets"),
            assets.clone(),
            "",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.paths"),
        theme,
        move || {
            let value = PathsConfig {
                wallpapers: wallpapers.peek(),
                lyrics: lyrics.peek(),
                recordings: recordings.peek(),
                screenshots: screenshots.peek(),
                assets: assets.peek(),
            };
            persist(&path, "paths", &value);
        },
    )?;
    section(|| telar::t!("settings.section.paths"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.rescan_seconds"),
            rescan.clone(),
            "300",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_networks"),
            max_networks.clone(),
            "20",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_hidden"),
            show_hidden.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.network"),
        theme,
        move || {
            let value = NetworkConfig {
                enabled: enabled.peek(),
                rescan_seconds: parse_u32(&rescan.peek(), n.rescan_seconds),
                max_networks: parse_u32(&max_networks.peek(), n.max_networks),
                show_hidden: show_hidden.peek(),
            };
            persist(&path, "network", &value);
        },
    )?;
    section(|| telar::t!("settings.section.network"), rows, save, theme)
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.scan_on_open"),
            scan_on_open.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_devices"),
            max_devices.clone(),
            "12",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_unnamed"),
            show_unnamed.clone(),
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.bluetooth"),
        theme,
        move || {
            let value = BluetoothConfig {
                enabled: enabled.peek(),
                scan_on_open: scan_on_open.peek(),
                max_devices: parse_u32(&max_devices.peek(), b.max_devices),
                show_unnamed: show_unnamed.peek(),
            };
            persist(&path, "bluetooth", &value);
        },
    )?;
    section(|| telar::t!("settings.section.bluetooth"), rows, save, theme)
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
            || telar::t!("settings.field.icons"),
            icons.clone(),
            "volume, mic, network, battery",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.spacing"),
            spacing.clone(),
            "0.35",
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.status_icons"),
        theme,
        move || {
            let value = StatusIconsConfig {
                icons: split_csv(&icons.peek()),
                spacing: parse_f32(&spacing.peek(), base.spacing),
            };
            persist(&path, "status_icons", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.status_icons"),
        rows,
        save,
        theme,
    )
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
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(|| telar::t!("settings.field.compact"), compact.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.recolour"),
            recolour.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.background"),
            background.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.hidden"),
            hidden.clone(),
            "steam_app_*",
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.tray"),
        theme,
        move || {
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
        },
    )?;
    section(|| telar::t!("settings.section.tray"), rows, save, theme)
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
    let inverted = signal(w.inverted);
    let max_chars = signal(w.max_chars.to_string());

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.compact"), compact.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.show_icon"),
            show_icon.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.inverted"),
            inverted.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_chars"),
            max_chars.clone(),
            "300",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.active_window"),
        theme,
        move || {
            let value = ActiveWindowConfig {
                compact: compact.peek(),
                show_icon: show_icon.peek(),
                inverted: inverted.peek(),
                max_chars: parse_u32(&max_chars.peek(), w.max_chars),
            };
            persist(&path, "active_window", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.active_window"),
        rows,
        save,
        theme,
    )
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
    let fullscreen = signal(fullscreen_popups_str(n.fullscreen).to_string());
    let group_by_app = signal(n.group_by_app);
    let group_preview = signal(n.group_preview_num.to_string());
    let action_on_click = signal(n.action_on_click);
    let body_lines = signal(n.body_lines.to_string());
    let open_expanded = signal(n.open_expanded);
    let sound = signal(n.sound.clone());
    let clear_threshold = signal(n.clear_threshold.to_string());

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_visible"),
            max_visible.clone(),
            "4",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "5000",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.critical_sticky"),
            critical.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "380",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "10", theme)?,
        enum_field(
            || telar::t!("settings.field.fullscreen_popups"),
            fullscreen.clone(),
            FULLSCREEN_POPUPS,
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.group_by_app"),
            group_by_app.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.group_preview_num"),
            group_preview.clone(),
            "3",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.action_on_click"),
            action_on_click.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.body_lines"),
            body_lines.clone(),
            "4",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.open_expanded"),
            open_expanded.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.sound"),
            sound.clone(),
            "canberra-gtk-play -i message",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.clear_threshold"),
            clear_threshold.clone(),
            "0.35",
            theme,
        )?,
    ];

    let base = n.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.notifications"),
        theme,
        move || {
            let value = NotificationsConfig {
                edge: parse_edge(&edge.peek()),
                align: parse_align(&align.peek()),
                max_visible: parse_u32(&max_visible.peek(), base.max_visible),
                timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
                critical_sticky: critical.peek(),
                width: parse_f32(&width.peek(), base.width),
                gap: parse_f32(&gap.peek(), base.gap),
                fullscreen: parse_fullscreen_popups(&fullscreen.peek()),
                group_by_app: group_by_app.peek(),
                group_preview_num: parse_u32(&group_preview.peek(), base.group_preview_num),
                action_on_click: action_on_click.peek(),
                body_lines: parse_u32(&body_lines.peek(), base.body_lines),
                open_expanded: open_expanded.peek(),
                sound: sound.peek(),
                clear_threshold: parse_f32(&clear_threshold.peek(), base.clear_threshold),
            };
            persist(&path, "notifications", &value);
        },
    )?;
    section(
        || telar::t!("settings.section.notifications"),
        rows,
        save,
        theme,
    )
}

/// `[toasts]`, including the per-event switches.
///
/// The event matrix is a nested table (`[toasts.events]`) with a fixed set of keys, so it is edited here rather
/// than left to the TOML — the same reason `background.monitors` came off the map-editing list: the keys are
/// enumerable, so the panel can name them all.
fn toasts_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let t = &config.toasts;
    let enabled = signal(t.enabled);
    let edge = signal(edge_str(t.edge).to_string());
    let align = signal(align_str(t.align).to_string());
    let max_toasts = signal(t.max_toasts.to_string());
    let timeout = signal(t.timeout_ms.to_string());
    let width = signal(t.width.to_string());
    let gap = signal(t.gap.to_string());

    let events = t.events;
    let config_loaded = signal(events.config_loaded);
    let charging = signal(events.charging);
    let game_mode = signal(events.game_mode);
    let dnd = signal(events.dnd);
    let audio_output = signal(events.audio_output);
    let audio_input = signal(events.audio_input);
    let lock_keys = signal(events.lock_keys);
    let kb_layout = signal(events.kb_layout);
    let vpn = signal(events.vpn);
    let now_playing = signal(events.now_playing);
    let screenshot = signal(events.screenshot);
    let recording = signal(events.recording);

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.align"),
            align.clone(),
            ALIGNS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_toasts"),
            max_toasts.clone(),
            "3",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.timeout_ms"),
            timeout.clone(),
            "2500",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.width"),
            width.clone(),
            "300",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.gap"), gap.clone(), "8", theme)?,
        subheader(|| telar::t!("settings.subheader.events"), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_config_loaded"),
            config_loaded.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_charging"),
            charging.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_game_mode"),
            game_mode.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.event_dnd"), dnd.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_audio_output"),
            audio_output.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_audio_input"),
            audio_input.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_lock_keys"),
            lock_keys.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_kb_layout"),
            kb_layout.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.event_vpn"), vpn.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.event_now_playing"),
            now_playing.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_screenshot"),
            screenshot.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.event_recording"),
            recording.clone(),
            theme,
        )?,
    ];

    let base = t.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.toasts"),
        theme,
        move || {
            let value = ToastsConfig {
                enabled: enabled.peek(),
                edge: parse_edge(&edge.peek()),
                align: parse_align(&align.peek()),
                max_toasts: parse_u32(&max_toasts.peek(), base.max_toasts),
                timeout_ms: parse_u64(&timeout.peek(), base.timeout_ms),
                width: parse_f32(&width.peek(), base.width),
                gap: parse_f32(&gap.peek(), base.gap),
                events: ToastEvents {
                    config_loaded: config_loaded.peek(),
                    charging: charging.peek(),
                    game_mode: game_mode.peek(),
                    dnd: dnd.peek(),
                    audio_output: audio_output.peek(),
                    audio_input: audio_input.peek(),
                    lock_keys: lock_keys.peek(),
                    kb_layout: kb_layout.peek(),
                    vpn: vpn.peek(),
                    now_playing: now_playing.peek(),
                    screenshot: screenshot.peek(),
                    recording: recording.peek(),
                },
            };
            persist(&path, "toasts", &value);
        },
    )?;
    section(|| telar::t!("settings.section.toasts"), rows, save, theme)
}

fn screenshot_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let s = &config.screenshot;
    let copy = signal(s.copy);
    let save_to_disk = signal(s.save);
    let cursor = signal(s.include_cursor);
    let freeze = signal(s.freeze);
    let notify = signal(s.notify);
    let backend = signal(s.backend.clone());
    let file_name = signal(s.file_name.clone());
    let annotator = signal(s.annotator.clone());

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.copy"), copy.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.save"),
            save_to_disk.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.include_cursor"),
            cursor.clone(),
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.freeze"), freeze.clone(), theme)?,
        toggle_field(|| telar::t!("settings.field.notify"), notify.clone(), theme)?,
        enum_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            SHOT_BACKENDS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.file_name"),
            file_name.clone(),
            "screenshot_%Y-%m-%d_%H-%M-%S",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.annotator"),
            annotator.clone(),
            "satty --filename {file}",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.screenshot"),
        theme,
        move || {
            let value = ScreenshotConfig {
                copy: copy.peek(),
                save: save_to_disk.peek(),
                include_cursor: cursor.peek(),
                freeze: freeze.peek(),
                notify: notify.peek(),
                backend: backend.peek(),
                file_name: file_name.peek(),
                annotator: annotator.peek(),
            };
            persist(&path, "screenshot", &value);
        },
    )?;
    section(|| telar::t!("settings.section.screenshot"), rows, save, theme)
}

fn recorder_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let r = &config.recorder;
    let backend = signal(r.backend.clone());
    let audio = signal(r.audio);
    let device = signal(r.audio_device.clone());
    let fps = signal(r.fps.to_string());
    let file_name = signal(r.file_name.clone());
    let notify = signal(r.notify);
    let max_entries = signal(r.max_entries.to_string());

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.backend"),
            backend.clone(),
            RECORDER_BACKENDS,
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.audio"), audio.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.audio_device"),
            device.clone(),
            "default_output",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.fps"), fps.clone(), "60", theme)?,
        text_field(
            || telar::t!("settings.field.file_name"),
            file_name.clone(),
            "recording_%Y-%m-%d_%H-%M-%S",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.notify"), notify.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.max_entries"),
            max_entries.clone(),
            "12",
            theme,
        )?,
    ];

    let base = r.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.recorder"),
        theme,
        move || {
            let value = RecorderConfig {
                backend: backend.peek(),
                audio: audio.peek(),
                audio_device: device.peek(),
                fps: parse_u32(&fps.peek(), base.fps),
                file_name: file_name.peek(),
                notify: notify.peek(),
                max_entries: parse_u32(&max_entries.peek(), base.max_entries),
            };
            persist(&path, "recorder", &value);
        },
    )?;
    section(|| telar::t!("settings.section.recorder"), rows, save, theme)
}

fn utilities_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let u = &config.utilities;
    let toggles = signal(join_csv(&u.toggles));
    let show_capture = signal(u.show_capture);
    let show_recordings = signal(u.show_recordings);
    let columns = signal(u.columns.to_string());
    let preview = signal(u.window_preview_ms.to_string());

    let rows = vec![
        text_field(
            || telar::t!("settings.field.toggles"),
            toggles.clone(),
            "wifi, bluetooth, mic, dnd",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_capture"),
            show_capture.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_recordings"),
            show_recordings.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.columns"),
            columns.clone(),
            "4",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.window_preview_ms"),
            preview.clone(),
            "1000",
            theme,
        )?,
    ];

    let base = u.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.utilities"),
        theme,
        move || {
            let value = UtilitiesConfig {
                toggles: split_csv(&toggles.peek()),
                show_capture: show_capture.peek(),
                show_recordings: show_recordings.peek(),
                columns: parse_u32(&columns.peek(), base.columns),
                window_preview_ms: parse_u64(&preview.peek(), base.window_preview_ms),
            };
            persist(&path, "utilities", &value);
        },
    )?;
    section(|| telar::t!("settings.section.utilities"), rows, save, theme)
}

fn sidebar_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let s = &config.sidebar;
    let edge = signal(edge_str(s.edge).to_string());
    let size = signal(s.size.to_string());
    let show_toggles = signal(s.show_toggles);
    let show_history = signal(s.show_history);

    let rows = vec![
        enum_field(
            || telar::t!("settings.field.edge"),
            edge.clone(),
            EDGES,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.size"),
            size.clone(),
            "400",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_toggles"),
            show_toggles.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_history"),
            show_history.clone(),
            theme,
        )?,
    ];

    let base = s.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.sidebar"),
        theme,
        move || {
            let value = SidebarConfig {
                edge: parse_edge(&edge.peek()),
                size: parse_u32(&size.peek(), base.size),
                show_toggles: show_toggles.peek(),
                show_history: show_history.peek(),
            };
            persist(&path, "sidebar", &value);
        },
    )?;
    section(|| telar::t!("settings.section.sidebar"), rows, save, theme)
}

fn keynav_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let vim = signal(config.keynav.vim);
    let rows = vec![toggle_field(
        || telar::t!("settings.field.vim"),
        vim.clone(),
        theme,
    )?];
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.keynav"),
        theme,
        move || {
            persist(&path, "keynav", &KeyNavConfig { vim: vim.peek() });
        },
    )?;
    section(|| telar::t!("settings.section.keynav"), rows, save, theme)
}

/// Every screen a `[background.monitors]` row should exist for: the ones plugged in now, plus any the config
/// already names.
///
/// Both halves matter. Only listing the connected screens would silently drop the override a user wrote for the
/// monitor they left at the office the moment they saved anything; only listing the configured ones would mean
/// a screen can never get its first override from the UI, which is the whole of J9.
fn monitor_keys(configured: &std::collections::HashMap<String, PathBuf>) -> Vec<String> {
    let mut names: Vec<String> = platform_layershell::outputs()
        .into_iter()
        .filter_map(|output| output.name)
        .collect();
    for name in configured.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    names.sort();
    names
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
    let transition = signal(b.transition.id().to_string());
    let transition_ms = signal(b.transition_ms.to_string());

    let mut rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.image"),
            image.clone(),
            "~/wall.png",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.transition"),
            transition.clone(),
            TRANSITIONS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.transition_ms"),
            transition_ms.clone(),
            "600",
            theme,
        )?,
    ];

    // One row per screen, which is what makes a map-valued section editable without K13's generic key/value
    // machinery: the keys are not free text here, they are the monitors that exist.
    let names = monitor_keys(&b.monitors);
    let mut monitors: Vec<(String, RwSignal<String>)> = Vec::new();
    if !names.is_empty() {
        rows.push(subheader(|| telar::t!("settings.subheader.monitors"), theme)?);
    }
    for name in names {
        let value = signal(
            b.monitors
                .get(&name)
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
        );
        let label = name.clone();
        rows.push(text_field(
            move || label.clone(),
            value.clone(),
            "(global image)",
            theme,
        )?);
        monitors.push((name, value));
    }

    let base = b.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.background"),
        theme,
        move || {
            let monitors = monitors
                .iter()
                .filter_map(|(name, value)| {
                    opt_string(&value.peek()).map(|path| (name.clone(), PathBuf::from(path)))
                })
                .collect();
            let value = BackgroundConfig {
                enabled: enabled.peek(),
                image: opt_string(&image.peek()).map(PathBuf::from),
                monitors,
                transition: WallpaperTransition::from_id(&transition.peek()).unwrap_or_default(),
                transition_ms: parse_u64(&transition_ms.peek(), base.transition_ms),
                clock: base.clock.clone(),
                visualiser: base.visualiser,
            };
            persist(&path, "background", &value);
        },
    )?;
    section(|| telar::t!("settings.section.background"), rows, save, theme)
}

fn wallpaper_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let w = &config.wallpaper;
    let enabled = signal(w.enabled);
    let recursive = signal(w.recursive);
    let max_entries = signal(w.max_entries.to_string());
    let thumbnail_size = signal(w.thumbnail_size.to_string());
    let extensions = signal(join_csv(&w.extensions));

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.recursive"),
            recursive.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.max_entries"),
            max_entries.clone(),
            "2000",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.thumbnail_size"),
            thumbnail_size.clone(),
            "320",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.extensions"),
            extensions.clone(),
            "png, jpg",
            theme,
        )?,
    ];

    let base = w.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.wallpaper"),
        theme,
        move || {
            let value = WallpaperConfig {
                enabled: enabled.peek(),
                recursive: recursive.peek(),
                max_entries: parse_u32(&max_entries.peek(), base.max_entries),
                thumbnail_size: parse_u32(&thumbnail_size.peek(), base.thumbnail_size),
                extensions: split_csv(&extensions.peek()),
            };
            persist(&path, "wallpaper", &value);
        },
    )?;
    section(|| telar::t!("settings.section.wallpaper"), rows, save, theme)
}

/// The clock drawn on the wallpaper. Its own section rather than rows inside `[background]`: it is a nested
/// table, and one Save writing both would mean every clock tweak rewrote the wallpaper settings with it.
fn desktop_clock_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let c = &config.background.clock;
    let enabled = signal(c.enabled);
    let position = signal(c.position.id().to_string());
    let scale = signal(c.scale.to_string());
    let margin = signal(c.margin.to_string());
    let invert = signal(c.invert);
    let show_date = signal(c.show_date);
    let format = signal(c.format.clone().unwrap_or_default());
    let date_format = signal(c.date_format.clone().unwrap_or_default());
    let background = signal(c.background);
    let opacity = signal(c.background_opacity.to_string());
    let blur = signal(c.background_blur.to_string());
    let shadow = signal(c.shadow);

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        enum_field(
            || telar::t!("settings.field.position"),
            position.clone(),
            PLACEMENTS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.scale"),
            scale.clone(),
            "3",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.margin"),
            margin.clone(),
            "48",
            theme,
        )?,
        toggle_field(|| telar::t!("settings.field.invert"), invert.clone(), theme)?,
        toggle_field(
            || telar::t!("settings.field.show_date"),
            show_date.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.time_format"),
            format.clone(),
            "(clock)",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.date_format"),
            date_format.clone(),
            "(clock)",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.plate"),
            background.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.plate_opacity"),
            opacity.clone(),
            "0.35",
            theme,
        )?,
        text_field(|| telar::t!("settings.field.blur"), blur.clone(), "0", theme)?,
        toggle_field(|| telar::t!("settings.field.shadow"), shadow.clone(), theme)?,
    ];

    let base = config.background.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.desktop_clock"),
        theme,
        move || {
            let clock = DesktopClockConfig {
                enabled: enabled.peek(),
                position: Placement::from_id(&position.peek()).unwrap_or_default(),
                scale: parse_f32(&scale.peek(), base.clock.scale),
                margin: parse_u32(&margin.peek(), base.clock.margin),
                invert: invert.peek(),
                show_date: show_date.peek(),
                format: opt_string(&format.peek()),
                date_format: opt_string(&date_format.peek()),
                background: background.peek(),
                background_opacity: parse_f32(&opacity.peek(), base.clock.background_opacity),
                background_blur: parse_f32(&blur.peek(), base.clock.background_blur),
                shadow: shadow.peek(),
            };
            persist(
                &path,
                "background",
                &BackgroundConfig {
                    clock,
                    ..base.clone()
                },
            );
        },
    )?;
    section(
        || telar::t!("settings.section.desktop_clock"),
        rows,
        save,
        theme,
    )
}

fn animation_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let a = &config.animation;
    let enabled = signal(a.enabled);
    let scale = signal(a.duration_scale.to_string());
    let curve = signal(a.curve.clone());
    let easing = signal(a.easing.clone());
    let panel_ms = signal(a.panel_duration_ms.to_string());

    let rows = vec![
        toggle_field(|| telar::t!("settings.field.enabled"), enabled.clone(), theme)?,
        text_field(
            || telar::t!("settings.field.duration_scale"),
            scale.clone(),
            "1",
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.curve"),
            curve.clone(),
            CURVES,
            theme,
        )?,
        enum_field(
            || telar::t!("settings.field.easing"),
            easing.clone(),
            EASINGS,
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.panel_duration_ms"),
            panel_ms.clone(),
            "180",
            theme,
        )?,
    ];

    let base = a.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.animation"),
        theme,
        move || {
            let value = AnimationConfig {
                enabled: enabled.peek(),
                duration_scale: parse_f32(&scale.peek(), base.duration_scale),
                curve: curve.peek(),
                easing: easing.peek(),
                panel_duration_ms: parse_u64(&panel_ms.peek(), base.panel_duration_ms),
            };
            persist(&path, "animation", &value);
        },
    )?;
    section(|| telar::t!("settings.section.animation"), rows, save, theme)
}

/// K12: what this shell is and what it found to talk to.
///
/// Readings, not fields — so it has no Save. The compositor and session lines are what a bug report needs
/// first and what a user otherwise has to leave the shell to find.
fn about_section(
    _config: &Config,
    _path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let rows = vec![
        reading_row(
            || telar::t!("settings.field.version"),
            env!("CARGO_PKG_VERSION"),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.compositor"),
            &env_or_unknown("HYPRLAND_INSTANCE_SIGNATURE").map_or_else(
                || telar::t!("settings.about.not_hyprland"),
                |_| "Hyprland".to_string(),
            ),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.session"),
            &env_or_unknown("XDG_SESSION_TYPE").unwrap_or_else(|| telar::t!("common.unknown")),
            theme,
        )?,
        reading_row(
            || telar::t!("settings.field.config_file"),
            &Config::default_path().display().to_string(),
            theme,
        )?,
    ];
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        std::iter::once(section_label(|| telar::t!("settings.section.about"), theme)?)
            .chain(rows)
            .collect(),
    )?;
    Ok(Box::new(column))
}

/// A non-empty environment variable, which is the only kind worth reporting.
fn env_or_unknown(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

/// A label and a value the user cannot change — the About page's only row shape.
fn reading_row(
    label: impl Fn() -> String + 'static,
    value: &str,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = value.to_string();
    let text = Text::auto(
        move || value.clone(),
        LayoutStyle::new().flex_grow(1.0),
        move || theme.text_style(FontRole::Body, theme.text),
    )?;
    labelled(label, Box::new(text), theme)
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
            || telar::t!("settings.field.top_left"),
            tl.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.top_right"),
            tr.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.bottom_left"),
            bl.clone(),
            "module id",
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.bottom_right"),
            br.clone(),
            "module id",
            theme,
        )?,
    ];

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.corners"),
        theme,
        move || {
            let value = CornersConfig {
                top_left: opt_string(&tl.peek()),
                top_right: opt_string(&tr.peek()),
                bottom_left: opt_string(&bl.peek()),
                bottom_right: opt_string(&br.peek()),
            };
            persist(&path, "corners", &value);
        },
    )?;
    section(|| telar::t!("settings.section.corners"), rows, save, theme)
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
    let text = Text::auto(label, LayoutStyle::new(), move || {
        theme
            .text_style(FontRole::Body, theme.text)
            .with_weight(700)
    })?;
    Ok(Box::new(text))
}

fn subheader(
    label: impl Fn() -> String + 'static,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(label, LayoutStyle::new(), move || {
        theme
            .text_style(FontRole::Caption, theme.muted)
            .with_weight(700)
    })?;
    Ok(Box::new(text))
}

fn labelled(
    label: impl Fn() -> String + 'static,
    control: Box<dyn LayoutItem>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label_text = Text::auto(label, LayoutStyle::new().width(120.0), move || {
        theme.text_style(FontRole::Body, theme.subtle)
    })?;
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
        move || theme.text_style(FontRole::Body, theme.text),
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
                telar::t!("common.on")
            } else {
                telar::t!("common.off")
            }
        },
        LayoutStyle::new(),
        move || {
            let fg = if value_color.get() { on_fg } else { theme.text };
            theme.text_style(FontRole::Caption, fg).with_weight(700)
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
        move || theme.text_style(FontRole::Body, theme.text),
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
    let text = Text::auto(label, LayoutStyle::new(), move || {
        theme.text_style(FontRole::Body, fg).with_weight(700)
    })?;
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

fn fullscreen_popups_str(policy: FullscreenPopups) -> &'static str {
    match policy {
        FullscreenPopups::On => "on",
        FullscreenPopups::Off => "off",
        FullscreenPopups::Never => "never",
    }
}

fn parse_fullscreen_popups(s: &str) -> FullscreenPopups {
    match s {
        "on" => FullscreenPopups::On,
        "never" => FullscreenPopups::Never,
        _ => FullscreenPopups::Off,
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
    use telar::{App, Color, Component, WindowConfig, reset_layout_runtime, set_theme};

    // Switching the locale after the panel is built re-renders its labels live: the section titles are
    // reactive `t!` closures, so the rendered text changes from English to Spanish without a rebuild.
    #[test]
    fn labels_live_switch_locale() {
        use telar::{ComponentList, DrawCommand, Event};

        fn has_text(tree: &ComponentList, needle: &str) -> bool {
            tree.commands()
                .iter()
                .any(|c| matches!(c, DrawCommand::Text { text, .. } if text.contains(needle)))
        }

        reset_layout_runtime();
        set_theme(NordTheme::new());
        let panel = settings_panel().expect("settings panel");
        let mut tree = ComponentList::new(SurfaceRoot::new(panel).expect("root"));
        tree.on_event(&Event::WindowResized {
            width: 380,
            height: 1200,
        });

        // Force the locale after building so the assertion is independent of the machine's system locale; the
        // labels are reactive `t!` closures, so `commands()` re-renders in whatever locale is active now.
        telar::set_locale("en");
        assert!(has_text(&tree, "Settings"), "English title before switch");
        assert!(!has_text(&tree, "Ajustes"));

        telar::set_locale("es");
        assert!(
            has_text(&tree, "Ajustes"),
            "Spanish title after live switch"
        );
        assert!(
            !has_text(&tree, "Settings"),
            "English title gone after switch"
        );
    }

    #[test]
    fn csv_round_trips_and_trims() {
        assert_eq!(
            split_csv("workspaces,  clock ,notes"),
            vec![
                "workspaces".to_string(),
                "clock".to_string(),
                "notes".to_string(),
            ]
        );
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
        assert_eq!(
            moved,
            vec![base[1].clone(), base[2].clone(), base[0].clone()]
        );

        let added = entries_from_csv("clock, clock, clock", &base);
        assert_eq!(added[2], ModuleEntry::bare("clock"));
    }

    #[test]
    fn enum_helpers_round_trip() {
        for e in Edge::ALL {
            assert_eq!(parse_edge(edge_str(e)), e);
        }
        for (s, a) in [
            ("start", Align::Start),
            ("center", Align::Center),
            ("end", Align::End),
        ] {
            assert_eq!(align_str(a), s);
            assert_eq!(parse_align(s), a);
        }
        for (s, sh) in [
            ("bar", Shape::Bar),
            ("sections", Shape::Sections),
            ("chips", Shape::Chips),
        ] {
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
    /// `XDG_CONFIG_HOME=/tmp/x TELAR_VISUAL_SETTINGS_OUT=/tmp/s.png cargo test -p hyprshell --lib visual_settings -- --nocapture`.
    #[test]
    fn visual_settings_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_SETTINGS_OUT") else {
            eprintln!("set TELAR_VISUAL_SETTINGS_OUT to render the settings panel; skipping");
            return;
        };
        crate::test_support::render_png(SettingsPreview, 920, 680, &out);
    }
}
