use std::path::{Path, PathBuf};

use rsx::{
    AlignItems, Container, Input, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    RwSignal, SizeDimension, StyledContainer, Text, TextStyle, box_item, signal, use_theme,
};
use serde::Serialize;

use crate::core::config::{
    Align, BackgroundConfig, BarConfig, BarsConfig, Config, CornersConfig, DrawerConfig, Edge,
    FloatConfig, GeneralConfig, IconsConfig, NotificationsConfig, OsdConfig, PanelsConfig, Shape,
    ShapeConfig, ThemeConfig,
};
use crate::shared::icon::icon_view;
use crate::shared::module::{icon_px, module_fg};
use crate::shared::theme::{FontRole, NordTheme};

const EDGES: &[&str] = &["top", "bottom", "left", "right"];
const ALIGNS: &[&str] = &["start", "center", "end"];
const SHAPES: &[&str] = &["bar", "sections", "chips"];
const LANGUAGES: &[&str] = &["en", "es"];

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

    let rows = vec![language_field(
        || rsx::t!("settings.field.language"),
        lang.clone(),
        theme,
    )?];

    let path = path.to_path_buf();
    let save = save_button(|| rsx::t!("settings.save.general"), theme, move || {
        persist(&path, "general", &GeneralConfig {
            language: lang.peek(),
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
        text_field(|| rsx::t!("settings.field.name"), name.clone(), "nord", theme)?,
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
        start: signal(join_csv(&bar.start)),
        center: signal(join_csv(&bar.center)),
        end: signal(join_csv(&bar.end)),
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
        start: split_csv(&s.start.peek()),
        center: split_csv(&s.center.peek()),
        end: split_csv(&s.end.peek()),
        shape: base.shape,
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
