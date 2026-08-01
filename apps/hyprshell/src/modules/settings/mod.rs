mod pages;

use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, OnceLock};

use serde::Serialize;
use telar::{
    AlignItems, Color, Container, Input, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ReactiveList, Rect, RectStyle, RwSignal, ShapeStyle, SizeDimension, StyledContainer, Text,
    box_item, signal, use_theme,
};

use crate::core::config::{
    ActiveWindowConfig, Align, AnimationConfig, AppsConfig, AudioConfig, BackgroundConfig,
    BackgroundVisualiserConfig, BarConfig, BarsConfig, BatteryConfig, BatteryWarning,
    BluetoothConfig, BrightnessConfig, Capitalize, ClockConfig, Config, CornersConfig,
    DashboardConfig, DesktopClockConfig, DrawerConfig, Edge, FloatConfig, FullscreenPopups,
    GeneralConfig, GpuConfig, IconsConfig, IdleConfig, IdleStage, KeyNavConfig, LauncherConfig,
    LockStatusConfig, LyricsConfig, MediaConfig, MediaScroll, ModuleEntry, ModuleOverride,
    NetworkConfig, NotificationsConfig, OpenMode, OsdConfig, PanelsConfig, PathsConfig, Placement,
    PopoutsConfig, RecorderConfig, ScaleConfig, ScreenshotConfig, Shape, ShapeConfig,
    SidebarConfig, StatusIconsConfig, TemperatureConfig, TemperatureUnit, ThemeConfig, ToastEvents,
    ToastsConfig, TrayConfig, UtilitiesConfig, Variant, VisualiserConfig, WallpaperConfig,
    WallpaperTransition, WeatherConfig, WorkspacesConfig,
};
use crate::shared::icon::icon_view;
use crate::shared::module::{icon_px, module_fg};
use crate::shared::state::kept;
use crate::shared::services::apps::{self, App};
use crate::shared::theme::{BUILT_IN_THEMES, FontRole, NordTheme, THEME_TOKENS};

const EDGES: &[&str] = &["top", "bottom", "left", "right"];
const ALIGNS: &[&str] = &["start", "center", "end"];
const SHAPES: &[&str] = &["bar", "sections", "chips"];
const LANGUAGES: &[&str] = &["en", "es"];
const MEDIA_SCROLLS: &[&str] = &["volume", "track", "seek", "none"];
const CAPITALIZATIONS: &[&str] = &["none", "upper", "lower", "title"];
const TEMPERATURE_UNITS: &[&str] = &["celsius", "fahrenheit"];
/// This application's module id, which is also the id its surface is registered under — what a reload needs to
/// know to leave the window that caused it alone (see [`crate::core::shell::authored_change`]).
const MODULE: &str = "settings";

const WEEKDAYS: &[&str] = &["monday", "sunday", "saturday"];
const FULLSCREEN_POPUPS: &[&str] = &["on", "off", "never"];
const MODES: &[&str] = &["auto", "dark", "light"];
const VARIANTS: &[&str] = &["vibrant", "content", "expressive", "fidelity", "muted"];
const TRANSITIONS: &[&str] = &["fade", "wipe", "none"];
const SHOT_BACKENDS: &[&str] = &["auto", "screencopy", "grim"];
const RECORDER_BACKENDS: &[&str] = &["auto", "wf-recorder", "gpu-screen-recorder"];
const CURVES: &[&str] = &["gentle", "snappy", "bouncy"];
const VARIANT_STYLES: &[&str] = &["default", "filled"];
const OPEN_MODES: &[&str] = &["drawer", "float"];
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
/// file, and a form applies itself a moment after the last edit — its Save button is the same write without the
/// wait (see [`live_apply`]). Both go through [`Config::save_section`] (format-preserving), which the running
/// shell hot-reloads and applies live; Revert (in the header) puts the file back to how it was when the window
/// opened.
pub fn settings_panel() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let path = Arc::new(Config::default_path());
    let config = Arc::new(Config::load_or_default(&path));
    crate::shared::services::locale::attach(config.language());

    // The selection and the query are the whole state of the application, and they belong to the *surface*
    // rather than to this build of it: an edit made from another window rebuilds this one, and a settings
    // application that jumped back to its first page every time the config changed would be unusable.
    let selected = kept("settings.page", || signal(0usize));
    let query = kept("settings.query", || signal(String::new()));
    // Bumped when the file stops being what the forms are showing — which is Revert, and only Revert. A form
    // applying itself writes what it already holds, and re-seeding *that* is how the field being typed into
    // loses its caret.
    let reseed = kept("settings.reseed", || signal(0u64));
    // What Revert restores: the file as it was when the *user* opened this window, not as it was a reload ago.
    OPENED_WITH.with(|slot| {
        let mut slot = slot.borrow_mut();
        if slot.is_none() {
            *slot = std::fs::read_to_string(path.as_path()).ok();
        }
    });

    let body = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(NAV_GAP)
            .width(SizeDimension::Percent(1.0)),
        vec![
            nav_pane(selected.clone(), query.read_only(), theme)?,
            page_stack(
                selected.read_only(),
                query.read_only(),
                reseed.read_only(),
                config,
                Arc::clone(&path),
                theme,
            )?,
        ],
    )?;

    let panel = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(16.0)
            .width(SizeDimension::Percent(1.0)),
        vec![header(query, reseed, path, theme)?, Box::new(body)],
    )?;
    Ok(Box::new(panel))
}

/// Forgets the Revert snapshot. Called when the panel is closed for real, so the next window reverts to the
/// file as *it* found it rather than to something a previous session opened against.
pub fn forget_panel_state() {
    OPENED_WITH.with(|slot| *slot.borrow_mut() = None);
}

/// The title, the search box that reaches every page, and Revert.
fn header(
    query: RwSignal<String>,
    reseed: RwSignal<u64>,
    path: Arc<PathBuf>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
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

    // Not a `save_button`: that one now records the form it belongs to, and Revert belongs to no form.
    let revert_ink = theme.red;
    let revert = StyledContainer::new(
        LayoutStyle::new()
            .padding_horizontal(12.0)
            .padding_vertical(6.0)
            .flex_shrink(0.0)
            .justify_content(JustifyContent::CENTER),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(Text::auto(
            || telar::t!("settings.revert"),
            LayoutStyle::new(),
            move || {
                theme
                    .text_style(FontRole::Caption, revert_ink)
                    .with_weight(700)
            },
        )?)],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || {
        revert_to_opened(path.as_path());
        // Straight away rather than waiting for the reload the write triggers: Revert is the one moment the
        // forms on screen are known to be wrong, and it is the user asking to see the file instead.
        reseed.set(reseed.peek() + 1);
    });

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(12.0)
            .width(SizeDimension::Percent(1.0)),
        vec![Box::new(title), Box::new(boxed), Box::new(revert)],
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
    reseed: telar::ReadSignal<u64>,
    config: Arc<Config>,
    path: Arc<PathBuf>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let height = config.settings_page_height();
    // The nav is outside this scroll area on purpose: a nav pane that scrolls away with the page it selects is
    // a list of links you have to scroll back up to use.
    let scroll = telar::LayoutScrollArea::new_kept(
        "settings.scroll",
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            // `min_width(0)` against flexbox's `auto` default: a form's rows are `width: 100%` of whatever they
            // are given, and a flex item that may not shrink below its content asks for the widest row it has,
            // which is how the page area ends up wider than the surface it is in.
            .min_width(0.0)
            .height(height),
        move |viewport| {
            // A page is *replaced*, not resized: three screens down the Appearance page is not a place to be
            // dropped into Network, and neither is three screens down the forms a search has just narrowed
            // away. The scroll area puts a too-short page back in range on its own; only this knows that what
            // is in the viewport is now a different thing rather than the same thing resized.
            //
            // Not on the first run, which is the effect being seeded rather than the user choosing a page —
            // and on a rebuild that seeding run is exactly what would throw away the position being kept.
            let (page, search) = (selected.clone(), query.clone());
            let seeded = std::cell::Cell::new(false);
            let follow_page = telar::effect(move || {
                page.get();
                search.get();
                if seeded.replace(true) {
                    viewport.scroll_to_top();
                }
            });
            let page_area = build_page_area(selected, query, reseed, config, path, theme)?;
            crate::shared::reactive::keeping(page_area, follow_page)
        },
    )?;
    Ok(Box::new(scroll))
}

/// The forms themselves: the sections the current page and search leave visible, each seeded from the file.
fn build_page_area(
    selected: telar::ReadSignal<usize>,
    query: telar::ReadSignal<String>,
    reseed: telar::ReadSignal<u64>,
    config: Arc<Config>,
    path: Arc<PathBuf>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    // The snapshot the window opened with decides how tall the page area is, and nothing else: every form is
    // seeded from the file at the moment it is *built*, so a form rebuilt is a form re-seeded.
    let (_opened_with, path) = (config, path);
    let source = move || {
        // All read out first: `visible` translates labels, which reads the locale signal, and a nested read
        // inside another signal's borrow is the re-entrant panic that only fires when the widget is built.
        let index = selected.get();
        let text = query.get();
        let at = reseed.get();
        pages::visible(index, &text)
            .into_iter()
            .map(|section| (text.clone(), at, section))
            .collect()
    };
    let build = move |(_, _, section): (String, u64, &'static pages::Section)| {
        let config = Arc::new(Config::load_or_default(path.as_path()));
        (section.build)(&config, &path, theme)
    };
    Ok(Box::new(ReactiveList::with_style(
        LayoutStyle::new()
            .flex_column()
            .gap(20.0)
            .width(SizeDimension::Percent(1.0)),
        source,
        // Keyed on the query and the re-seed as well as the form: narrowing changes which forms are here, and
        // Revert changes what they should be showing. Anything not in the key is a form the user may be
        // typing into, which must survive its own applied changes.
        |(query, at, section): &(String, u64, &'static pages::Section)| {
            (query.clone(), *at, section.label)
        },
        build,
    )?) as Box<dyn LayoutItem>)
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

/// The palette tokens the preview strip shows, in the order they read as a design rather than as a list: the
/// three surfaces the shell is built out of, the two inks over them, then the hues.
const PREVIEW_TOKENS: &[&str] = &[
    "base", "surface", "overlay", "text", "subtle", "accent", "red", "orange", "yellow", "green",
    "cyan", "blue", "teal", "purple",
];

/// What `[theme] accent` accepts, in the order [`NordTheme::accent_by_name`] resolves them. `""` is the
/// palette's own accent, which is the value a config that never set one carries.
const ACCENT_NAMES: &[&str] = &[
    "", "blue", "cyan", "teal", "red", "orange", "yellow", "green", "purple",
];

const SWATCH: f32 = 22.0;
const SWATCH_RADIUS: f32 = 6.0;
const TILE_WIDTH: f32 = 76.0;
const TILE_HEIGHT: f32 = 40.0;

/// A palette a control draws from and re-reads: the pending `[theme]` selection resolved through
/// [`Config::theme_with`], so a swatch shows the theme being chosen rather than the one being worn.
type Palette = Rc<dyn Fn() -> NordTheme>;

/// Resolves the page's unsaved `[theme]` selection into a palette, on every read.
///
/// Not a [`Live`](crate::shared::reactive::Live): that is a `Memo`, which needs its value to be `PartialEq` to
/// know whether it moved, and a palette is twenty-two colours and a font table. A closure re-resolving is a
/// match and a struct copy — cheaper than the comparison would be.
fn pending_palette(
    config: &Config,
    name: telar::ReadSignal<String>,
    mode: telar::ReadSignal<String>,
    accent: telar::ReadSignal<String>,
) -> Palette {
    let base = Arc::new(config.clone());
    let saved = config.theme.clone();
    Rc::new(move || {
        // Read out first: each is a separate signal, and `theme_with` is not something to run inside one's borrow.
        let (name, mode, accent) = (name.get(), mode.get(), accent.get());
        base.theme_with(&ThemeConfig {
            name,
            mode,
            accent,
            ..saved.clone()
        })
    })
}

/// The palette as fourteen swatches. The one control on this page that is not a field: a scheme is a thing you
/// look at, and `accent = "cyan"` in a text box is a name for a colour rather than the colour.
fn palette_preview(palette: Palette, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut swatches: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(PREVIEW_TOKENS.len());
    for token in PREVIEW_TOKENS {
        let palette = palette.clone();
        swatches.push(Box::new(StyledContainer::new(
            LayoutStyle::new().width(SWATCH).height(SWATCH),
            move |_r| {
                RectStyle::filled(palette().token(token), SWATCH_RADIUS)
                    .with_stroke(telar::Stroke::new(theme.overlay, 1.0))
            },
            vec![],
        )?));
    }
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .flex_grow(1.0)
            .min_width(0.0),
        swatches,
    )?;
    labelled(|| telar::t!("settings.field.palette"), Box::new(row), theme)
}

