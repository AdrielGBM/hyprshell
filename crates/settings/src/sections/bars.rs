//! Every bar, and every chip a bar can carry.
//!
//! One `*_section` per form on the page, each owning one `[toml]` table and saving it on its own.

use std::rc::Rc;

use telar::{
    AlignItems, Container, LayoutError, LayoutItem, LayoutStyle, ReactiveList, Rect, RectStyle,
    RwSignal, SizeDimension, StyledContainer, Text, box_item, signal,
};

use crate::form::*;
use crate::table::*;
use config::theme::{FontRole, NordTheme};
use config::{
    ActiveWindowConfig, BarConfig, BarsConfig, BatteryConfig, BatteryWarning, ClockConfig,
    DrawerConfig, FloatConfig, LockStatusConfig, ModuleEntry, ModuleOverride, OpenMode, OsdConfig,
    PanelsConfig, PopoutsConfig, StatusIconsConfig, TemperatureConfig, TrayConfig, Variant,
    WorkspacesConfig,
};

/// Every pill's laid-out box, keyed by `(zone, index)` — and by `(zone, ZONE_ROW)` for a zone's own row.
type PillRects =
    Rc<std::cell::RefCell<std::collections::HashMap<(usize, usize), telar::ReadSignal<Rect>>>>;

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
                move || crate::pages::label("settings.field", label),
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
    let mut chips: Vec<Box<dyn LayoutItem>> = Vec::new();
    for id in ui::module::with_registry(|registry| registry.ids()) {
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

pub(crate) fn bars_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn panels_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn popouts_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn osd_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn clock_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn workspaces_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn temperature_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

/// `[modules.<id>]`: the per-module presentation overrides.
///
/// Keyed on the registry rather than on what the bars currently use, so a module can be styled before it is
/// placed — the alternative would be a user having to add a chip, save, reopen the page and only then be able
/// to give it an accent.
pub(crate) fn module_overrides_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
    let mut ids: Vec<String> = ui::module::with_registry(|registry| registry.ids());
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

/// The `[[battery.warn_levels]]` editor: one card per warning, with Add and Remove.
pub(crate) fn battery_warnings_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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
            move || list.add(BatteryWarning::default()),
        )?
    };

    let path = path.to_path_buf();
    let saved = Rc::clone(&list);
    let save = save_button(
        || telar::t!("settings.save.battery_warnings"),
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

pub(crate) fn battery_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn lock_status_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn status_icons_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn tray_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

pub(crate) fn active_window_section() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (config, path) = crate::form::source();
    let theme = telar::use_theme::<NordTheme>();
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

#[cfg(test)]
mod tests {
    use super::*;
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
            variant: Some(config::Variant::Filled),
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
}