/// One tile per selectable theme, each painted in its own colours: the surface it would give the shell, the ink
/// it would write with, and its accent. The tile a cycle button replaces — ten presses to see ten palettes is
/// the control this page had, and the reason K2 existed.
fn theme_swatches(
    name: RwSignal<String>,
    mode: telar::ReadSignal<String>,
    config: Config,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let config = Arc::new(config);
    let mut tiles: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(theme_options().len());
    for option in theme_options() {
        tiles.push(theme_tile(
            option,
            name.clone(),
            mode.clone(),
            &config,
            theme,
        )?);
    }
    let grid = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .flex_grow(1.0)
            .min_width(0.0),
        tiles,
    )?;
    labelled(|| telar::t!("settings.field.name"), Box::new(grid), theme)
}

fn theme_tile(
    option: &'static str,
    name: RwSignal<String>,
    mode: telar::ReadSignal<String>,
    config: &Arc<Config>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let saved = config.theme.clone();
    let config = Arc::clone(config);
    // Resolved with the page's *pending* mode, so switching to light repaints every tile rather than showing
    // ten dark palettes above a mode the user has already changed.
    let swatch_of = move || {
        let mode = mode.get();
        config.theme_with(&ThemeConfig {
            name: option.to_string(),
            mode,
            ..saved.clone()
        })
    };

    let ink = swatch_of.clone();
    let label = Text::auto(
        move || option.to_string(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, ink().text),
    )?;
    let dot_of = swatch_of.clone();
    let dot = StyledContainer::new(
        LayoutStyle::new().width(10.0).height(10.0).flex_shrink(0.0),
        move |_r| RectStyle::filled(dot_of().accent, 5.0),
        vec![],
    )?;
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(5.0),
        vec![Box::new(dot), box_item(label)],
    )?;

    let selected = name.read_only();
    let fill = swatch_of;
    let tile = StyledContainer::new(
        LayoutStyle::new()
            .width(TILE_WIDTH)
            .height(TILE_HEIGHT)
            .padding_horizontal(6.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| {
            // Read both out before painting: `selected` and the palette closure each touch the runtime.
            let chosen = selected.get() == option;
            let palette = fill();
            let border = if chosen { theme.accent } else { theme.overlay };
            RectStyle::filled(palette.surface, SWATCH_RADIUS)
                .with_stroke(telar::Stroke::new(border, if chosen { 2.0 } else { 1.0 }))
        },
        vec![Box::new(row)],
    )?
    .on_press(move || name.set(option.to_string()));
    Ok(Box::new(tile))
}

/// The accents `[theme] accent` accepts, each drawn in the pending palette's own version of that hue — so
/// "cyan" under rosé-pine is rosé-pine's cyan, which is the whole point of naming a hue rather than a hex.
fn accent_swatches(
    accent: RwSignal<String>,
    palette: Palette,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut swatches: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(ACCENT_NAMES.len());
    for option in ACCENT_NAMES {
        let selected = accent.read_only();
        let palette = palette.clone();
        let set = accent.clone();
        swatches.push(Box::new(
            StyledContainer::new(
                LayoutStyle::new().width(SWATCH).height(SWATCH),
                move |_r| {
                    let chosen = selected.get() == *option;
                    let colour = palette().accent_by_name(option);
                    let border = if chosen { theme.text } else { theme.overlay };
                    RectStyle::filled(colour, SWATCH_RADIUS)
                        .with_stroke(telar::Stroke::new(border, if chosen { 2.0 } else { 1.0 }))
                },
                vec![],
            )?
            .on_press(move || set.set(option.to_string())),
        ));
    }
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .flex_grow(1.0)
            .min_width(0.0),
        swatches,
    )?;
    labelled(|| telar::t!("settings.field.accent"), Box::new(row), theme)
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

    // What the pickers below and the preview above them all read: the palette the *pending* selection resolves
    // to, not the one the shell is currently wearing. A swatch row showing the saved theme while the user is
    // choosing another one is a preview of the wrong thing.
    let pending = pending_palette(
        config,
        name.read_only(),
        mode.read_only(),
        accent.read_only(),
    );

    let rows = vec![
        palette_preview(pending.clone(), theme)?,
        theme_swatches(name.clone(), mode.read_only(), config.clone(), theme)?,
        accent_swatches(accent.clone(), pending, theme)?,
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
    persistent: RwSignal<bool>,
    show_on_hover: RwSignal<bool>,
    peek: RwSignal<String>,
    zones: ZoneEditor,
}

fn bar_signals(bar: &BarConfig) -> BarSignals {
    BarSignals {
        size: signal(bar.size.to_string()),
        persistent: signal(bar.persistent),
        show_on_hover: signal(bar.show_on_hover),
        peek: signal(bar.peek.to_string()),
        zones: ZoneEditor::new(bar),
    }
}

/// K3: one bar's three zones, edited as draggable module pills.
///
/// What this replaces is three comma-separated text fields of desktop ids — a control that required knowing
/// every module's spelling, gave no way to see what was available, and turned "put the clock on the other end"
/// into two careful edits. A pill can be dragged anywhere in any of the three zones, dropped to reorder, and
/// dismissed with its own ✕; the palette underneath is every module the shell registers.
///
/// The entries are carried whole rather than by id, which is what keeps `{ id = "clock", accent = "red" }`
/// intact across a reorder — the thing the CSV field had to reconstruct by claiming entries by name.
#[derive(Clone)]
struct ZoneEditor {
    zones: [RwSignal<Vec<ModuleEntry>>; 3],
    /// Where each pill and each zone row was laid out. A drop is resolved against the pointer's actual
    /// position, so dragging a pill onto another zone's *empty* space works as well as onto a pill in it.
    rects: PillRects,
    /// Which zone the palette adds to, so pressing a module is one press rather than a press and a drag.
    target: RwSignal<usize>,
}

/// Every pill's laid-out box, keyed by `(zone, index)` — and by `(zone, ZONE_ROW)` for a zone's own row.
type PillRects =
    Rc<std::cell::RefCell<std::collections::HashMap<(usize, usize), telar::ReadSignal<Rect>>>>;

/// The key a zone row registers its own rect under — past any pill index it could ever hold.
const ZONE_ROW: usize = usize::MAX;

impl ZoneEditor {
    fn new(bar: &BarConfig) -> Self {
        Self {
            zones: [
                signal(bar.start.clone()),
                signal(bar.center.clone()),
                signal(bar.end.clone()),
            ],
            rects: Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            target: signal(0usize),
        }
    }

    fn entries(&self, zone: usize) -> Vec<ModuleEntry> {
        self.zones[zone].peek()
    }

    fn append(&self, zone: usize, entry: ModuleEntry) {
        let mut entries = self.zones[zone].peek();
        entries.push(entry);
        self.zones[zone].set(entries);
    }

    fn remove(&self, zone: usize, index: usize) {
        let mut entries = self.zones[zone].peek();
        if index < entries.len() {
            entries.remove(index);
            self.zones[zone].set(entries);
        }
    }

    /// Moves the pill at `(from_zone, from_index)` to `to_index` of `to_zone`.
    fn move_entry(&self, from: (usize, usize), to: (usize, usize)) {
        let mut source = self.zones[from.0].peek();
        if from.1 >= source.len() {
            return;
        }
        let entry = source.remove(from.1);
        if from.0 == to.0 {
            let index = to.1.min(source.len());
            source.insert(index, entry);
            self.zones[from.0].set(source);
            return;
        }
        let mut target = self.zones[to.0].peek();
        let index = to.1.min(target.len());
        target.insert(index, entry);
        self.zones[from.0].set(source);
        self.zones[to.0].set(target);
    }

    /// Where a drop at `point` (surface coordinates) lands: the pill under it, else the zone row it is over.
    fn drop_target(&self, point: (f32, f32)) -> Option<(usize, usize)> {
        // Read the three lengths once, and use them to ignore the rects of pills that are no longer there. A
        // zone that went from three pills to two leaves `(zone, 2)` in the map pointing at a destroyed
        // widget's rect, and nothing about that entry says so — it would go on winning drops over the area it
        // used to occupy, ahead of whichever live pill the map happened to be walked to second.
        let lengths = [
            self.zones[0].peek().len(),
            self.zones[1].peek().len(),
            self.zones[2].peek().len(),
        ];
        let rects = self.rects.borrow();
        let mut row: Option<(usize, usize)> = None;
        for ((zone, index), rect) in rects.iter() {
            if *index != ZONE_ROW && *index >= lengths[*zone] {
                continue;
            }
            let rect = rect.get();
            if !rect.contains(point.0, point.1) {
                continue;
            }
            if *index == ZONE_ROW {
                // Held rather than returned: a pill's own rect is inside its row's, and the pill is the more
                // precise answer whichever order the map happens to be walked in.
                row = Some((*zone, lengths[*zone]));
                continue;
            }
            // The half of the pill the pointer is on decides which side of it the dragged one lands.
            let after = point.0 > rect.x + rect.width / 2.0;
            return Some((*zone, index + usize::from(after)));
        }
        row
    }

    fn track(&self, zone: usize, index: usize, rect: telar::ReadSignal<Rect>) {
        self.rects.borrow_mut().insert((zone, index), rect);
    }
}

fn bar_rows(
    label: impl Fn() -> String + 'static,
    s: &BarSignals,
    theme: NordTheme,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let mut rows = vec![
        subheader(label, theme)?,
        text_field(
            || telar::t!("settings.field.size"),
            s.size.clone(),
            "34",
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.persistent"),
            s.persistent.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.show_on_hover"),
            s.show_on_hover.clone(),
            theme,
        )?,
        text_field(
            || telar::t!("settings.field.peek"),
            s.peek.clone(),
            "2",
            theme,
        )?,
    ];
    for (zone, label) in ZONE_LABELS.iter().enumerate() {
        rows.push(zone_row(label, zone, &s.zones, theme)?);
    }
    rows.push(module_palette(&s.zones, theme)?);
    Ok(rows)
}

const ZONE_LABELS: [&str; 3] = ["start", "center", "end"];
const PILL_RADIUS: f32 = 8.0;

/// One zone: its name, and the pills in it.
fn zone_row(
    label: &'static str,
    zone: usize,
    editor: &ZoneEditor,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = editor.zones[zone].read_only();
    let list_editor = editor.clone();
    let pills = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .flex_grow(1.0)
            .min_width(0.0),
        move || source.get().into_iter().enumerate().collect(),
        // Keyed on the position *and* the id: a reorder has to redraw both pills that swapped, and a list
        // keyed on the id alone would leave them where they were.
        |(index, entry): &(usize, ModuleEntry)| format!("{index}|{}", entry.id),
        move |(index, entry): (usize, ModuleEntry)| {
            module_pill(zone, index, entry, list_editor.clone(), theme)
        },
    )?;

    let selected = editor.target.read_only();
    let choose = editor.target.clone();
    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(8.0)
            .padding_all(6.0)
            .min_height(theme.font(FontRole::Body) * 2.4)
            .width(SizeDimension::Percent(1.0)),
        move |_r| {
            let fill = if selected.get() == zone {
                theme.overlay
            } else {
                theme.base
            };
            RectStyle::filled(fill, PILL_RADIUS)
        },
        vec![
            box_item(Text::auto(
                move || pages::label("settings.field", label),
                LayoutStyle::new().width(90.0).flex_shrink(0.0),
                move || theme.text_style(FontRole::Caption, theme.subtle),
            )?),
            Box::new(pills),
        ],
    )?
    .on_press(move || choose.set(zone));
    let rect = telar::track_layout(row.layout_node())
        .expect("a container registers its rect")
        .read_only();
    editor.track(zone, ZONE_ROW, rect);
    Ok(Box::new(row))
}

/// One module on a bar: its id, a ✕ that takes it off, and the drag that moves it.
fn module_pill(
    zone: usize,
    index: usize,
    entry: ModuleEntry,
    editor: ZoneEditor,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = entry.id.clone();
    let label = Text::auto(
        move || id.clone(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.text),
    )?;
    let remove = {
        let editor = editor.clone();
        toggle_pill("x", false, theme.red, theme, move || {
            editor.remove(zone, index)
        })?
    };

    let pill = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(4.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0)
            .flex_shrink(0.0),
        move |_r| RectStyle::filled(theme.surface, PILL_RADIUS),
        vec![box_item(label), remove],
    )?;
    let rect = telar::track_layout(pill.layout_node())
        .expect("a container registers its rect")
        .read_only();
    editor.track(zone, index, rect.clone());

    let dropped = editor.clone();
    let pill = pill.on_drag_end(move |x, y| {
        // The gesture reports where the pointer is *inside the pill*; the drop is about where that is on the
        // surface, so the pill's own origin has to be added back before anything can be hit-tested.
        let origin = rect.peek();
        let point = (origin.x + x, origin.y + y);
        if let Some(target) = dropped.drop_target(point) {
            dropped.move_entry((zone, index), target);
        }
    });
    Ok(Box::new(pill))
}

/// Every module the shell registers, as something to press. The add half of K3: the CSV field it replaces
/// required knowing a module existed before it could be typed.
fn module_palette(
    editor: &ZoneEditor,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let registry = crate::shared::module::default_registry();
    let mut chips: Vec<Box<dyn LayoutItem>> = Vec::new();
    for id in registry.ids() {
        let editor = editor.clone();
        let label = id.clone();
        let text = Text::auto(
            move || label.clone(),
            LayoutStyle::new(),
            move || theme.text_style(FontRole::Caption, theme.subtle),
        )?;
        chips.push(Box::new(
            StyledContainer::new(
                LayoutStyle::new()
                    .padding_horizontal(8.0)
                    .padding_vertical(4.0)
                    .flex_shrink(0.0),
                move |_r| RectStyle::filled(theme.base, PILL_RADIUS),
                vec![box_item(text)],
            )?
            .on_hover_style(move |_r| RectStyle::filled(theme.overlay, PILL_RADIUS))
            .on_press(move || {
                let zone = editor.target.peek();
                editor.append(zone, ModuleEntry::bare(id.clone()));
            }),
        ));
    }
    let grid = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(6.0)
            .flex_grow(1.0)
            .min_width(0.0),
        chips,
    )?;
    labelled(
        || telar::t!("settings.field.add_module"),
        Box::new(grid),
        theme,
    )
}

fn bar_from(s: &BarSignals, base: &BarConfig) -> BarConfig {
    BarConfig {
        size: parse_u32(&s.size.peek(), base.size),
        start: s.zones.entries(0),
        center: s.zones.entries(1),
        end: s.zones.entries(2),
        shape: base.shape,
        persistent: s.persistent.peek(),
        show_on_hover: s.show_on_hover.peek(),
        peek: parse_u32(&s.peek.peek(), base.peek),
    }
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
    rows.extend(bar_rows(
        || telar::t!("settings.subheader.top"),
        &top,
        theme,
    )?);
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.workspaces"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.marquee"),
            marquee.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        text_field(
            || telar::t!("settings.field.gain"),
            gain.clone(),
            "1",
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.visualiser"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.brightness"),
        rows,
        save,
        theme,
    )
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
        text_field(
            || telar::t!("settings.field.warn"),
            warn.clone(),
            "70",
            theme,
        )?,
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
    ];

    let base = l.clone();
    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.launcher"),
        theme,
        move || {
            // Merged into the file as it is now, because the applications page below owns the other half of
            // this same `[launcher]` table. A snapshot taken when the form was built would quietly revert a
            // favourite marked since — see `persist_with`.
            persist_with(&path, "launcher", |current| LauncherConfig {
                width: parse_u32(&width.peek(), base.width),
                height: parse_u32(&height.peek(), base.height),
                radius: base.radius,
                max_results: parse_u32(&max_results.peek(), base.max_results),
                fuzzy: fuzzy.peek(),
                calculator: calculator.peek(),
                qalc: qalc.peek(),
                enable_dangerous_actions: dangerous.peek(),
                // Not this form's to edit: the app list owns the first three, and `actions` is a list of
                // tables that stays hand-edited in the TOML.
                favourites: current.launcher.favourites.clone(),
                hidden: current.launcher.hidden.clone(),
                icons: current.launcher.icons.clone(),
                actions: current.launcher.actions.clone(),
            });
        },
    )?;
    section(|| telar::t!("settings.section.launcher"), rows, save, theme)
}

/// K13, first half: the maps whose keys are enumerable.
///
/// `background.monitors` came off this list with J9 by the route that generalises worst and works best — its
/// keys are not free text, they are the monitors that exist, so the panel names them instead of asking the
/// user to type one. Three of the four remaining maps take the same route, each with its own answer to "what
/// are the keys":
///
/// - `[theme.colors]` — the palette's own token names, which are fixed and shipped ([`THEME_TOKENS`]).
/// - `[modules.<id>]` — every module registered in the shell, so a chip can be restyled before it is on a bar.
/// - `[media.aliases]` — the players that have been seen on the bus, plus whatever the config already names.
///   The one genuinely open set here, handled exactly as `monitor_keys` handles a monitor left at the office:
///   listing only what is running now would delete an alias for a player that happens to be closed.
///
/// A row per key with the *resolved* value as its placeholder, so an empty field reads as "whatever the theme
/// says" rather than as a value that got lost.
fn theme_colors_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let resolved = config.resolve_theme();
    let fields: Vec<(&'static str, RwSignal<String>)> = THEME_TOKENS
        .iter()
        .map(|token| {
            (
                *token,
                signal(config.theme.colors.get(*token).cloned().unwrap_or_default()),
            )
        })
        .collect();

    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(fields.len());
    for (token, value) in fields.iter().map(|(t, v)| (*t, v.clone())) {
        rows.push(text_field(
            move || token.to_string(),
            value,
            &crate::shared::theme::hex(resolved.token(token)),
            theme,
        )?);
    }

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.theme_colors"),
        theme,
        move || {
            let colors: std::collections::HashMap<String, String> = fields
                .iter()
                .filter_map(|(token, value)| {
                    opt_string(&value.peek()).map(|hex| (token.to_string(), hex))
                })
                .collect();
            // Only this form's key: `theme_section` above owns every other one in `[theme]`.
            persist_with(&path, "theme", |current| ThemeConfig {
                colors,
                ..current.theme.clone()
            });
        },
    )?;
    section(
        || telar::t!("settings.section.theme_colors"),
        rows,
        save,
        theme,
    )
}

/// Every media player a `[media.aliases]` row should exist for: the ones seen on the bus this session, plus
/// any the config already renames. Both halves matter, for the reason `monitor_keys` documents.
fn player_keys(configured: &std::collections::HashMap<String, String>) -> Vec<String> {
    let mut keys: Vec<String> = configured.keys().cloned().collect();
    if let Some(player) = crate::shared::services::mpris::current()
        && !player.identity.trim().is_empty()
        && !keys.contains(&player.identity)
    {
        keys.push(player.identity.clone());
    }
    keys.sort_unstable();
    keys
}

fn media_aliases_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let keys = player_keys(&config.media.aliases);
    let fields: Vec<(String, RwSignal<String>)> = keys
        .iter()
        .map(|key| {
            (
                key.clone(),
                signal(config.media.aliases.get(key).cloned().unwrap_or_default()),
            )
        })
        .collect();

    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(fields.len().max(1));
    if fields.is_empty() {
        rows.push(box_item(Text::auto(
            || telar::t!("settings.media.no_players"),
            LayoutStyle::new(),
            move || theme.text_style(FontRole::Caption, theme.muted),
        )?));
    }
    for (key, value) in &fields {
        let label = key.clone();
        rows.push(text_field(
            move || label.clone(),
            value.clone(),
            key,
            theme,
        )?);
    }

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.media_aliases"),
        theme,
        move || {
            let aliases: std::collections::HashMap<String, String> = fields
                .iter()
                .filter_map(|(key, value)| {
                    opt_string(&value.peek()).map(|alias| (key.clone(), alias))
                })
                .collect();
            persist_with(&path, "media", |current| MediaConfig {
                aliases,
                ..current.media.clone()
            });
        },
    )?;
    section(
        || telar::t!("settings.section.media_aliases"),
        rows,
        save,
        theme,
    )
}

/// `[modules.<id>]`: the per-module presentation overrides.
///
/// Keyed on the registry rather than on what the bars currently use, so a module can be styled before it is
/// placed — the alternative would be a user having to add a chip, save, reopen the page and only then be able
/// to give it an accent.
fn module_overrides_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut ids: Vec<String> = crate::shared::module::default_registry().ids();
    for configured in config.modules.keys() {
        if !ids.contains(configured) {
            ids.push(configured.clone());
        }
    }
    ids.sort_unstable();

    struct Fields {
        id: String,
        variant: RwSignal<String>,
        accent: RwSignal<String>,
        open: RwSignal<String>,
        width: RwSignal<String>,
        height: RwSignal<String>,
    }

    let mut fields: Vec<Fields> = Vec::with_capacity(ids.len());
    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(ids.len() * 6);
    for id in ids {
        let existing = config.modules.get(&id).cloned().unwrap_or_default();
        let entry = Fields {
            variant: signal(variant_str(existing.variant).to_string()),
            accent: signal(existing.accent.clone().unwrap_or_default()),
            open: signal(open_mode_str(existing.open).to_string()),
            width: signal(opt_num(existing.width)),
            height: signal(opt_num(existing.height)),
            id: id.clone(),
        };
        rows.push(subheader(move || id.clone(), theme)?);
        rows.push(enum_field(
            || telar::t!("settings.field.variant_style"),
            entry.variant.clone(),
            VARIANT_STYLES,
            theme,
        )?);
        rows.push(text_field(
            || telar::t!("settings.field.accent"),
            entry.accent.clone(),
            "(theme)",
            theme,
        )?);
        rows.push(enum_field(
            || telar::t!("settings.field.open"),
            entry.open.clone(),
            OPEN_MODES,
            theme,
        )?);
        rows.push(text_field(
            || telar::t!("settings.field.width"),
            entry.width.clone(),
            "(panels)",
            theme,
        )?);
        rows.push(text_field(
            || telar::t!("settings.field.height"),
            entry.height.clone(),
            "(panels)",
            theme,
        )?);
        fields.push(entry);
    }

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.modules"),
        theme,
        move || {
            let overrides: std::collections::HashMap<String, ModuleOverride> = fields
                .iter()
                .filter_map(|entry| {
                    let value = ModuleOverride {
                        variant: parse_variant(&entry.variant.peek()),
                        accent: opt_string(&entry.accent.peek()),
                        open: parse_open_mode(&entry.open.peek()),
                        width: opt_u32(&entry.width.peek()),
                        height: opt_u32(&entry.height.peek()),
                    };
                    // A module left entirely at its defaults gets no table at all, so the file keeps only the
                    // overrides a user actually made rather than thirty empty sections.
                    if is_default_override(&value) {
                        None
                    } else {
                        Some((entry.id.clone(), value))
                    }
                })
                .collect();
            persist(&path, "modules", &overrides);
        },
    )?;
    section(|| telar::t!("settings.section.modules"), rows, save, theme)
}

fn is_default_override(value: &ModuleOverride) -> bool {
    value.variant == Variant::Default
        && value.accent.is_none()
        && value.open == OpenMode::default()
        && value.width.is_none()
        && value.height.is_none()
}

/// K13, second half: a `[[list]]` of config tables, edited as rows with an Add button and a remove control on
/// each — `[[battery.warn_levels]]` and `[[idle.stages]]`.
///
/// Two pieces of state, deliberately. The *order* is a signal, so adding or removing a row redraws the list.
/// The *values* are a plain map behind an `Rc`, because a row's fields change on every keystroke and a signal
/// there would rebuild the row being typed into — the trap every keyed list in this shell documents.
///
/// Rows are keyed on a synthetic id rather than on their index. An index-keyed list reuses row 1's widgets for
/// what used to be row 2 when row 1 is deleted, because the key it reconciles on did not change: the user
/// deletes one warning and the form quietly shows them another one's values under the first one's heading.
struct TableList<T> {
    order: RwSignal<Vec<u64>>,
    values: Rc<std::cell::RefCell<std::collections::HashMap<u64, T>>>,
    next: Rc<std::cell::Cell<u64>>,
}

impl<T: Clone + 'static> TableList<T> {
    fn new(entries: Vec<T>) -> Self {
        let list = Self {
            order: signal(Vec::new()),
            values: Rc::new(std::cell::RefCell::new(std::collections::HashMap::new())),
            next: Rc::new(std::cell::Cell::new(0)),
        };
        for entry in entries {
            list.add(entry);
        }
        list
    }

    fn clone_handle(&self) -> Self {
        Self {
            order: self.order.clone(),
            values: Rc::clone(&self.values),
            next: Rc::clone(&self.next),
        }
    }

    fn add(&self, entry: T) {
        let id = self.next.get();
        self.next.set(id + 1);
        self.values.borrow_mut().insert(id, entry);
        let mut order = self.order.peek();
        order.push(id);
        self.order.set(order);
    }

    fn remove(&self, id: u64) {
        self.values.borrow_mut().remove(&id);
        let order: Vec<u64> = self
            .order
            .peek()
            .into_iter()
            .filter(|existing| *existing != id)
            .collect();
        self.order.set(order);
    }

    fn edit(&self, id: u64, apply: impl FnOnce(&mut T)) {
        if let Some(entry) = self.values.borrow_mut().get_mut(&id) {
            apply(entry);
        }
    }

    fn get(&self, id: u64) -> Option<T> {
        self.values.borrow().get(&id).cloned()
    }

    /// The list as the config carries it, in the order the rows are drawn in.
    fn collect(&self) -> Vec<T> {
        let values = self.values.borrow();
        self.order
            .peek()
            .into_iter()
            .filter_map(|id| values.get(&id).cloned())
            .collect()
    }

    fn view(
        &self,
        row: impl Fn(u64) -> Result<Box<dyn LayoutItem>, LayoutError> + 'static,
    ) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let order = self.order.read_only();
        Ok(Box::new(ReactiveList::with_gap(
            move || order.get(),
            |id: &u64| id.to_string(),
            move |id: u64| row(id),
            10.0,
        )?))
    }
}

/// A text field bound to one field of a [`TableList`] entry, writing back on every keystroke.
///
/// Returns the effect for the row to hold: a bare `effect(…)` statement runs once and stops, which looks like
/// a field that accepts the first character and then ignores the rest of the word.
fn bound_field<T: Clone + 'static>(
    label: impl Fn() -> String + 'static,
    list: &TableList<T>,
    id: u64,
    initial: String,
    placeholder: &str,
    theme: NordTheme,
    apply: impl Fn(&mut T, &str) + 'static,
) -> Result<(Box<dyn LayoutItem>, telar::Effect), LayoutError> {
    let value = signal(initial);
    let watched = value.read_only();
    let list = list.clone_handle();
    let sync = telar::effect(move || {
        let text = watched.get();
        list.edit(id, |entry| apply(entry, &text));
    });
    Ok((text_field(label, value, placeholder, theme)?, sync))
}

/// [`bound_field`] for a switch.
fn bound_toggle<T: Clone + 'static>(
    label: impl Fn() -> String + 'static,
    list: &TableList<T>,
    id: u64,
    initial: bool,
    theme: NordTheme,
    apply: impl Fn(&mut T, bool) + 'static,
) -> Result<(Box<dyn LayoutItem>, telar::Effect), LayoutError> {
    let value = signal(initial);
    let watched = value.read_only();
    let list = list.clone_handle();
    let sync = telar::effect(move || {
        let on = watched.get();
        list.edit(id, |entry| apply(entry, on));
    });
    Ok((toggle_field(label, value, theme)?, sync))
}

/// The `[[battery.warn_levels]]` editor: one card per warning, with Add and Remove.
fn battery_warnings_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = Rc::new(TableList::new(config.battery.warn_levels.clone()));

    let rows = {
        let list = Rc::clone(&list);
        let handle = Rc::clone(&list);
        handle.view(move |id| {
            let Some(warning) = list.get(id) else {
                return Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?));
            };
            let (level, a) = bound_field(
                || telar::t!("settings.field.level"),
                &list,
                id,
                warning.level.to_string(),
                "20",
                theme,
                |entry: &mut BatteryWarning, text| entry.level = parse_i32(text, entry.level),
            )?;
            let (title, b) = bound_field(
                || telar::t!("settings.field.title"),
                &list,
                id,
                warning.title.clone(),
                "(default)",
                theme,
                |entry: &mut BatteryWarning, text| entry.title = text.to_string(),
            )?;
            let (message, c) = bound_field(
                || telar::t!("settings.field.message"),
                &list,
                id,
                warning.message.clone(),
                "(default)",
                theme,
                |entry: &mut BatteryWarning, text| entry.message = text.to_string(),
            )?;
            let (icon, d) = bound_field(
                || telar::t!("settings.field.icon"),
                &list,
                id,
                warning.icon.clone(),
                "battery-low",
                theme,
                |entry: &mut BatteryWarning, text| entry.icon = text.to_string(),
            )?;
            let (critical, e) = bound_toggle(
                || telar::t!("settings.field.critical_urgency"),
                &list,
                id,
                warning.critical,
                theme,
                |entry: &mut BatteryWarning, on| entry.critical = on,
            )?;
            entry_card(
                vec![level, title, message, icon, critical],
                &list,
                id,
                theme,
                vec![a, b, c, d, e],
            )
        })?
    };

    let add = {
        let list = Rc::clone(&list);
        save_button(
            || telar::t!("settings.list.add"),
            theme,
            move || list.add(BatteryWarning::default()),
        )?
    };

    let path = path.to_path_buf();
    let saved = Rc::clone(&list);
    let save = save_button(
        || telar::t!("settings.save.battery_warnings"),
        theme,
        move || {
            persist_with(&path, "battery", |current| BatteryConfig {
                warn_levels: saved.collect(),
                ..current.battery.clone()
            });
        },
    )?;

    section(
        || telar::t!("settings.section.battery_warnings"),
        vec![rows, add],
        save,
        theme,
    )
}

/// The `[[idle.stages]]` editor. `hyprshell --list` is what the action fields accept; the placeholders name
/// the three a user reaches for.
fn idle_stages_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let list = Rc::new(TableList::new(config.idle.stages.clone()));

    let rows = {
        let list = Rc::clone(&list);
        let handle = Rc::clone(&list);
        handle.view(move |id| {
            let Some(stage) = list.get(id) else {
                return Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?));
            };
            let (timeout, a) = bound_field(
                || telar::t!("settings.field.timeout_seconds"),
                &list,
                id,
                stage.timeout.to_string(),
                "300",
                theme,
                |entry: &mut IdleStage, text| entry.timeout = parse_u64(text, entry.timeout),
            )?;
            let (action, b) = bound_field(
                || telar::t!("settings.field.action"),
                &list,
                id,
                stage.action.clone(),
                "lock on",
                theme,
                |entry: &mut IdleStage, text| entry.action = text.to_string(),
            )?;
            let (return_action, c) = bound_field(
                || telar::t!("settings.field.return_action"),
                &list,
                id,
                stage.return_action.clone(),
                "shell dpms on",
                theme,
                |entry: &mut IdleStage, text| entry.return_action = text.to_string(),
            )?;
            entry_card(
                vec![timeout, action, return_action],
                &list,
                id,
                theme,
                vec![a, b, c],
            )
        })?
    };

    let add = {
        let list = Rc::clone(&list);
        save_button(
            || telar::t!("settings.list.add"),
            theme,
            move || list.add(IdleStage::default()),
        )?
    };

    let path = path.to_path_buf();
    let saved = Rc::clone(&list);
    let save = save_button(
        || telar::t!("settings.save.idle_stages"),
        theme,
        move || {
            persist_with(&path, "idle", |current| IdleConfig {
                stages: saved.collect(),
                ..current.idle.clone()
            });
        },
    )?;

    section(
        || telar::t!("settings.section.idle_stages"),
        vec![rows, add],
        save,
        theme,
    )
}

/// One entry of a [`TableList`]: its fields in a filled card, with the control that deletes it.
fn entry_card<T: Clone + 'static>(
    mut fields: Vec<Box<dyn LayoutItem>>,
    list: &TableList<T>,
    id: u64,
    theme: NordTheme,
    subscriptions: Vec<telar::Effect>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let remove = {
        let list = list.clone_handle();
        toggle_pill("trash-2", false, theme.red, theme, move || list.remove(id))?
    };
    fields.push(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .justify_content(JustifyContent::END)
            .width(SizeDimension::Percent(1.0)),
        vec![remove],
    )?));
    let card = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(6.0)
            .padding_all(10.0)
            .width(SizeDimension::Percent(1.0)),
        move |_r| RectStyle::filled(theme.surface, 8.0),
        fields,
    )?;
    crate::shared::reactive::keeping_all(Box::new(card), subscriptions)
}

/// How wide one wallpaper tile is, and the shape of its picture — landscape, because a wallpaper is a picture
/// of a screen and a square crop of one is unrecognisable.
const WALL_TILE: f32 = 132.0;
const WALL_ASPECT: f32 = 9.0 / 16.0;

/// The same bound, and the same reason, as the launcher's wallpaper grid: `ReactiveList` builds a widget per
/// tile up front, so a library of two thousand would spend the UI thread before the page appeared. The search
/// box is what reaches past it.
const WALL_TILES: usize = 150;

/// K9: the wallpaper library, grouped by the folder each image was found in.
///
/// `[background] image` names one file and `[background.monitors]` names one per screen — both of which the
/// forms below already edit. What neither of them is, is a way to *see* the library, and choosing a picture
/// from a list of paths is choosing it by its file name. Pressing a tile sets it on every screen, which is the
/// rule the wallpaper commands already follow: a mutation with no target named means all of them.
fn wallpaper_browser_section(
    config: &Config,
    _path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let query = signal(String::new());

    let library = signal(crate::shared::services::wallpaper::all());
    let sink = library.clone();
    platform_layershell::watch(
        crate::shared::services::wallpaper::subscribe_library,
        move |entries| sink.set(entries),
    );

    // Which tile reads as the current one. The runtime choice first, then whatever `[background]` resolves to,
    // so a fresh session with nothing chosen at runtime still marks the picture actually on screen.
    let configured = crate::shared::services::wallpaper::current_image(config, None);
    let current = signal(
        crate::shared::services::wallpaper::assignment()
            .global
            .or(configured),
    );
    let current_sink = current.clone();
    platform_layershell::watch(
        crate::shared::services::wallpaper::subscribe,
        move |assignment: crate::shared::services::wallpaper::Assignment| {
            current_sink.set(assignment.global)
        },
    );

    let search = text_field(
        || telar::t!("settings.field.search"),
        query.clone(),
        "sunset",
        theme,
    )?;

    let source_library = library.read_only();
    let source_query = query.read_only();
    let source_current = current.read_only();
    let groups = ReactiveList::with_gap(
        move || {
            let entries = source_library.get();
            let query = source_query.get();
            let current = source_current.get();
            folders(&entries, &query)
                .into_iter()
                .map(|(folder, entries)| WallGroup {
                    key: group_key(&folder, &entries, current.as_deref()),
                    folder,
                    entries,
                })
                .collect()
        },
        |group: &WallGroup| group.key.clone(),
        move |group: WallGroup| wallpaper_group(group, current.read_only(), theme),
        14.0,
    )?;

    let empty_library = library.read_only();
    let empty_query = query.read_only();
    let empty = Text::auto(
        move || {
            let entries = empty_library.get();
            let query = empty_query.get();
            if entries.is_empty() {
                telar::t!("settings.wallpaper.empty")
            } else if folders(&entries, &query).is_empty() {
                telar::t!("settings.wallpaper.no_match")
            } else {
                String::new()
            }
        },
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.muted),
    )?;

    let clear = save_button(
        || telar::t!("settings.wallpaper.clear"),
        theme,
        || crate::shared::services::wallpaper::clear(None),
    )?;

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![
            section_label(|| telar::t!("settings.section.library"), theme)?,
            search,
            box_item(empty),
            Box::new(groups),
            clear,
        ],
    )?))
}

/// One folder's worth of the library, as the browser draws it.
#[derive(Clone, Debug, PartialEq)]
struct WallGroup {
    /// Keyed on the pictures *and* on which of them is current, so choosing one repaints the ring without the
    /// whole library rebuilding under the pointer.
    key: String,
    folder: String,
    entries: Vec<crate::shared::services::wallpaper::Entry>,
}

fn group_key(
    folder: &str,
    entries: &[crate::shared::services::wallpaper::Entry],
    current: Option<&Path>,
) -> String {
    let chosen = entries
        .iter()
        .position(|entry| Some(entry.path.as_path()) == current);
    format!("{folder}|{}|{chosen:?}", entries.len())
}

/// The library narrowed by `query` and grouped by folder, top-level images first and the rest alphabetical —
/// which is the order a file manager would show them in, and the only one a user can predict.
fn folders(
    entries: &[crate::shared::services::wallpaper::Entry],
    query: &str,
) -> Vec<(String, Vec<crate::shared::services::wallpaper::Entry>)> {
    let needle = query.trim().to_lowercase();
    let mut grouped: std::collections::BTreeMap<
        String,
        Vec<crate::shared::services::wallpaper::Entry>,
    > = std::collections::BTreeMap::new();
    let mut shown = 0usize;
    for entry in entries {
        if shown >= WALL_TILES {
            break;
        }
        if !needle.is_empty()
            && !entry.name.to_lowercase().contains(&needle)
            && !entry.folder.to_lowercase().contains(&needle)
        {
            continue;
        }
        grouped
            .entry(entry.folder.clone())
            .or_default()
            .push(entry.clone());
        shown += 1;
    }
    grouped.into_iter().collect()
}

fn wallpaper_group(
    group: WallGroup,
    current: telar::ReadSignal<Option<PathBuf>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut tiles: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(group.entries.len());
    for entry in group.entries {
        tiles.push(wallpaper_tile(entry, current.clone(), theme)?);
    }
    let grid = Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_wrap()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        tiles,
    )?;
    // A folder name at the top level is empty, and a heading reading nothing is a gap the user has to guess at.
    let folder = group.folder.clone();
    let heading = subheader(
        move || {
            if folder.is_empty() {
                telar::t!("settings.wallpaper.top_level")
            } else {
                folder.clone()
            }
        },
        theme,
    )?;
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(6.0)
            .width(SizeDimension::Percent(1.0)),
        vec![heading, Box::new(grid)],
    )?))
}

fn wallpaper_tile(
    entry: crate::shared::services::wallpaper::Entry,
    current: telar::ReadSignal<Option<PathBuf>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let picture_height = (WALL_TILE * WALL_ASPECT).round();
    let picture = crate::shared::thumbnail::view(
        entry.path.clone(),
        WALL_TILE,
        picture_height,
        6.0,
        "image",
        theme,
    )?;

    let name = entry.name.clone();
    let label = Text::auto(
        move || name.clone(),
        LayoutStyle::new().width(SizeDimension::Percent(1.0)),
        move || {
            theme
                .text_style(FontRole::Caption, theme.subtle)
                .with_max_lines(1)
                .with_ellipsis(true)
        },
    )?;

    let path = entry.path.clone();
    let chosen = entry.path.clone();
    let tile = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .gap(4.0)
            .width(WALL_TILE + 8.0)
            .padding_all(4.0)
            .align_items(AlignItems::CENTER),
        move |_r| {
            let is_current = current.get().as_deref() == Some(path.as_path());
            let fill = if is_current { theme.accent } else { theme.base };
            RectStyle::filled(fill, 8.0)
        },
        vec![picture, box_item(label)],
    )?
    .on_hover_style(move |_r| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || crate::shared::services::wallpaper::set(&chosen, None));
    Ok(Box::new(tile))
}

/// How many applications the database page draws at once. `ReactiveList` builds a widget per row up front, so
/// a machine with two thousand entries would spend the UI thread before the page appeared — the same bound,
/// and the same reason, as the launcher's wallpaper grid. The search box is what reaches past it.
const APP_ROWS: usize = 200;

/// K7: the installed applications, as the launcher sees them.
///
/// `[launcher] favourites` and `hidden` are lists of desktop-entry ids, and a user does not know their
/// software by desktop-entry id — the CSV fields these replace asked them to type `org.gnome.Nautilus` from
/// memory. Here the list is the control: every application it found, each with the two switches and the icon
/// override that are the only per-app settings there are.
fn apps_section(
    config: &Config,
    path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let query = signal(String::new());
    let favourites = signal(config.launcher.favourites.clone());
    let hidden = signal(config.launcher.hidden.clone());
    let icons = signal(config.launcher.icons.clone());

    let installed = signal(apps::all());
    let sink = installed.clone();
    platform_layershell::watch(apps::subscribe, move |apps| sink.set(apps));

    let search = text_field(
        || telar::t!("settings.field.search"),
        query.clone(),
        "firefox",
        theme,
    )?;

    let source_apps = installed.read_only();
    let source_query = query.read_only();
    let source_favourites = favourites.read_only();
    let source_hidden = hidden.read_only();
    let list = ReactiveList::with_gap(
        move || {
            // Every signal read out before any of them is mapped: `matching` does no reactive work, but a
            // `with` over one while reading the next is the borrow panic this file keeps documenting.
            let apps = source_apps.get();
            let query = source_query.get();
            let favourites = source_favourites.get();
            let hidden = source_hidden.get();
            matching(&apps, &query)
                .into_iter()
                .map(|app| AppRow {
                    favourite: favourites.contains(&app.id),
                    hidden: hidden.contains(&app.id),
                    app,
                })
                .collect()
        },
        |row: &AppRow| format!("{}|{}|{}", row.app.id, row.favourite, row.hidden),
        {
            let (favourites, hidden, icons) = (favourites.clone(), hidden.clone(), icons.clone());
            move |row: AppRow| {
                app_row(
                    row,
                    favourites.clone(),
                    hidden.clone(),
                    icons.clone(),
                    theme,
                )
            }
        },
        6.0,
    )?;

    let count_apps = installed.read_only();
    let count_query = query.read_only();
    let count = Text::auto(
        move || {
            let apps = count_apps.get();
            let query = count_query.get();
            let shown = matching(&apps, &query).len();
            telar::t!(
                "settings.apps.count",
                shown = shown.to_string(),
                total = apps.len().to_string()
            )
        },
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.muted),
    )?;

    let path = path.to_path_buf();
    let save = save_button(
        || telar::t!("settings.save.apps"),
        theme,
        move || {
            persist_with(&path, "launcher", |current| LauncherConfig {
                favourites: favourites.peek(),
                hidden: hidden.peek(),
                icons: icons.peek(),
                ..current.launcher.clone()
            });
        },
    )?;

    section(
        || telar::t!("settings.section.apps"),
        vec![search, box_item(count), Box::new(list)],
        save,
        theme,
    )
}

/// One application in the database list, and the two switches it carries.
#[derive(Clone, Debug, PartialEq)]
struct AppRow {
    app: App,
    favourite: bool,
    hidden: bool,
}

/// The applications a query narrows to, capped. Sorted by name rather than left in scan order, because the
/// directories are read most-specific-first and a user browsing a list expects an alphabet.
fn matching(apps: &[App], query: &str) -> Vec<App> {
    let needle = query.trim().to_lowercase();
    let mut found: Vec<App> = apps
        .iter()
        .filter(|app| {
            needle.is_empty()
                || app.haystack().to_lowercase().contains(&needle)
                || app.id.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    found.sort_by_key(|app| app.name.to_lowercase());
    found.truncate(APP_ROWS);
    found
}

fn app_row(
    row: AppRow,
    favourites: RwSignal<Vec<String>>,
    hidden: RwSignal<Vec<String>>,
    icons: RwSignal<std::collections::HashMap<String, String>>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id = row.app.id.clone();
    let declared = row.app.icon.clone();
    let override_now = icons.peek().get(&id).cloned().unwrap_or_default();
    let reference = if override_now.trim().is_empty() {
        declared.clone()
    } else {
        override_now.clone()
    };

    let icon = crate::shared::icon::app_icon_view(&reference, 24.0)?.unwrap_or(icon_view(
        || "app-window".to_string(),
        move || theme.muted,
        24.0,
    )?);

    let name = row.app.name.clone();
    let name_text = Text::auto(
        move || name.clone(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Body, theme.text),
    )?;
    let subtitle = id.clone();
    let id_text = Text::auto(
        move || subtitle.clone(),
        LayoutStyle::new(),
        move || theme.text_style(FontRole::Caption, theme.subtle),
    )?;
    let labels = Container::new(
        LayoutStyle::new()
            .flex_column()
            .flex_grow(1.0)
            .min_width(0.0)
            .gap(1.0),
        vec![box_item(name_text), box_item(id_text)],
    )?;

    // An `RwSignal<String>` per row rather than one field the Save button reads back: a save that walked two
    // hundred rows would have to hold two hundred signals it almost never uses. The field feeds the shared map
    // through an effect instead, and `keeping` ties that effect to this row's lifetime — a bare `effect(…)`
    // statement runs once and stops, which is the trap `shared::reactive` exists to document.
    let icon_field = signal(override_now);
    let watched = icon_field.read_only();
    let key = id.clone();
    let sync = telar::effect(move || {
        let text = watched.get();
        let mut map = icons.peek();
        let changed = match text.trim() {
            "" => map.remove(&key).is_some(),
            icon => map.insert(key.clone(), icon.to_string()).as_deref() != Some(icon),
        };
        if changed {
            icons.set(map);
        }
    });
    let placeholder = if declared.trim().is_empty() {
        telar::t!("settings.apps.no_icon")
    } else {
        declared.clone()
    };
    let boxed_icon_field = StyledContainer::new(
        LayoutStyle::new()
            .width(150.0)
            .flex_shrink(0.0)
            .padding_horizontal(8.0)
            .padding_vertical(4.0),
        move |_r| RectStyle::filled(theme.base, 8.0),
        vec![box_item(
            Input::new(
                icon_field,
                LayoutStyle::new()
                    .flex_grow(1.0)
                    .height(theme.font(FontRole::Body) * 1.6),
                move || theme.text_style(FontRole::Caption, theme.text),
            )?
            .placeholder(placeholder),
        )],
    )?;

    let star = toggle_pill(
        "star",
        row.favourite,
        theme.yellow,
        theme,
        toggle_membership(favourites, id.clone()),
    )?;
    let eye = toggle_pill(
        "eye-off",
        row.hidden,
        theme.red,
        theme,
        toggle_membership(hidden, id),
    )?;

    let row = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(10.0)
            .padding_horizontal(10.0)
            .padding_vertical(6.0)
            .width(SizeDimension::Percent(1.0)),
        move |_r| RectStyle::filled(theme.base, 8.0),
        vec![
            icon,
            Box::new(labels),
            Box::new(boxed_icon_field),
            star,
            eye,
        ],
    )?;
    // The wrapper is what holds the field's effect for exactly this row's lifetime; it paints nothing and is
    // full-width, which is what the row already is.
    crate::shared::reactive::keeping(Box::new(row), sync)
}

/// Adds `id` to a list or takes it out again — what both switches on an application row do.
fn toggle_membership(list: RwSignal<Vec<String>>, id: String) -> impl Fn() + 'static {
    move || {
        let mut ids = list.peek();
        match ids.iter().position(|existing| *existing == id) {
            Some(index) => {
                ids.remove(index);
            }
            None => ids.push(id.clone()),
        }
        list.set(ids);
    }
}

/// A square icon button that reads as on or off — the row-sized form of [`toggle_field`], which is a labelled
/// row and far too wide to put two of on every application.
fn toggle_pill(
    glyph: &'static str,
    on: bool,
    tint: Color,
    theme: NordTheme,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ink = if on { theme.base } else { theme.muted };
    let icon = icon_view(move || glyph.to_string(), move || ink, 16.0)?;
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new()
                .flex_shrink(0.0)
                .padding_all(6.0)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER),
            move |_r| {
                let fill = if on { tint } else { theme.overlay };
                RectStyle::filled(fill, 8.0)
            },
            vec![icon],
        )?
        .on_press(on_press),
    ))
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.dashboard"),
        rows,
        save,
        theme,
    )
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

/// The three live pages: a mixer, the networks in range, and the Bluetooth devices.
///
/// Each is the module's own panel rather than a second rendering of the same service. A settings page that
/// listed access points its own way would be a second thing to keep in step with NetworkManager, and the first
/// divergence between the two would be invisible — the panel a user reaches from the bar and the page they
/// reach from the nav would simply disagree.
///
/// They take no `path` and draw no Save: nothing here is a config value. Connecting to a network is an action
/// on the machine, not a preference to be written down.
fn mixer_live_section(
    config: &Config,
    _path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    live_section(
        || telar::t!("settings.section.mixer"),
        crate::modules::mixer::mixer_view(config.audio, theme, false)?,
        theme,
    )
}

fn network_live_section(
    config: &Config,
    _path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    live_section(
        || telar::t!("settings.section.wifi"),
        crate::modules::network::network_view(config.network, false)?,
        theme,
    )
}

fn bluetooth_live_section(
    config: &Config,
    _path: &Path,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    live_section(
        || telar::t!("settings.section.devices"),
        crate::modules::bluetooth::bluetooth_view(config.bluetooth, false)?,
        theme,
    )
}

/// A section that is a control rather than a form: the page's own heading, then the module's live content, and
/// no Save — [`section`] without the button, because there is nothing here to write.
fn live_section(
    title: impl Fn() -> String + 'static,
    content: Box<dyn LayoutItem>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(10.0)
            .width(SizeDimension::Percent(1.0)),
        vec![section_label(title, theme)?, content],
    )?))
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.bluetooth"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
        toggle_field(
            || telar::t!("settings.field.compact"),
            compact.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.compact"),
            compact.clone(),
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.screenshot"),
        rows,
        save,
        theme,
    )
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
    section(
        || telar::t!("settings.section.utilities"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        rows.push(subheader(
            || telar::t!("settings.subheader.monitors"),
            theme,
        )?);
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
    section(
        || telar::t!("settings.section.background"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.wallpaper"),
        rows,
        save,
        theme,
    )
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
        text_field(
            || telar::t!("settings.field.blur"),
            blur.clone(),
            "0",
            theme,
        )?,
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
        toggle_field(
            || telar::t!("settings.field.enabled"),
            enabled.clone(),
            theme,
        )?,
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
    section(
        || telar::t!("settings.section.animation"),
        rows,
        save,
        theme,
    )
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
        std::iter::once(section_label(
            || telar::t!("settings.section.about"),
            theme,
        )?)
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

/// K14, the recorder half: every field the form helpers build, so a section knows when one of them moved.
///
/// A thread-local rather than a parameter because the alternative is threading a tracker through all forty
/// `*_section` functions and every `text_field`/`toggle_field`/`enum_field` call inside them. The forms are
/// built one at a time on the driver thread, and each ends with exactly one [`save_button`] — which is where
/// the recording is drained. That is the whole contract: **a form's fields must be built before its button.**
///
/// Each entry is an effect that bumps `revision` when its field changes, plus the revision itself. Effects are
/// handed to the button so they live exactly as long as the form does.
struct FormRecorder {
    revision: RwSignal<u64>,
    subscriptions: Vec<telar::Effect>,
}

thread_local! {
    static RECORDING: std::cell::RefCell<Option<FormRecorder>> = const { std::cell::RefCell::new(None) };
}

/// How long after the last keystroke a live-preview form applies itself. Long enough that typing a font name
/// is one apply rather than nine, short enough to read as a preview rather than as a delay.
const LIVE_DEBOUNCE: std::time::Duration = std::time::Duration::from_millis(700);

/// Registers `value` as one of the current form's fields. Called by every field helper.
fn record_field<T: Clone + PartialEq + 'static>(value: &RwSignal<T>) {
    let watched = value.read_only();
    RECORDING.with(|recording| {
        let mut recording = recording.borrow_mut();
        let recorder = recording.get_or_insert_with(|| FormRecorder {
            revision: signal(0u64),
            subscriptions: Vec::new(),
        });
        let revision = recorder.revision.clone();
        // An effect fires once when it is registered, and that first run is the field being *seeded* — not a
        // user changing anything. Reporting it would make every form apply itself the moment it was drawn.
        let seeded = std::cell::Cell::new(false);
        recorder.subscriptions.push(telar::effect(move || {
            let _ = watched.get();
            if seeded.replace(true) {
                revision.set(revision.peek() + 1);
            }
        }));
    });
}

/// Wires the recorded fields to `apply`, debounced — the second half of K14.
///
/// Returns the subscriptions for the caller to hold. The window survives the reload its own write causes (the
/// shell reconciles its surfaces in place rather than reopening them), so what the user is typing into is the
/// same field it was before the change landed.
fn live_apply(apply: Rc<dyn Fn()>) -> Vec<telar::Effect> {
    let Some(recorder) = RECORDING.with(|recording| recording.borrow_mut().take()) else {
        return Vec::new();
    };
    let FormRecorder {
        revision,
        mut subscriptions,
    } = recorder;
    let watched = revision.read_only();
    subscriptions.push(telar::effect(move || {
        let at = watched.get();
        if at == 0 {
            return;
        }
        let apply = Rc::clone(&apply);
        let watched = watched.clone();
        // Debounced by re-reading the counter when the timer fires: a change that arrived in the meantime has
        // its own timer running, so only the last one in a burst applies.
        platform_layershell::timeout(LIVE_DEBOUNCE, move || {
            if watched.peek() == at {
                apply();
            }
        });
    }));
    subscriptions
}

thread_local! {
    /// The file exactly as it was when this settings window first opened, which is what Revert restores.
    static OPENED_WITH: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
}

/// Puts `config.toml` back to how it was when this settings window opened, and lets the config watcher apply
/// it — the Revert half of K14.
///
/// The whole file rather than a per-section undo stack: with apply-on-change there is no single edit to undo,
/// and "how it was when I opened this" is the state a user actually means. It therefore also discards a change
/// made to the file by hand while the window was open, which is why it is a button and not automatic.
fn revert_to_opened(path: &Path) {
    let snapshot = OPENED_WITH.with(|slot| slot.borrow().clone());
    let Some(text) = snapshot else {
        return;
    };
    // This window's own write, like a save — what it does to the forms it decides itself, below.
    crate::core::shell::authored_change(MODULE);
    if let Err(e) = std::fs::write(path, text) {
        tracing::warn!("settings: could not revert {}: {e}", path.display());
    }
}

fn persist<T: Serialize>(path: &Path, name: &str, value: &T) {
    // Written before the write, not after: the config watcher can notice the file inside the same turn.
    crate::core::shell::authored_change(MODULE);
    if let Err(e) = Config::save_section(path, name, value) {
        tracing::warn!("settings: could not save [{name}]: {e}");
    }
}

/// [`persist`] for a form that owns only *part* of a `[toml]` section.
///
/// `save_section` replaces the whole table, so every form has to hand it the keys it does not edit as well —
/// and taking those from the snapshot the form was built with is what makes two forms over one section
/// destructive: the applications page marks a favourite, the launcher form saves a width ten seconds later,
/// and the favourite is gone. Reading the file at save time is also what makes a hand-edit made while the
/// settings window was open survive it.
fn persist_with<T: Serialize>(path: &Path, name: &str, build: impl FnOnce(&Config) -> T) {
    persist(path, name, &build(&Config::load_or_default(path)));
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
    record_field(&value);
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
    record_field(&value);
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
    record_field(&value);
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

/// A form's action button — and, with live preview on, where that form's fields get wired to it.
///
/// The wiring lives here because every `*_section` builds its fields and then calls this exactly once, so this
/// is the one point in the file that has both the form's fields (through [`RECORDING`]) and the action they
/// feed. The alternative was a fortieth argument on forty functions.
fn save_button(
    label: impl Fn() -> String + 'static,
    theme: NordTheme,
    on_press: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let on_press: Rc<dyn Fn()> = Rc::new(on_press);
    let live = live_apply(Rc::clone(&on_press));

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
    .on_press(move || on_press());
    if live.is_empty() {
        return Ok(Box::new(button));
    }
    crate::shared::reactive::keeping_all(Box::new(button), live)
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

fn variant_str(variant: Variant) -> &'static str {
    match variant {
        Variant::Filled => "filled",
        Variant::Default => "default",
    }
}

fn parse_variant(s: &str) -> Variant {
    match s {
        "filled" => Variant::Filled,
        _ => Variant::Default,
    }
}

fn open_mode_str(mode: OpenMode) -> &'static str {
    match mode {
        OpenMode::Float => "float",
        OpenMode::Drawer => "drawer",
    }
}

fn parse_open_mode(s: &str) -> OpenMode {
    match s {
        "float" => OpenMode::Float,
        _ => OpenMode::Drawer,
    }
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

    /// Every form on every page, built. `labels_live_switch_locale` only ever reaches the first page — the
    /// page area is a keyed list over the *selected* page — so until this existed, a section that panicked on
    /// a nested signal read shipped as long as it was not on Appearance. Which is most of them.
    #[test]
    fn every_section_on_every_page_builds() {
        let config = Config::starter();
        let path = std::path::PathBuf::from("/nonexistent/hyprshell-test.toml");
        for page in pages::PAGES {
            for section in page.sections {
                reset_layout_runtime();
                set_theme(NordTheme::new());
                assert!(
                    (section.build)(&config, &path, NordTheme::new()).is_ok(),
                    "{}/{} does not build",
                    page.label,
                    section.label
                );
            }
        }
    }

    /// Switching pages puts the view back at the top — and leaves it free to move afterwards.
    ///
    /// The second half is the one that has to be asserted: "scroll back to the top when the page changes" is
    /// an effect, and an effect that reads the offset it writes re-runs on every wheel tick and puts the view
    /// straight back, which is a page that cannot be scrolled at all rather than one that starts at its top.
    #[test]
    fn a_page_switch_returns_to_the_top_and_the_page_still_scrolls() {
        use telar::{ComponentList, Event, PointerSource, ScrollDelta};

        telar::Scope::with(|| {
            reset_layout_runtime();
            set_theme(NordTheme::new());
            let panel = settings_panel().expect("settings panel");
            let mut tree = ComponentList::new(SurfaceRoot::new(panel).expect("root"));
            tree.on_event(&Event::WindowResized {
                width: 900,
                height: 600,
            });

            // The panel's own state, reached the way the panel reaches it: `kept` is scoped to the surface,
            // and this test is that surface.
            let page = kept("settings.page", || signal(0usize));
            let (_, offset_y) = kept("settings.scroll", || (signal(0.0f32), signal(0.0f32)));

            // Over the page area — right of the nav pane, below the header — and then a wheel down.
            let wheel = |tree: &mut ComponentList| {
                tree.on_event(&Event::PointerMoved {
                    x: 600.0,
                    y: 300.0,
                    source: PointerSource::Mouse,
                });
                tree.on_event(&Event::Scrolled {
                    delta: ScrollDelta::Pixels { x: 0.0, y: -120.0 },
                });
                telar::batch(|| {});
                telar::relayout_if_dirty();
            };

            wheel(&mut tree);
            assert!(
                offset_y.peek() > 0.0,
                "a page taller than its viewport scrolls, or the rest of this test proves nothing"
            );

            page.set(1);
            telar::batch(|| {});
            telar::relayout_if_dirty();
            assert_eq!(
                offset_y.peek(),
                0.0,
                "changing page is a different thing in the viewport, not the same thing resized"
            );

            wheel(&mut tree);
            assert!(
                offset_y.peek() > 0.0,
                "and the new page scrolls like any other — the effect that put the view back at the top must \
                 not have subscribed itself to the offset it wrote"
            );
        });
    }

    /// The same, in the tree the shell actually mounts: the window chrome around the panel, and the panel
    /// reached the way a float reaches it. The plain-panel test above misses whatever the frame contributes.
    #[test]
    fn a_page_switch_still_scrolls_inside_the_window_frame() {
        use telar::{ComponentList, Event, PointerSource, ScrollDelta, SurfaceFrameStyle};

        telar::Scope::with(|| {
            reset_layout_runtime();
            let theme = NordTheme::new();
            set_theme(theme);
            crate::modules::drawer::set_content_radius(12.0);
            let body =
                crate::modules::drawer::module_panel(MODULE).expect("the settings panel builds");
            let frame = telar::surface_frame(
                MODULE.to_string(),
                SurfaceFrameStyle {
                    background: theme.surface,
                    title_bar: theme.overlay,
                    title_text: theme.text,
                    close: theme.muted,
                    radius: 12.0,
                    font_size: theme.font(FontRole::Title),
                },
                std::rc::Rc::new(|| {}),
                body,
                None,
            )
            .expect("surface frame");
            let mut tree = ComponentList::new(SurfaceRoot::new(frame).expect("root"));
            tree.on_event(&Event::WindowResized {
                width: 920,
                height: 680,
            });

            let page = kept("settings.page", || signal(0usize));
            let (_, offset_y) = kept("settings.scroll", || (signal(0.0f32), signal(0.0f32)));
            let wheel = |tree: &mut ComponentList| {
                tree.on_event(&Event::PointerMoved {
                    x: 600.0,
                    y: 400.0,
                    source: PointerSource::Mouse,
                });
                tree.on_event(&Event::Scrolled {
                    delta: ScrollDelta::Pixels { x: 0.0, y: -120.0 },
                });
                telar::batch(|| {});
                telar::relayout_if_dirty();
            };

            wheel(&mut tree);
            assert!(offset_y.peek() > 0.0, "the first page scrolls");

            page.set(1);
            telar::batch(|| {});
            telar::relayout_if_dirty();
            assert_eq!(offset_y.peek(), 0.0, "a page switch starts at the top");

            wheel(&mut tree);
            assert!(
                offset_y.peek() > 0.0,
                "and the page under the frame still scrolls afterwards"
            );
        });
    }

    /// K14's one subtle rule: an effect fires once when it is registered, and that run is the field being
    /// seeded from the file — not a user changing anything. Counting it would make every form on the page
    /// write itself back the moment it was drawn, which with a dozen forms on a page is a dozen config saves
    /// and a dozen reloads for a window the user has only just opened.
    #[test]
    fn seeding_a_form_is_not_a_change_to_it() {
        telar::reset_runtime();
        RECORDING.with(|recording| *recording.borrow_mut() = None);

        let name = signal("nord".to_string());
        let filled = signal(false);
        record_field(&name);
        record_field(&filled);

        let recorder = RECORDING.with(|recording| recording.borrow_mut().take());
        let recorder = recorder.expect("two fields were recorded");
        assert_eq!(recorder.subscriptions.len(), 2);
        assert_eq!(
            recorder.revision.peek(),
            0,
            "drawing the form is not editing it"
        );

        name.set("rose-pine".to_string());
        assert_eq!(recorder.revision.peek(), 1);
        filled.set(true);
        assert_eq!(recorder.revision.peek(), 2, "either field counts");

        // And the recording is per form: the next one starts empty, or a section would apply its neighbour's
        // fields as well as its own.
        assert!(RECORDING.with(|recording| recording.borrow().is_none()));
    }

    /// The bug an index-keyed list would ship: deleting one entry has to take *that* entry's values with it,
    /// and leave every other row still holding its own. Nothing about the rendered form says which is which,
    /// so it is only visible as a user finding someone else's numbers in the box they were editing.
    #[test]
    fn removing_one_entry_leaves_the_others_holding_their_own_values() {
        telar::reset_runtime();
        let list = TableList::new(vec![
            IdleStage {
                timeout: 300,
                action: "lock on".into(),
                return_action: String::new(),
            },
            IdleStage {
                timeout: 600,
                action: "shell dpms off".into(),
                return_action: "shell dpms on".into(),
            },
            IdleStage {
                timeout: 900,
                action: "session do suspend".into(),
                return_action: String::new(),
            },
        ]);
        let ids = list.order.peek();
        assert_eq!(ids.len(), 3);

        list.edit(ids[2], |stage| stage.timeout = 1200);
        list.remove(ids[0]);

        let left = list.collect();
        assert_eq!(
            left.iter().map(|s| s.timeout).collect::<Vec<_>>(),
            vec![600, 1200],
            "the survivors keep their own values, including one edited before the removal"
        );
        assert_eq!(left[0].return_action, "shell dpms on");

        // And an added row is a new entry, never a reused slot: an id is spent even when one has been freed.
        list.add(IdleStage::default());
        let after = list.order.peek();
        assert_eq!(after.len(), 3);
        assert!(
            !after.contains(&ids[0]),
            "the removed id is not handed out again"
        );
        assert_eq!(list.collect().len(), 3);
    }

    #[test]
    fn the_application_list_narrows_alphabetically_and_stops_at_the_cap() {
        let app = |id: &str, name: &str, keyword: &str| apps::App {
            id: id.to_string(),
            name: name.to_string(),
            keywords: vec![keyword.to_string()],
            ..apps::App::default()
        };
        let installed = vec![
            app("zed", "Zed", "editor"),
            app("firefox", "Firefox", "www"),
            app("org.gnome.Nautilus", "Files", "browser"),
        ];

        let names = |query: &str| {
            matching(&installed, query)
                .into_iter()
                .map(|a| a.name)
                .collect::<Vec<_>>()
        };
        assert_eq!(
            names(""),
            vec!["Files", "Firefox", "Zed"],
            "an alphabet, not the order the directories were scanned in"
        );
        assert_eq!(names("www"), vec!["Firefox"], "keywords are searchable");
        assert_eq!(
            names("nautilus"),
            vec!["Files"],
            "so is the desktop id, which is what the config actually stores"
        );
        assert!(names("zzz-no-such-app").is_empty());

        let many: Vec<apps::App> = (0..APP_ROWS + 50)
            .map(|i| app(&format!("app{i}"), &format!("App {i:04}"), ""))
            .collect();
        assert_eq!(matching(&many, "").len(), APP_ROWS);
    }

    #[test]
    fn the_wallpaper_browser_groups_by_folder_and_narrows_by_name() {
        use crate::shared::services::wallpaper::Entry;
        let entry = |name: &str, folder: &str| Entry {
            path: PathBuf::from(format!("/w/{folder}/{name}.jpg")),
            name: name.to_string(),
            folder: folder.to_string(),
        };
        let library = vec![
            entry("dune", "deserts"),
            entry("fjord", ""),
            entry("erg", "deserts"),
        ];

        let grouped = folders(&library, "");
        assert_eq!(
            grouped.iter().map(|(f, _)| f.as_str()).collect::<Vec<_>>(),
            vec!["", "deserts"],
            "the library root sorts above its sub-folders"
        );
        assert_eq!(grouped[1].1.len(), 2);

        // A folder name matches as well as an image name: "show me the deserts" is the question a grouped
        // browser exists to answer, and typing one image's name to reach its neighbours is not an answer.
        assert_eq!(folders(&library, "deserts")[0].1.len(), 2);
        assert_eq!(folders(&library, "fjord").len(), 1);
        assert!(folders(&library, "tundra").is_empty());
    }

    #[test]
    fn a_group_is_keyed_on_which_of_its_tiles_is_current() {
        use crate::shared::services::wallpaper::Entry;
        let entries: Vec<Entry> = ["a", "b"]
            .iter()
            .map(|name| Entry {
                path: PathBuf::from(format!("/w/{name}.jpg")),
                name: name.to_string(),
                folder: String::new(),
            })
            .collect();
        // Choosing a picture has to move the ring, and nothing else about the group changed when it did.
        let none = group_key("", &entries, None);
        let first = group_key("", &entries, Some(Path::new("/w/a.jpg")));
        let second = group_key("", &entries, Some(Path::new("/w/b.jpg")));
        assert_ne!(none, first);
        assert_ne!(first, second);
        assert_eq!(first, group_key("", &entries, Some(Path::new("/w/a.jpg"))));
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

    /// A reorder must not cost an entry its own settings. The comma-separated field this replaced could only
    /// carry ids, so it had to reconstruct `{ id = "clock", accent = "red" }` by claiming entries back by
    /// name; the pill editor moves the entry itself, and this is the guard that it keeps doing so — including
    /// across zones, where losing the accent would look like the module having been re-added rather than moved.
    #[test]
    fn dragging_a_module_carries_its_own_settings_with_it() {
        telar::reset_runtime();
        let accented = ModuleEntry {
            id: "clock".to_string(),
            accent: Some("red".to_string()),
            variant: None,
        };
        let filled = ModuleEntry {
            id: "clock".to_string(),
            variant: Some(crate::Variant::Filled),
            accent: None,
        };
        let bar = BarConfig {
            start: vec![ModuleEntry::bare("workspaces"), accented.clone()],
            center: vec![filled.clone()],
            end: Vec::new(),
            ..BarConfig::default()
        };
        let editor = ZoneEditor::new(&bar);

        editor.move_entry((0, 1), (0, 0));
        assert_eq!(
            editor.entries(0),
            vec![accented.clone(), ModuleEntry::bare("workspaces")]
        );

        editor.move_entry((0, 0), (1, 1));
        assert_eq!(editor.entries(0), vec![ModuleEntry::bare("workspaces")]);
        assert_eq!(editor.entries(1), vec![filled, accented]);

        editor.move_entry((1, 0), (2, 9));
        assert_eq!(editor.entries(2).len(), 1);
        editor.remove(2, 0);
        assert!(editor.entries(2).is_empty());
        editor.remove(2, 0);
        assert!(editor.entries(2).is_empty(), "removing nothing is a no-op");

        editor.append(2, ModuleEntry::bare("notes"));
        assert_eq!(editor.entries(2), vec![ModuleEntry::bare("notes")]);
    }

    /// A removed pill leaves its rect behind, and nothing about the entry says the widget is gone. Without the
    /// length check, that ghost goes on winning drops over the area it used to occupy — ahead of whichever
    /// live pill the map happened to be walked to second, which makes it look intermittent.
    #[test]
    fn a_removed_pill_does_not_keep_catching_drops() {
        telar::reset_runtime();
        let bar = BarConfig {
            start: vec![
                ModuleEntry::bare("workspaces"),
                ModuleEntry::bare("clock"),
                ModuleEntry::bare("notes"),
            ],
            ..BarConfig::default()
        };
        let editor = ZoneEditor::new(&bar);
        let at = |x: f32| Rect {
            x,
            y: 0.0,
            width: 40.0,
            height: 20.0,
        };
        for (index, x) in [0.0f32, 50.0, 100.0].into_iter().enumerate() {
            editor.track(0, index, signal(at(x)).read_only());
        }
        // The zone row underneath them all, which is what a drop on empty space lands on.
        editor.track(
            0,
            ZONE_ROW,
            signal(Rect {
                x: 0.0,
                y: 0.0,
                width: 400.0,
                height: 20.0,
            })
            .read_only(),
        );

        assert_eq!(editor.drop_target((105.0, 10.0)), Some((0, 2)));
        editor.remove(0, 2);
        assert_eq!(
            editor.drop_target((105.0, 10.0)),
            Some((0, 2)),
            "the ghost's area now falls through to the zone row, which appends"
        );
        // And the pills that are still there keep answering for themselves.
        assert_eq!(editor.drop_target((55.0, 10.0)), Some((0, 1)));
        assert_eq!(
            editor.drop_target((75.0, 10.0)),
            Some((0, 2)),
            "past the middle of a pill lands after it"
        );
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
