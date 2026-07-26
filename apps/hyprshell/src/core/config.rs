use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rsx::Color;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};

use crate::shared::theme::NordTheme;

/// Fallback gap a panel keeps from a hugging bar (one with no outer gap of its own) and from the screen edges.
pub const DEFAULT_PANEL_GAP: u32 = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Edge {
    Top,
    Bottom,
    Left,
    Right,
}

impl Edge {
    pub const ALL: [Edge; 4] = [Edge::Top, Edge::Bottom, Edge::Left, Edge::Right];

    pub fn is_horizontal(self) -> bool {
        matches!(self, Edge::Top | Edge::Bottom)
    }

    pub fn is_vertical(self) -> bool {
        !self.is_horizontal()
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Edge::Top => "top",
            Edge::Bottom => "bottom",
            Edge::Left => "left",
            Edge::Right => "right",
        }
    }

    pub fn corners(self) -> (Corner, Corner) {
        match self {
            Edge::Top => (Corner::TopLeft, Corner::TopRight),
            Edge::Bottom => (Corner::BottomLeft, Corner::BottomRight),
            Edge::Left => (Corner::TopLeft, Corner::BottomLeft),
            Edge::Right => (Corner::TopRight, Corner::BottomRight),
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

impl Corner {
    pub const ALL: [Corner; 4] = [
        Corner::TopLeft,
        Corner::TopRight,
        Corner::BottomLeft,
        Corner::BottomRight,
    ];

    pub fn horizontal_edge(self) -> Edge {
        match self {
            Corner::TopLeft | Corner::TopRight => Edge::Top,
            Corner::BottomLeft | Corner::BottomRight => Edge::Bottom,
        }
    }

    pub fn vertical_edge(self) -> Edge {
        match self {
            Corner::TopLeft | Corner::BottomLeft => Edge::Left,
            Corner::TopRight | Corner::BottomRight => Edge::Right,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Corner::TopLeft => "top-left",
            Corner::TopRight => "top-right",
            Corner::BottomLeft => "bottom-left",
            Corner::BottomRight => "bottom-right",
        }
    }
}

/// Background granularity (`Bar`/`Sections`/`Chips`); visual appearance (hug/float/rounding) is controlled by gap/spacing/radius, not mode.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Shape {
    #[default]
    Bar,
    Sections,
    Chips,
}

/// Global shape settings. `gap` defaults to 0 (edge-to-edge bar; floating is opt-in). `spacing`/`radius` are unset by default so they fall back to the theme's values — set them here (or per-bar) to override the theme.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ShapeConfig {
    pub mode: Shape,
    pub frame: bool,
    pub gap: u32,
    pub spacing: Option<u32>,
    pub radius: Option<u32>,
    pub inactive_size: u32,
}

impl Default for ShapeConfig {
    fn default() -> Self {
        Self {
            mode: Shape::Bar,
            frame: false,
            gap: 0,
            spacing: None,
            radius: None,
            inactive_size: 6,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct BarShape {
    pub mode: Option<Shape>,
    pub gap: Option<u32>,
    pub spacing: Option<u32>,
    pub radius: Option<u32>,
}

#[derive(Clone, Copy, Debug)]
pub struct ResolvedShape {
    pub mode: Shape,
    pub gap: u32,
    /// Resolved from per-bar → global `[shape]` → theme.
    pub spacing: f32,
    /// Resolved from per-bar → global `[shape]` → theme.
    pub radius: f32,
}

impl ResolvedShape {
    pub fn padding(self) -> f32 {
        (self.spacing / 2.0).round()
    }

    /// Chip radius shrunk to nest inside a unit.
    pub fn chip_radius(self) -> f32 {
        (self.radius - self.padding()).max(0.0)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct CornersConfig {
    pub top_left: Option<String>,
    pub top_right: Option<String>,
    pub bottom_left: Option<String>,
    pub bottom_right: Option<String>,
}

impl CornersConfig {
    pub fn get(&self, corner: Corner) -> Option<&str> {
        match corner {
            Corner::TopLeft => self.top_left.as_deref(),
            Corner::TopRight => self.top_right.as_deref(),
            Corner::BottomLeft => self.bottom_left.as_deref(),
            Corner::BottomRight => self.bottom_right.as_deref(),
        }
    }
}

/// Container background: `Default` is transparent (blends into the bar, highlights on hover/press); `Filled` paints a solid accent with an auto-contrast foreground.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Variant {
    #[default]
    Default,
    Filled,
}

/// How a module's panel opens: a drawer hanging off the bar edge (default), or a centred floating window with a title bar and close button.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OpenMode {
    #[default]
    Drawer,
    Float,
}

/// Per-module presentation override, keyed by module id under `[modules.<id>]`: container variant, an accent token that wins over the global `[theme] accent`, and how its panel opens.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct ModuleOverride {
    pub variant: Variant,
    pub accent: Option<String>,
    pub open: OpenMode,
}

/// Which bar zone a module sits in; a drawer derives its cross-axis alignment from this.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Zone {
    Start,
    Center,
    End,
}

/// One module placed on a bar.
///
/// Written as a bare id in the common case (`start = ["clock", "workspaces"]`) and as a table when an instance
/// needs settings of its own (`{ id = "clock", accent = "red" }`). The table form is what lets the same module
/// appear on a bar twice looking different — a `[modules.<id>]` override is keyed by id and so applies to every
/// copy at once.
///
/// Deliberately presentation-only. `open` stays under `[modules.<id>]` because a panel is toggled by module id
/// from three places — a chip, `hyprshell panel toggle`, a keybind — and only one of them has an entry in hand;
/// an entry-scoped answer would make the same panel open differently depending on how you asked for it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ModuleEntry {
    pub id: String,
    pub variant: Option<Variant>,
    pub accent: Option<String>,
}

impl ModuleEntry {
    pub fn bare(id: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            variant: None,
            accent: None,
        }
    }

    /// Whether the entry carries nothing beyond its id, and so writes back as a plain string.
    fn is_bare(&self) -> bool {
        self.variant.is_none() && self.accent.is_none()
    }
}

/// The table form, and the shape a non-bare entry serialises to.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
struct ModuleEntryTable {
    id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    variant: Option<Variant>,
    #[serde(skip_serializing_if = "Option::is_none")]
    accent: Option<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum RawModuleEntry {
    Bare(String),
    Table(ModuleEntryTable),
}

impl<'de> Deserialize<'de> for ModuleEntry {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match RawModuleEntry::deserialize(deserializer)? {
            RawModuleEntry::Bare(id) => Self::bare(id),
            RawModuleEntry::Table(t) => Self {
                id: t.id,
                variant: t.variant,
                accent: t.accent,
            },
        })
    }
}

impl Serialize for ModuleEntry {
    /// A bare entry writes back as the string it was read from, so a config that never used the table form
    /// round-trips through the settings panel unchanged.
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if self.is_bare() {
            return serializer.serialize_str(&self.id);
        }
        ModuleEntryTable {
            id: self.id.clone(),
            variant: self.variant,
            accent: self.accent.clone(),
        }
        .serialize(serializer)
    }
}

/// The drawer panel's size (§4): a fixed width and a max height its content scrolls within.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct DrawerConfig {
    pub width: f32,
    pub max_height: f32,
}

impl Default for DrawerConfig {
    fn default() -> Self {
        Self {
            width: 320.0,
            max_height: 280.0,
        }
    }
}

/// A floating window's size (§5) in logical px.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct FloatConfig {
    pub width: u32,
    pub height: u32,
}

impl Default for FloatConfig {
    fn default() -> Self {
        Self {
            width: 360,
            height: 240,
        }
    }
}

/// The compact status cluster (`[status_icons]`): several service readings sharing one chip.
///
/// `icons` is a list rather than a set of flags because the order is the point — it is what a user reads
/// left-to-right, and a fixed order would make the cluster the shell's priority instead of theirs. The names
/// match the module ids the same readings have as standalone chips, so moving one between the two is not a
/// rename; `caps` and `num` are the exception, since `lockstatus` is one module drawing two indicators and a
/// cluster should be able to take only one of them.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct StatusIconsConfig {
    pub icons: Vec<String>,
    /// Gap between icons, as a fraction of the icon size, so a cluster keeps its proportions on any bar.
    pub spacing: f32,
}

impl Default for StatusIconsConfig {
    fn default() -> Self {
        Self {
            icons: ["volume", "mic", "network", "battery"]
                .into_iter()
                .map(String::from)
                .collect(),
            spacing: 0.35,
        }
    }
}

/// Hover popouts (`[popouts]`): the readout a chip shows while the pointer rests on it, distinct from the
/// drawer a click opens.
///
/// The delays are what separate a popout from a flicker. Without `open_delay`, dragging the pointer across the
/// bar would fire every chip's popout in turn; without `close_delay`, the popout would vanish in the gap
/// between the chip and itself. Both are clamped on read, so a typo can make a popout slow but never instant
/// or permanent.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct PopoutsConfig {
    /// Off costs nothing: no chip tracks the pointer and no surface is ever opened.
    pub enabled: bool,
    /// How long the pointer must rest on a chip before its popout opens, in ms.
    pub open_delay: u64,
    /// How long the popout survives after the pointer leaves, in ms.
    pub close_delay: u64,
    pub width: f32,
    /// The tallest a popout may grow. Its surface is this tall whatever the card needs; the surplus is carved
    /// out of the input region, so it stays click-through rather than swallowing presses.
    pub max_height: f32,
}

impl Default for PopoutsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            open_delay: 280,
            close_delay: 200,
            width: 264.0,
            max_height: 300.0,
        }
    }
}

impl PopoutsConfig {
    /// The rest before opening. Never zero: an instant popout on a bar the pointer is only crossing is noise.
    pub fn open_after(&self) -> Duration {
        Duration::from_millis(self.open_delay.clamp(60, 5_000))
    }

    /// The grace after leaving. Never zero either — the pointer has to cross the gap between the chip and the
    /// popout to reach it, and a zero grace would close it mid-crossing.
    pub fn close_after(&self) -> Duration {
        Duration::from_millis(self.close_delay.clamp(60, 5_000))
    }

    pub fn card_width(&self) -> f32 {
        self.width.clamp(140.0, 900.0)
    }

    pub fn card_height(&self) -> f32 {
        self.max_height.clamp(80.0, 1200.0)
    }
}

/// Panel presentation shared by drawers and floating windows (`[panels]`): the gap they keep from the bar and the screen edges, and each form's size. One home for both so a drawer and a float are configured the same way.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct PanelsConfig {
    /// Gap a panel keeps from the bar and the screen edges. Unset (the default) derives it — the bar's own outer gap when it floats, else [`DEFAULT_PANEL_GAP`] — so panels sit off the bar just like tiled apps; set a value to pin a fixed gap on every edge regardless of the bar.
    pub gap: Option<u32>,
    pub drawer: DrawerConfig,
    pub float: FloatConfig,
}

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[serde(rename_all = "lowercase")]
pub enum Align {
    Start,
    #[default]
    Center,
    End,
}

/// Where OSD popups appear: anchored edge, cross-axis alignment, and auto-dismiss timeout in ms (`0` disables auto-dismiss); defaults to top-centre, 1200 ms.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct OsdConfig {
    pub edge: Edge,
    pub align: Align,
    pub timeout_ms: u64,
}

impl Default for OsdConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            align: Align::Center,
            timeout_ms: 1200,
        }
    }
}

/// Where bar icons come from: an Iconify-compatible HTTP endpoint (`{provider}/{set}/{name}.svg`) and the default set applied to a bare icon name. A name may override the set inline as `set:name` (e.g. `mdi:home`), so multiple icon sets work through one endpoint. `provider` is configurable because Iconify is self-hostable/mirrorable. `app_icon_theme` names the freedesktop icon theme used to resolve notification app icons (empty = detect from GTK settings, falling back to `hicolor`).
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct IconsConfig {
    pub provider: String,
    pub default_set: String,
    pub app_icon_theme: String,
}

impl Default for IconsConfig {
    fn default() -> Self {
        Self {
            provider: "https://api.iconify.design".to_string(),
            default_set: "lucide".to_string(),
            app_icon_theme: String::new(),
        }
    }
}

/// Notification popups: where the stack anchors (defaults to top-right), how many show at once before the rest queue, the per-popup auto-dismiss (`0` = sticky), whether `critical` urgency ignores that timeout, and the card width. Popups follow the focused monitor.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct NotificationsConfig {
    pub edge: Edge,
    pub align: Align,
    pub max_visible: u32,
    pub timeout_ms: u64,
    pub critical_sticky: bool,
    pub width: f32,
    pub gap: f32,
}

impl Default for NotificationsConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Top,
            align: Align::End,
            max_visible: 4,
            timeout_ms: 5000,
            critical_sticky: true,
            width: 380.0,
            gap: 10.0,
        }
    }
}

/// Full-screen wallpaper behind everything, one surface per monitor. Off by default so the compositor's own background shows through; setting an `image` — or `enabled = true` for a plain themed background — turns it on. `[background.monitors]` maps output names to per-monitor images, each falling back to the global `image`. Paths may use `~`.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BackgroundConfig {
    pub enabled: bool,
    pub image: Option<PathBuf>,
    pub monitors: HashMap<String, PathBuf>,
}

impl BackgroundConfig {
    /// Whether hyprshell paints a background surface at all; opt-in so it never clobbers the compositor's wallpaper unless asked (an image or per-monitor entry implies it).
    pub fn is_enabled(&self) -> bool {
        self.enabled || self.image.is_some() || !self.monitors.is_empty()
    }

    /// The image for `output`: its per-monitor entry, else the global `image`; `None` paints the theme base colour.
    pub fn image_for(&self, output: Option<&str>) -> Option<&PathBuf> {
        output
            .and_then(|name| self.monitors.get(name))
            .or(self.image.as_ref())
    }
}

/// App-wide settings that don't belong to a specific visual section. `language` is a BCP-47 tag
/// (`"en"`, `"es"`); empty means "follow the OS locale, else English". `show_over_fullscreen` lifts the bars
/// onto the overlay layer so they stay visible over a fullscreen window — off by default, since a fullscreen
/// game or video is normally meant to cover them. `logo` is the icon the `logo` module shows; empty detects the
/// distribution from `/etc/os-release`.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct GeneralConfig {
    pub language: String,
    pub show_over_fullscreen: bool,
    pub logo: String,
    /// The terminal used to run a desktop entry marked `Terminal=true`; empty falls back to `xterm`.
    pub terminal: String,
}

/// The `active_window` module. `compact` shows the app's class instead of the document title — stable while you
/// move around inside one app, and much narrower. `max_chars` bounds the one bar value with no natural size: a
/// browser tab title can be a paragraph, and letting it size the chip would push every other module off the bar.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct ActiveWindowConfig {
    pub compact: bool,
    pub show_icon: bool,
    pub max_chars: u32,
}

impl Default for ActiveWindowConfig {
    fn default() -> Self {
        Self {
            compact: false,
            show_icon: true,
            max_chars: 60,
        }
    }
}

/// How a rendered label is cased. Applied after the template, so it works on `{name}` (which Hyprland reports
/// however the user named the workspace) without every template having to spell the casing out.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Capitalize {
    #[default]
    None,
    Upper,
    Lower,
    /// First letter of every word, the rest lowered.
    Title,
}

impl Capitalize {
    pub fn apply(self, text: &str) -> String {
        match self {
            Capitalize::None => text.to_string(),
            Capitalize::Upper => text.to_uppercase(),
            Capitalize::Lower => text.to_lowercase(),
            Capitalize::Title => title_case(text),
        }
    }
}

/// Uppercases the first letter of every whitespace-separated word and lowers the rest, preserving the original
/// separators so `my-notes  2` keeps its dash and its double space.
fn title_case(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at_word_start = true;
    for c in text.chars() {
        if c.is_whitespace() {
            at_word_start = true;
            out.push(c);
        } else if at_word_start {
            at_word_start = false;
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
    }
    out
}

/// The `workspaces` module.
///
/// `shown` pins how many pills the bar draws regardless of how many workspaces exist, which is what keeps the
/// bar's width from shifting every time one is created or destroyed; `0` shows exactly the ones that exist.
/// `label` is a `{id}`/`{name}`/`{index}` template so a user can have numbers, names or icons without the
/// shell enumerating presets, and `special_icons` maps a scratchpad's bare name to an Iconify glyph.
///
/// `occupied_label` and `active_label` override that template for a pill holding windows and for the focused
/// one; both empty (the default) means every pill renders the same way, which is what most bars want.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct WorkspacesConfig {
    pub shown: u32,
    /// Show only the workspaces belonging to this bar's own monitor.
    pub per_monitor: bool,
    pub show_special: bool,
    /// Draw an app icon per window inside each pill, capped at `max_window_icons`.
    pub window_icons: bool,
    pub max_window_icons: u32,
    /// Tint a pill that holds windows differently from an empty one.
    pub occupied_background: bool,
    /// The wheel over the pills switches workspace.
    pub scroll: bool,
    pub label: String,
    /// Template for a pill that holds windows; empty falls back to `label`.
    pub occupied_label: String,
    /// Template for the focused pill; empty falls back to `occupied_label`, then `label`.
    pub active_label: String,
    pub capitalize: Capitalize,
    pub special_icons: HashMap<String, String>,
}

impl Default for WorkspacesConfig {
    fn default() -> Self {
        Self {
            shown: 0,
            per_monitor: false,
            show_special: true,
            window_icons: false,
            max_window_icons: 4,
            occupied_background: true,
            scroll: true,
            label: "{id}".to_string(),
            occupied_label: String::new(),
            active_label: String::new(),
            capitalize: Capitalize::default(),
            special_icons: HashMap::new(),
        }
    }
}

impl WorkspacesConfig {
    /// The template a pill in this state renders from: the most specific one the user set, falling back to the
    /// general `label` so setting only `active_label` leaves every other pill alone.
    fn template(&self, occupied: bool, active: bool) -> &str {
        let specific = if active {
            [&self.active_label, &self.occupied_label]
        } else if occupied {
            [&self.occupied_label, &self.label]
        } else {
            [&self.label, &self.label]
        };
        specific
            .into_iter()
            .find(|t| !t.trim().is_empty())
            .unwrap_or(&self.label)
    }

    /// Renders a pill's label from the template for its state, then applies `capitalize`. `{index}` is the
    /// pill's position, which is what a fixed-width bar wants when the ids themselves are sparse.
    pub fn render_label(
        &self,
        id: i32,
        name: &str,
        index: usize,
        occupied: bool,
        active: bool,
    ) -> String {
        let rendered = self
            .template(occupied, active)
            .replace("{id}", &id.to_string())
            .replace("{name}", name)
            .replace("{index}", &(index + 1).to_string());
        self.capitalize.apply(&rendered)
    }
}

/// Audio control (`[audio]`). `increment` is what one wheel notch over the volume or microphone chip moves and
/// what `hyprshell volume up` steps by. `max_volume` is the ceiling the sink can be raised to: PipeWire lets a
/// sink boost past 100 %, which rescues a quiet laptop and wrecks a good speaker, so it belongs to the user
/// rather than to a constant in the code.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct AudioConfig {
    pub increment: i32,
    pub max_volume: i32,
}

impl Default for AudioConfig {
    fn default() -> Self {
        Self {
            increment: 5,
            max_volume: 150,
        }
    }
}

impl AudioConfig {
    /// The step, bounded so `0` or a negative typo can't leave the chip inert or make the wheel run backwards.
    pub fn step(&self) -> i32 {
        self.increment.clamp(1, 50)
    }

    /// The sink ceiling, never under 100 % — a sink must at least reach its own nominal maximum.
    pub fn ceiling(&self) -> i32 {
        self.max_volume.clamp(100, 300)
    }
}

/// Backlight control (`[brightness]`): the step a wheel notch or `hyprshell brightness up` moves. Its own
/// section rather than a key under `[audio]` so the per-output and DDC/CI settings that follow have a home.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct BrightnessConfig {
    pub increment: i32,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self { increment: 5 }
    }
}

impl BrightnessConfig {
    pub fn step(&self) -> i32 {
        self.increment.clamp(1, 50)
    }
}

/// Which scale a temperature is shown in.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum TemperatureUnit {
    #[default]
    Celsius,
    Fahrenheit,
}

impl TemperatureUnit {
    pub fn from_celsius(self, celsius: f32) -> f32 {
        match self {
            TemperatureUnit::Celsius => celsius,
            TemperatureUnit::Fahrenheit => celsius * 9.0 / 5.0 + 32.0,
        }
    }

    pub fn suffix(self) -> &'static str {
        match self {
            TemperatureUnit::Celsius => "°C",
            TemperatureUnit::Fahrenheit => "°F",
        }
    }

    /// A whole degree plus its unit — the reading a bar chip has room for, and unambiguous once a user has
    /// switched scales.
    pub fn format(self, celsius: f32) -> String {
        format!("{:.0}{}", self.from_celsius(celsius), self.suffix())
    }
}

/// The `temperature` module. `sensor` names an hwmon chip (`k10temp`, `coretemp`) or a sensor label (`Tctl`,
/// `Package id 0`) to follow; empty tracks the hottest sensor on the machine, which is what works without a
/// per-machine config. `warn`/`critical` are the °C the chip tints amber and red at — a desktop CPU that idles
/// at 65 °C should not show a permanent warning, so they are the user's numbers.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct TemperatureConfig {
    pub unit: TemperatureUnit,
    pub sensor: String,
    pub warn: f32,
    pub critical: f32,
}

impl Default for TemperatureConfig {
    fn default() -> Self {
        Self {
            unit: TemperatureUnit::default(),
            sensor: String::new(),
            warn: 70.0,
            critical: 85.0,
        }
    }
}

/// One charge level worth interrupting the user about, declared as a `[[battery.warn_levels]]` table. It fires
/// once as the charge crosses down through `level` while discharging, and re-arms once the battery is charging
/// again — so a laptop left at 19 % does not warn every minute.
///
/// `title` and `message` left empty take the shell's own translated text, so the defaults follow the UI
/// language instead of pinning English into everyone's config; `{level}` in either is replaced with the charge
/// at the moment it fired.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BatteryWarning {
    pub level: i32,
    pub title: String,
    pub message: String,
    pub icon: String,
    /// Raise it at `Critical` urgency, so with the default `critical_sticky` it waits to be read rather than
    /// timing out behind whatever the user is doing.
    pub critical: bool,
}

impl Default for BatteryWarning {
    fn default() -> Self {
        Self {
            level: 20,
            title: String::new(),
            message: String::new(),
            icon: "battery-low".to_string(),
            critical: false,
        }
    }
}

impl BatteryWarning {
    /// The notification title: the user's own text, else the shell's translated default.
    pub fn title(&self, level: i32) -> String {
        let configured = self.title.trim();
        if configured.is_empty() {
            rsx::t!("battery.warning.title")
        } else {
            configured.replace("{level}", &level.to_string())
        }
    }

    pub fn message(&self, level: i32) -> String {
        let configured = self.message.trim();
        if configured.is_empty() {
            rsx::t!("battery.warning.body", level = level.to_string())
        } else {
            configured.replace("{level}", &level.to_string())
        }
    }
}

/// Low-battery behaviour (`[battery]`): the levels that raise a notification, and the action to take once the
/// charge is low enough that the machine should put itself away. On a desktop none of it ever fires, since
/// there is no battery to read.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BatteryConfig {
    pub enabled: bool,
    pub warn_levels: Vec<BatteryWarning>,
    /// Charge at or below which `critical_action` runs; `0` (the default) never acts on the user's behalf.
    pub critical_level: i32,
    /// A `session` action id — `suspend`, `hibernate`, `shutdown`; empty runs nothing.
    pub critical_action: String,
}

impl Default for BatteryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            // Two warnings out of the box: a laptop shell that silently runs a battery flat is a bug, and a
            // desktop never reaches this code because it has no battery to report.
            warn_levels: vec![
                BatteryWarning::default(),
                BatteryWarning {
                    level: 10,
                    icon: "battery-warning".to_string(),
                    critical: true,
                    ..BatteryWarning::default()
                },
            ],
            critical_level: 0,
            critical_action: String::new(),
        }
    }
}

/// The `lockstatus` module: caps- and num-lock indicators. `hide_inactive` shows an indicator only while its
/// key is engaged, for a bar that should stay quiet; off (the default) keeps both glyphs in place, muted, so
/// the module is visible the moment it is added and the bar's width never shifts.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct LockStatusConfig {
    pub caps: bool,
    pub num: bool,
    pub hide_inactive: bool,
}

impl Default for LockStatusConfig {
    fn default() -> Self {
        Self {
            caps: true,
            num: true,
            hide_inactive: false,
        }
    }
}

/// Whether `text` matches `pattern`, in which `*` stands for any run of characters. Matching ignores case.
///
/// A deliberate subset of a regex: tray applications put a PID or a version in their id (`steam_app_12345`,
/// `chrome_status_icon_1`), which a wildcard covers, and a full regex engine is a dependency this shell does
/// not otherwise carry.
fn glob_matches(pattern: &str, text: &str) -> bool {
    let pattern = pattern.trim().to_lowercase();
    let text = text.trim().to_lowercase();
    let parts: Vec<&str> = pattern.split('*').collect();
    if parts.len() == 1 {
        return pattern == text;
    }
    let (first, last) = (parts[0], parts[parts.len() - 1]);
    // The two anchors must not overlap: `a*b` matches `ab`, but nothing shorter.
    if !text.starts_with(first)
        || !text.ends_with(last)
        || first.len() + last.len() > text.len()
    {
        return false;
    }
    let mut rest = &text[first.len()..text.len() - last.len()];
    for part in &parts[1..parts.len() - 1] {
        match rest.find(part) {
            Some(at) => rest = &rest[at + part.len()..],
            None => return false,
        }
    }
    true
}

/// The system tray (`[tray]`).
///
/// `hidden` drops an application's icon by its `Id`, and `icon_subs` swaps one for an Iconify glyph so an
/// application shipping a mismatched icon can be made to sit with the rest of the bar. Both match the id as a
/// `*` pattern rather than a literal, because a good number of applications bury a PID in theirs.
///
/// `recolour` tints every icon to the bar's foreground. Coherent, but it flattens an application that uses
/// colour to report state — a sync client going red — so it stays off unless asked for.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct TrayConfig {
    /// Off costs nothing: the module draws nothing and the service — three threads and a D-Bus name — is
    /// never started.
    pub enabled: bool,
    /// Drop the spacing between icons, for a bar with many of them.
    pub compact: bool,
    pub recolour: bool,
    /// Give every icon its own chip background instead of one shared strip.
    pub background: bool,
    pub hidden: Vec<String>,
    pub icon_subs: HashMap<String, String>,
}

impl Default for TrayConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            compact: false,
            recolour: false,
            background: false,
            hidden: Vec::new(),
            icon_subs: HashMap::new(),
        }
    }
}

impl TrayConfig {
    pub fn is_hidden(&self, id: &str) -> bool {
        self.hidden.iter().any(|p| glob_matches(p, id))
    }

    /// The Iconify glyph standing in for this application's own icon, if one is configured. The most specific
    /// pattern wins, so a blanket `*` can set a default without shadowing the entry that names one application
    /// — and the answer never depends on the map's iteration order.
    pub fn icon_sub_for(&self, id: &str) -> Option<&str> {
        self.icon_subs
            .iter()
            .filter(|(pattern, _)| glob_matches(pattern, id))
            .max_by_key(|(pattern, _)| pattern.trim_matches('*').len())
            .map(|(_, glyph)| glyph.as_str())
    }
}

/// The application launcher: a modal opened by keybind or IPC.
///
/// One entry in the launcher's action mode, declared as a `[[launcher.actions]]` table and reached by typing
/// `>`. `command` runs through `sh -c`, detached, exactly as a desktop entry's `Exec` does.
///
/// `dangerous` marks an action that should not run on a single keystroke — a reboot, a `rm`. Such an action
/// arms on the first Enter and only runs on the second, and it is listed at all only when
/// `enable_dangerous_actions` is on, so a shared or borrowed config can't hand someone a one-key wipe.
#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(default)]
pub struct LauncherAction {
    pub name: String,
    pub description: String,
    pub icon: String,
    pub command: String,
    pub enabled: bool,
    pub dangerous: bool,
}

impl Default for LauncherAction {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            icon: "zap".to_string(),
            command: String::new(),
            // Declaring an action is what enables it; the key exists so one can be parked without deleting it.
            enabled: true,
            dangerous: false,
        }
    }
}

impl LauncherAction {
    /// Whether this action should appear at all: it needs something to run, a name to show, and — when it is
    /// flagged dangerous — the config's blanket permission for dangerous actions.
    pub fn is_listed(&self, dangerous_allowed: bool) -> bool {
        self.enabled
            && !self.name.trim().is_empty()
            && !self.command.trim().is_empty()
            && (dangerous_allowed || !self.dangerous)
    }
}

/// `fuzzy` off makes the query a plain substring match, for users who find fuzzy matching too loose.
/// `hidden` lists desktop-entry ids to keep out of the results entirely, and `favourites` lists ids to pin
/// above the ranking, in the order given. `actions` are the `>`-prefixed commands of the action mode.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct LauncherConfig {
    pub width: u32,
    pub height: u32,
    pub radius: f32,
    pub max_results: u32,
    pub fuzzy: bool,
    /// Show the calculator's answer above the app matches when the query reads as arithmetic.
    pub calculator: bool,
    pub favourites: Vec<String>,
    pub hidden: Vec<String>,
    pub actions: Vec<LauncherAction>,
    /// Off by default: an action that can destroy something should take a deliberate opt-in, not arrive with
    /// a config someone pasted from the internet.
    pub enable_dangerous_actions: bool,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            width: 640,
            height: 420,
            radius: 14.0,
            max_results: 12,
            fuzzy: true,
            calculator: true,
            favourites: Vec::new(),
            hidden: Vec::new(),
            actions: Vec::new(),
            enable_dangerous_actions: false,
        }
    }
}

/// The `media` module. `preferred_player` names an MPRIS bus suffix (`spotify`, `mpv`) to favour when several
/// players are running — it only wins while that player is actually up, so naming one you don't always run
/// never blanks the chip. `aliases` renames a player for display, since players name themselves badly often
/// enough (`com.github.th_ch.youtube_music` → `YT Music`) to be worth a config key.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct MediaConfig {
    pub preferred_player: String,
    /// Max characters of `artist — title` on the bar, bounding a value with no natural size.
    pub max_chars: u32,
    /// What the wheel over the chip does: `volume`, `track`, or `none`.
    pub scroll: MediaScroll,
    pub aliases: HashMap<String, String>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            preferred_player: String::new(),
            max_chars: 40,
            scroll: MediaScroll::default(),
            aliases: HashMap::new(),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaScroll {
    #[default]
    Volume,
    Track,
    None,
}

/// The `clock` module and its panel. `format` and `date_format` are `strftime` patterns, so a user can have
/// anything from `%H:%M` to a full locale date without the shell enumerating presets; `twelve_hour` is the one
/// switch worth naming, since it is what most people actually mean by "change the clock format".
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ClockConfig {
    pub twelve_hour: bool,
    /// Overrides `twelve_hour` when set, for a user who wants seconds, a weekday, or anything else.
    pub format: Option<String>,
    pub show_date: bool,
    pub date_format: String,
}

impl Default for ClockConfig {
    fn default() -> Self {
        Self {
            twelve_hour: false,
            format: None,
            show_date: false,
            date_format: "%a %d %b".to_string(),
        }
    }
}

impl ClockConfig {
    /// The `strftime` pattern the chip renders: the explicit override, else a 12- or 24-hour clock with seconds.
    pub fn time_format(&self) -> &str {
        if let Some(format) = &self.format {
            return format;
        }
        if self.twelve_hour {
            "%I:%M:%S %p"
        } else {
            "%H:%M:%S"
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub general: GeneralConfig,
    pub bars: BarsConfig,
    pub theme: ThemeConfig,
    pub shape: ShapeConfig,
    pub corners: CornersConfig,
    pub panels: PanelsConfig,
    pub popouts: PopoutsConfig,
    pub osd: OsdConfig,
    pub icons: IconsConfig,
    pub notifications: NotificationsConfig,
    pub background: BackgroundConfig,
    pub active_window: ActiveWindowConfig,
    pub clock: ClockConfig,
    pub media: MediaConfig,
    pub workspaces: WorkspacesConfig,
    pub launcher: LauncherConfig,
    pub audio: AudioConfig,
    pub brightness: BrightnessConfig,
    pub temperature: TemperatureConfig,
    pub battery: BatteryConfig,
    pub lock_status: LockStatusConfig,
    pub status_icons: StatusIconsConfig,
    pub tray: TrayConfig,
    pub modules: HashMap<String, ModuleOverride>,
}

/// One bar per screen edge; empty bars collapse to zero. Default is all-empty by design (serde fills missing fields), so configs get only what they specify — see [`Config::starter`] for the initial setup.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BarsConfig {
    pub top: BarConfig,
    pub bottom: BarConfig,
    pub left: BarConfig,
    pub right: BarConfig,
}

impl BarsConfig {
    pub fn get(&self, edge: Edge) -> &BarConfig {
        match edge {
            Edge::Top => &self.top,
            Edge::Bottom => &self.bottom,
            Edge::Left => &self.left,
            Edge::Right => &self.right,
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BarConfig {
    pub size: u32,
    pub start: Vec<ModuleEntry>,
    pub center: Vec<ModuleEntry>,
    pub end: Vec<ModuleEntry>,
    pub shape: BarShape,
}

impl BarConfig {
    pub fn is_empty(&self) -> bool {
        self.start.is_empty() && self.center.is_empty() && self.end.is_empty()
    }
}

impl Default for BarConfig {
    fn default() -> Self {
        Self {
            size: 34,
            start: Vec::new(),
            center: Vec::new(),
            end: Vec::new(),
            shape: BarShape::default(),
        }
    }
}

/// Theme selection and overrides. `name` picks a built-in palette (or `custom`); the rest override individual tokens on top of it — numbers directly, and `[theme.colors]` per-token hex (`base = "#2e3440"`), keyed by the same names [`NordTheme::accent_by_name`] uses. Any unset field keeps the built-in's value.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub accent: String,
    pub radius: Option<u32>,
    pub spacing: Option<u32>,
    pub font_size: Option<f32>,
    pub icon_size: Option<f32>,
    /// Font family the whole shell renders in (must be installed). Unset keeps the renderer's default. Applied process-wide via [`rsx::set_default_font_family`], not carried in the (`Copy`) theme struct.
    pub font_family: Option<String>,
    /// Stroke width forced on stroke-based icon glyphs (e.g. `1.5`). Unset keeps each glyph's own stroke.
    pub icon_stroke: Option<f32>,
    pub colors: HashMap<String, String>,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "nord".to_string(),
            accent: "cyan".to_string(),
            radius: None,
            spacing: None,
            font_size: None,
            icon_size: None,
            font_family: None,
            icon_stroke: None,
            colors: HashMap::new(),
        }
    }
}

impl Config {
    /// Fresh-install starter config (distinct from `Default`, which is all-empty and backs serde's missing-field fill).
    pub fn starter() -> Self {
        Self {
            bars: BarsConfig {
                top: BarConfig {
                    size: 34,
                    start: vec![ModuleEntry::bare("workspaces")],
                    center: vec![ModuleEntry::bare("clock")],
                    end: vec![ModuleEntry::bare("notes")],
                    shape: BarShape::default(),
                },
                ..BarsConfig::default()
            },
            theme: ThemeConfig::default(),
            shape: ShapeConfig::default(),
            corners: CornersConfig::default(),
            panels: PanelsConfig::default(),
            popouts: PopoutsConfig::default(),
            osd: OsdConfig::default(),
            icons: IconsConfig::default(),
            notifications: NotificationsConfig::default(),
            background: BackgroundConfig::default(),
            active_window: ActiveWindowConfig::default(),
            clock: ClockConfig::default(),
            media: MediaConfig::default(),
            workspaces: WorkspacesConfig::default(),
            launcher: LauncherConfig::default(),
            audio: AudioConfig::default(),
            brightness: BrightnessConfig::default(),
            temperature: TemperatureConfig::default(),
            battery: BatteryConfig::default(),
            lock_status: LockStatusConfig::default(),
            status_icons: StatusIconsConfig::default(),
            tray: TrayConfig::default(),
            modules: HashMap::new(),
            general: GeneralConfig::default(),
        }
    }

    /// The effective UI language (BCP-47 tag): the `[general] language` override, else the OS locale, else
    /// English. Each surface applies it via `rsx::set_locale` when it builds.
    pub fn language(&self) -> String {
        let configured = self.general.language.trim();
        if !configured.is_empty() {
            return configured.to_string();
        }
        rsx::detect_system_locale().unwrap_or_else(|| "en".to_string())
    }

    /// The container variant for a module id, `Default` when it has no `[modules.<id>]` override.
    pub fn variant_for(&self, id: &str) -> Variant {
        self.modules.get(id).map(|m| m.variant).unwrap_or_default()
    }

    /// The accent-token name for a module id: its `[modules.<id>] accent` override, else the global `[theme] accent`; resolve via [`NordTheme::accent_by_name`](crate::NordTheme).
    pub fn accent_name_for(&self, id: &str) -> &str {
        self.modules
            .get(id)
            .and_then(|m| m.accent.as_deref())
            .unwrap_or(&self.theme.accent)
    }

    /// How a module's panel opens when clicked: its `[modules.<id>] open` override, else a drawer.
    pub fn open_mode_for(&self, id: &str) -> OpenMode {
        self.modules.get(id).map(|m| m.open).unwrap_or_default()
    }

    /// Which zone (start/center/end) a module occupies on `edge`, for deriving its drawer's alignment. A module
    /// placed twice answers with the first zone it appears in — the panel is keyed by module id, so there is
    /// only one of it to align.
    pub fn zone_of(&self, edge: Edge, module_id: &str) -> Option<Zone> {
        let bar = self.bars.get(edge);
        let holds = |entries: &[ModuleEntry]| entries.iter().any(|m| m.id == module_id);
        if holds(&bar.start) {
            Some(Zone::Start)
        } else if holds(&bar.center) {
            Some(Zone::Center)
        } else if holds(&bar.end) {
            Some(Zone::End)
        } else {
            None
        }
    }

    /// The container variant for a bar entry: its own `variant`, else the module's `[modules.<id>]` override.
    pub fn entry_variant(&self, entry: &ModuleEntry) -> Variant {
        entry.variant.unwrap_or_else(|| self.variant_for(&entry.id))
    }

    /// The accent-token name for a bar entry: its own `accent`, else the module's, else the global one.
    pub fn entry_accent_name<'a>(&'a self, entry: &'a ModuleEntry) -> &'a str {
        match entry.accent.as_deref() {
            Some(accent) => accent,
            None => self.accent_name_for(&entry.id),
        }
    }

    /// Effective shape for edge: per-bar override → global `[shape]` → (for spacing/radius) the theme.
    pub fn shape_for(&self, edge: Edge) -> ResolvedShape {
        let g = &self.shape;
        let b = &self.bars.get(edge).shape;
        ResolvedShape {
            mode: b.mode.unwrap_or(g.mode),
            gap: b.gap.unwrap_or(g.gap),
            spacing: self.resolved_spacing(edge),
            radius: self.resolved_radius(edge),
        }
    }

    /// The theme this config selects, with every `[theme]` override applied — accent, numeric tokens, and per-token `[theme.colors]` hex. The single place a theme is resolved, so its tokens back the config defaults everywhere.
    pub fn resolve_theme(&self) -> NordTheme {
        let t = &self.theme;
        let mut theme = NordTheme::named(&t.name).with_accent(&t.accent);
        if let Some(r) = t.radius {
            theme.radius = r as f32;
        }
        if let Some(s) = t.spacing {
            theme.spacing = s as f32;
        }
        if let Some(f) = t.font_size {
            theme.font_size = f;
        }
        if let Some(i) = t.icon_size {
            theme.icon_size = i;
        }
        if let Some(s) = t.icon_stroke {
            theme.icon_stroke = Some(s);
        }
        for (name, hex) in &t.colors {
            match Color::from_hex(hex) {
                Some(c) => theme = theme.with_color(name, c),
                None => tracing::warn!("theme color '{name}': invalid hex '{hex}'"),
            }
        }
        theme
    }

    /// The corner radius for `edge`: per-bar override → global `[shape] radius` → the theme's `radius`.
    pub fn resolved_radius(&self, edge: Edge) -> f32 {
        let b = &self.bars.get(edge).shape;
        b.radius
            .or(self.shape.radius)
            .map(|r| r as f32)
            .unwrap_or_else(|| self.resolve_theme().radius)
    }

    /// The module spacing for `edge`: per-bar override → global `[shape] spacing` → the theme's `spacing`.
    pub fn resolved_spacing(&self, edge: Edge) -> f32 {
        let b = &self.bars.get(edge).shape;
        b.spacing
            .or(self.shape.spacing)
            .map(|s| s as f32)
            .unwrap_or_else(|| self.resolve_theme().spacing)
    }

    /// Whether a bar hugs its edge; frame forces hug, otherwise only at gap == 0.
    pub fn hugs(&self, edge: Edge) -> bool {
        self.shape.frame || self.shape_for(edge).gap == 0
    }

    /// The bar's effective outer gap on `edge`: 0 when it hugs (frame or gap == 0), else its configured gap.
    pub fn edge_gap(&self, edge: Edge) -> u32 {
        if self.hugs(edge) {
            0
        } else {
            self.shape_for(edge).gap
        }
    }

    /// Space the bar reserves from its edge — its outer gap plus thickness — i.e. how far a panel or app must sit from the edge to clear it.
    pub fn edge_reserved(&self, edge: Edge) -> u32 {
        self.edge_gap(edge) + self.edge_thickness(edge)
    }

    /// The standard gap panels (drawers/floats) keep from the bar and the screen edges. A `[panels] gap` override wins; otherwise it's derived — the bar's own outer gap when it floats (so panels float in step with it), else a default so a hugging bar's panels still breathe. This is the "gaps_out"-style spacing that keeps a panel off the bar and off the corners.
    pub fn panel_gap(&self, edge: Edge) -> u32 {
        if let Some(gap) = self.panels.gap {
            return gap;
        }
        match self.edge_gap(edge) {
            0 => DEFAULT_PANEL_GAP,
            gap => gap,
        }
    }

    /// The corner radius a panel uses: the same as the bar on `edge` (its resolved `radius`, which itself falls back to the theme), so a drawer, float, OSD and notification card all carry the bar's rounding instead of a per-panel value.
    pub fn panel_radius(&self, edge: Edge) -> f32 {
        self.resolved_radius(edge)
    }

    /// A panel's margin `(top, right, bottom, left)` off the screen edges: uniformly the [`panel_gap`](Self::panel_gap). A panel surface uses `exclusive_zone = 0`, so the compositor already positions it past every bar's reserved zone (the reservation strip's exclusive zone); the panel only adds the standard gap beyond that — re-adding the bar's thickness here would double the distance off the bar. The one distance rule every panel shares, so a drawer, an OSD and a notification stack all clear the bar by the same config-controlled gap.
    pub fn panel_margin(&self, edge: Edge) -> (i32, i32, i32, i32) {
        let g = self.panel_gap(edge) as i32;
        (g, g, g, g)
    }

    /// Thickness of the surface on edge: bar size if active, inactive_size strip under frame, else 0.
    pub fn edge_thickness(&self, edge: Edge) -> u32 {
        let bar = self.bars.get(edge);
        if !bar.is_empty() {
            bar.size
        } else if self.shape.frame {
            self.shape.inactive_size
        } else {
            0
        }
    }

    pub fn edge_present(&self, edge: Edge) -> bool {
        self.edge_thickness(edge) > 0
    }

    /// The edge that owns corner (horizontal preferred over vertical); None if neither is active.
    pub fn corner_owner(&self, corner: Corner) -> Option<Edge> {
        let h = corner.horizontal_edge();
        let v = corner.vertical_edge();
        if self.edge_present(h) {
            Some(h)
        } else if self.edge_present(v) {
            Some(v)
        } else {
            None
        }
    }

    /// Corner modules for edge's leading and trailing ends (routed via start/end zones, no separate surfaces).
    pub fn corner_modules_for(&self, edge: Edge) -> (Option<&str>, Option<&str>) {
        let (lead, trail) = edge.corners();
        let owned = |c: Corner| {
            if self.corner_owner(c) == Some(edge) {
                self.corners.get(c)
            } else {
                None
            }
        };
        (owned(lead), owned(trail))
    }

    /// Whether the bar surface is fully opaque; only mode=bar with no gap/radius (or frame) stays opaque.
    pub fn bar_surface_opaque(&self, edge: Edge) -> bool {
        let s = self.shape_for(edge);
        s.mode == Shape::Bar && (self.shape.frame || (s.gap == 0 && s.radius == 0.0))
    }

    /// Reads and parses `config.toml`, writing the starter config on a fresh install (the `Missing` arm's job is
    /// the caller's, so the distinction survives). Parse failures are returned rather than swallowed — a typo
    /// must not silently replace a user's whole setup with the starter bar, which is what discarding the error
    /// would do; the caller keeps the last config that worked and reports the error instead.
    pub fn load(path: &Path) -> Result<Self, LoadError> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                let cfg = Config::starter();
                cfg.write_to(path);
                return Ok(cfg);
            }
            Err(e) => return Err(LoadError::Io(e)),
        };
        toml::from_str(&text).map_err(LoadError::Parse)
    }

    /// Serializes the whole config to `path`, creating its directory. Used only to seed a fresh install; edits
    /// to an existing file go through [`save_section`](Self::save_section), which preserves formatting.
    fn write_to(&self, path: &Path) {
        let Ok(text) = toml::to_string_pretty(self) else {
            return;
        };
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::write(path, text);
    }

    /// [`load`](Self::load) with the starter config as the fallback. For call sites with nothing better to fall
    /// back to (a panel building itself, a test); the running shell uses `load` so it can keep its last good
    /// config instead.
    pub fn load_or_default(path: &Path) -> Self {
        Config::load(path).unwrap_or_else(|e| {
            tracing::warn!("{e}; using the starter config");
            Config::starter()
        })
    }

    pub fn default_path() -> PathBuf {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty())
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .unwrap_or_else(|| PathBuf::from(".config"));
        base.join("hyprshell").join("config.toml")
    }

    /// Persists a single `[name]` section back to `config.toml`, replacing just that table while preserving every other section, key order, and comment in the file (format-preserving via `toml_edit`). `value` is a section struct such as [`ThemeConfig`]. Creates the file and its parent directory if missing. The running shell's config watcher then hot-reloads the change, so a save applies live.
    pub fn save_section<T: Serialize>(path: &Path, name: &str, value: &T) -> Result<(), SaveError> {
        let mut doc = std::fs::read_to_string(path)
            .unwrap_or_default()
            .parse::<DocumentMut>()
            .map_err(SaveError::Parse)?;
        let rendered = toml::to_string(value).map_err(SaveError::Serialize)?;
        let section = rendered.parse::<DocumentMut>().map_err(SaveError::Parse)?;
        let mut table = section.as_table().clone();
        // Carry over the existing header's decor (its leading comment) so replacing the table keeps the section's surrounding comments, not just its values.
        if let Some(existing) = doc.get(name).and_then(Item::as_table) {
            *table.decor_mut() = existing.decor().clone();
        }
        doc.insert(name, Item::Table(table));
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SaveError::Io)?;
        }
        std::fs::write(path, doc.to_string()).map_err(SaveError::Io)
    }
}

/// Why reading `config.toml` failed. Carries the `toml` error verbatim so the message the user sees names the
/// offending key and line rather than just "invalid config".
#[derive(Debug)]
pub enum LoadError {
    Parse(toml::de::Error),
    Io(std::io::Error),
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Parse(e) => write!(f, "config parse error: {e}"),
            LoadError::Io(e) => write!(f, "config read error: {e}"),
        }
    }
}

impl std::error::Error for LoadError {}

/// Why persisting a config section failed.
#[derive(Debug)]
pub enum SaveError {
    Serialize(toml::ser::Error),
    Parse(toml_edit::TomlError),
    Io(std::io::Error),
}

impl std::fmt::Display for SaveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SaveError::Serialize(e) => write!(f, "serializing config section: {e}"),
            SaveError::Parse(e) => write!(f, "parsing config file: {e}"),
            SaveError::Io(e) => write!(f, "writing config file: {e}"),
        }
    }
}

impl std::error::Error for SaveError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(entries: &[ModuleEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn a_zone_reads_bare_ids_and_tables_side_by_side() {
        let cfg: Config = toml::from_str(
            r#"
[bars.top]
start = ["workspaces", { id = "clock", accent = "red" }, { id = "clock", variant = "filled" }]
"#,
        )
        .expect("both entry forms parse in one array");
        assert_eq!(ids(&cfg.bars.top.start), ["workspaces", "clock", "clock"]);
        assert_eq!(cfg.bars.top.start[1].accent.as_deref(), Some("red"));
        assert_eq!(cfg.bars.top.start[2].variant, Some(Variant::Filled));

        // The point of the table form: a `[modules.<id>]` override is keyed by id, so it could only paint both copies the same.
        assert_eq!(cfg.entry_accent_name(&cfg.bars.top.start[1]), "red");
        assert_eq!(
            cfg.entry_variant(&cfg.bars.top.start[2]),
            Variant::Filled,
            "an entry's own variant wins"
        );
        assert_eq!(
            cfg.entry_variant(&cfg.bars.top.start[1]),
            Variant::Default,
            "and an entry that names none falls back rather than inheriting its neighbour's"
        );
    }

    #[test]
    fn a_bare_entry_writes_back_as_the_string_it_was_read_from() {
        let cfg: Config =
            toml::from_str("[bars.top]\nstart = [\"clock\"]\n").expect("config parses");
        let written = toml::to_string_pretty(&cfg.bars.top).expect("serialises");
        assert!(
            written.contains("start = [\"clock\"]"),
            "a bare entry gained a table it never asked for: {written}"
        );
        let back: BarConfig = toml::from_str(&written).expect("round-trips");
        assert_eq!(ids(&back.start), ["clock"]);
    }

    #[test]
    fn an_entry_with_settings_round_trips_through_toml() {
        let cfg: Config =
            toml::from_str("[bars.top]\nstart = [{ id = \"clock\", accent = \"red\" }]\n")
                .expect("config parses");
        let written = toml::to_string_pretty(&cfg.bars.top).expect("serialises");
        let back: BarConfig = toml::from_str(&written).expect("round-trips");
        assert_eq!(back.start, cfg.bars.top.start);
    }

    #[test]
    fn save_section_replaces_one_table_and_preserves_the_rest() {
        let dir = std::env::temp_dir().join(format!("hyprshell-save-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "# hand-written\n[theme]\nname = \"nord\"\naccent = \"cyan\"\n\n[icons]\ndefault_set = \"lucide\"\n",
        )
        .unwrap();

        let theme = ThemeConfig {
            accent: "orange".to_string(),
            ..ThemeConfig::default()
        };
        Config::save_section(&path, "theme", &theme).unwrap();

        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# hand-written"), "top comment survives");
        assert!(
            out.contains("[icons]") && out.contains("lucide"),
            "the untouched section survives"
        );
        let reloaded: Config = toml::from_str(&out).unwrap();
        assert_eq!(reloaded.theme.accent, "orange", "the edited value persisted");
        assert_eq!(
            reloaded.icons.default_set, "lucide",
            "the other section round-trips"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn starter_shows_only_a_top_bar() {
        let cfg = Config::starter();
        assert_eq!(ids(&cfg.bars.top.start), ["workspaces"]);
        assert_eq!(ids(&cfg.bars.top.center), ["clock"]);
        assert!(cfg.bars.bottom.is_empty());
        assert!(cfg.bars.left.is_empty());
        assert!(cfg.bars.right.is_empty());
    }

    #[test]
    fn plain_default_is_all_empty() {
        let cfg = Config::default();
        assert!(cfg.bars.top.is_empty() && cfg.bars.left.is_empty());
    }

    #[test]
    fn partial_config_leaves_unlisted_edges_empty() {
        let toml = r#"
[bars.left]
size = 44
start = ["workspaces"]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.bars.left.size, 44);
        assert_eq!(ids(&cfg.bars.left.start), ["workspaces"]);
        assert!(cfg.bars.top.is_empty());
    }

    #[test]
    fn edge_orientation() {
        assert!(Edge::Top.is_horizontal() && Edge::Bottom.is_horizontal());
        assert!(Edge::Left.is_vertical() && Edge::Right.is_vertical());
    }

    #[test]
    fn shape_defaults_reproduce_todays_bar() {
        let cfg: Config = toml::from_str("[bars.top]\nstart = [\"clock\"]\n").unwrap();
        assert_eq!(cfg.shape.mode, Shape::Bar);
        assert!(!cfg.shape.frame);
        assert_eq!(cfg.shape.gap, 0);
        assert_eq!(cfg.shape.radius, None, "unset radius falls back to the theme");
        let top = cfg.shape_for(Edge::Top);
        assert_eq!(top.mode, Shape::Bar);
        assert_eq!(top.gap, 0);
        assert_eq!(top.radius, 0.0, "the nord theme's default radius is 0");
        assert!(cfg.hugs(Edge::Top));
        assert!(cfg.bar_surface_opaque(Edge::Top));
    }

    #[test]
    fn zone_of_reflects_bar_zones() {
        let toml = r#"
[bars.top]
start = ["workspaces"]
center = ["clock"]
end = ["battery", "volume"]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.zone_of(Edge::Top, "workspaces"), Some(Zone::Start));
        assert_eq!(cfg.zone_of(Edge::Top, "clock"), Some(Zone::Center));
        assert_eq!(cfg.zone_of(Edge::Top, "volume"), Some(Zone::End));
        assert_eq!(cfg.zone_of(Edge::Top, "missing"), None);
        assert_eq!(cfg.zone_of(Edge::Bottom, "clock"), None);
    }

    #[test]
    fn panels_and_open_mode_defaults() {
        let cfg: Config = toml::from_str("[bars.top]\ncenter = [\"clock\"]\n").unwrap();
        assert_eq!(cfg.panels.drawer.width, 320.0);
        assert_eq!(cfg.panels.float.width, 360);
        assert_eq!(cfg.panels.float.height, 240);
        assert_eq!(cfg.panels.gap, None, "gap is derived unless overridden");
        assert_eq!(cfg.open_mode_for("clock"), OpenMode::Drawer);

        let floaty: Config = toml::from_str(
            "[modules.clock]\nopen = \"float\"\n[panels.drawer]\nwidth = 400\n[panels.float]\nwidth = 480\nheight = 320\n",
        )
        .unwrap();
        assert_eq!(floaty.open_mode_for("clock"), OpenMode::Float);
        assert_eq!(floaty.panels.drawer.width, 400.0);
        assert_eq!(floaty.panels.float.width, 480);
        assert_eq!(floaty.panels.float.height, 320);
    }

    #[test]
    fn starter_config_round_trips_through_toml() {
        // load_or_default writes the starter to disk on first run, so it must serialize and re-parse cleanly.
        let starter = Config::starter();
        let text = toml::to_string_pretty(&starter).expect("starter serializes");
        let parsed: Config = toml::from_str(&text).expect("starter re-parses");
        assert_eq!(parsed.panels.drawer.width, starter.panels.drawer.width);
        assert_eq!(parsed.panels.float.width, starter.panels.float.width);
        assert_eq!(parsed.panels.gap, None);
    }

    #[test]
    fn theme_config_overrides_colors_and_numbers() {
        let cfg: Config = toml::from_str(
            "[theme]\nname=\"custom\"\nradius=12\nfont_size=16\n[theme.colors]\nbase=\"#101010\"\naccent=\"#ff8800\"\n",
        )
        .unwrap();
        let theme = cfg.resolve_theme();
        assert_eq!(theme.radius, 12.0);
        assert_eq!(theme.font_size, 16.0);
        assert_eq!(theme.base, Color::from_hex("#101010").unwrap());
        assert_eq!(theme.accent, Color::from_hex("#ff8800").unwrap());
        // An unset token keeps the built-in value.
        assert_eq!(theme.text, NordTheme::new().text);
        // The [theme] number override also backs the shape resolution.
        assert_eq!(cfg.resolved_radius(Edge::Top), 12.0);
    }

    #[test]
    fn theme_config_parses_font_family_and_icon_stroke() {
        let cfg: Config =
            toml::from_str("[theme]\nfont_family = \"JetBrains Mono\"\nicon_stroke = 1.5\n").unwrap();
        // font_family stays in config (applied process-wide, not carried in the Copy theme struct).
        assert_eq!(cfg.theme.font_family.as_deref(), Some("JetBrains Mono"));
        // icon_stroke flows into the resolved theme so icon_view can read it.
        assert_eq!(cfg.resolve_theme().icon_stroke, Some(1.5));
        let bare: Config = toml::from_str("").unwrap();
        assert_eq!(bare.theme.font_family, None);
        assert_eq!(bare.resolve_theme().icon_stroke, None);
    }

    #[test]
    fn spacing_and_radius_fall_back_to_the_theme_then_config_overrides() {
        let theme = NordTheme::new();
        // Nothing set anywhere → the theme's numeric tokens.
        let bare: Config = toml::from_str("[bars.top]\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(bare.resolved_radius(Edge::Top), theme.radius);
        assert_eq!(bare.resolved_spacing(Edge::Top), theme.spacing);
        // Per-bar wins over [shape], which wins over the theme.
        let cfg: Config = toml::from_str(
            "[shape]\nradius=10\nspacing=4\n[bars.top]\ncenter=[\"clock\"]\n[bars.top.shape]\nradius=2\n[bars.bottom]\nstart=[\"clock\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.resolved_radius(Edge::Top), 2.0, "per-bar override wins");
        assert_eq!(cfg.resolved_spacing(Edge::Top), 4.0, "spacing falls to [shape]");
        assert_eq!(cfg.resolved_radius(Edge::Bottom), 10.0, "bottom takes [shape]");
    }

    #[test]
    fn panel_radius_matches_the_bar_on_each_edge() {
        // Per-bar radius override on top, global (0) elsewhere: panels inherit the radius of the bar they hang off.
        let cfg: Config = toml::from_str(
            "[shape]\nradius=0\n[bars.top]\ncenter=[\"clock\"]\n[bars.top.shape]\nradius=8\n[bars.left]\nstart=[\"clock\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.panel_radius(Edge::Top), 8.0);
        assert_eq!(cfg.panel_radius(Edge::Left), 0.0, "left inherits the global radius");
    }

    #[test]
    fn panel_margin_is_a_uniform_gap_and_never_double_counts_the_bar() {
        // The reservation strip already offsets a panel (exclusive_zone=0) past the bar, so the margin is just
        // the gap — adding the bar's reserved thickness here too would put the panel at double the distance.
        let floating: Config =
            toml::from_str("[shape]\ngap=8\n[bars.top]\nsize=34\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(floating.panel_gap(Edge::Top), 8);
        assert_eq!(floating.panel_margin(Edge::Top), (8, 8, 8, 8));

        // Hugging bar with no configured gap still gets the default breathing gap, uniformly.
        let hug: Config = toml::from_str("[bars.top]\nsize=34\ncenter=[\"clock\"]\n").unwrap();
        let d = DEFAULT_PANEL_GAP as i32;
        assert_eq!(hug.panel_margin(Edge::Top), (d, d, d, d));
    }

    #[test]
    fn panels_gap_override_pins_a_fixed_gap_on_every_edge() {
        let cfg: Config = toml::from_str(
            "[shape]\ngap=20\n[panels]\ngap=4\n[bars.top]\ncenter=[\"clock\"]\n[bars.bottom]\nstart=[\"clock\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.panels.gap, Some(4));
        assert_eq!(cfg.panel_gap(Edge::Top), 4, "the override wins over the derived bar gap");
        assert_eq!(cfg.panel_gap(Edge::Bottom), 4);

        let derived: Config = toml::from_str("[shape]\ngap=20\n[bars.top]\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(derived.panel_gap(Edge::Top), 20, "without an override it tracks the bar gap");
    }

    #[test]
    fn clock_format_follows_the_twelve_hour_switch_unless_overridden() {
        let d = ClockConfig::default();
        assert_eq!(d.time_format(), "%H:%M:%S");

        let twelve: Config = toml::from_str("[clock]\ntwelve_hour = true\n").unwrap();
        assert_eq!(twelve.clock.time_format(), "%I:%M:%S %p");

        let explicit: Config =
            toml::from_str("[clock]\ntwelve_hour = true\nformat = \"%H:%M\"\n").unwrap();
        assert_eq!(
            explicit.clock.time_format(),
            "%H:%M",
            "an explicit pattern wins over the switch"
        );
    }

    #[test]
    fn active_window_defaults_bound_the_title() {
        let d: Config = toml::from_str("").unwrap();
        assert_eq!(d.active_window.max_chars, 60);
        assert!(d.active_window.show_icon);
        assert!(!d.active_window.compact);

        let cfg: Config = toml::from_str("[active_window]\ncompact = true\n").unwrap();
        assert!(cfg.active_window.compact);
        assert_eq!(
            cfg.active_window.max_chars, 60,
            "unset fields keep their defaults"
        );
    }

    #[test]
    fn audio_and_brightness_steps_are_configurable_and_bounded() {
        let d: Config = toml::from_str("").unwrap();
        assert_eq!(d.audio.step(), 5);
        assert_eq!(d.audio.ceiling(), 150);
        assert_eq!(d.brightness.step(), 5);

        let cfg: Config = toml::from_str(
            "[audio]\nincrement = 2\nmax_volume = 100\n[brightness]\nincrement = 10\n",
        )
        .unwrap();
        assert_eq!(cfg.audio.step(), 2);
        assert_eq!(cfg.audio.ceiling(), 100);
        assert_eq!(cfg.brightness.step(), 10);

        // A typo must not leave the wheel inert, run it backwards, or let one notch cross the whole range.
        let broken: Config = toml::from_str(
            "[audio]\nincrement = 0\nmax_volume = 10\n[brightness]\nincrement = -5\n",
        )
        .unwrap();
        assert_eq!(broken.audio.step(), 1);
        assert_eq!(broken.audio.ceiling(), 100, "a sink must reach its own maximum");
        assert_eq!(broken.brightness.step(), 1);
    }

    #[test]
    fn temperature_unit_converts_and_labels_the_reading() {
        let d: Config = toml::from_str("").unwrap();
        assert_eq!(d.temperature.unit, TemperatureUnit::Celsius);
        assert!(d.temperature.sensor.is_empty(), "empty tracks the hottest sensor");
        assert_eq!(d.temperature.warn, 70.0);
        assert_eq!(d.temperature.critical, 85.0);
        assert_eq!(d.temperature.unit.format(61.5), "62°C");

        let cfg: Config = toml::from_str(
            "[temperature]\nunit = \"fahrenheit\"\nsensor = \"k10temp\"\nwarn = 80\n",
        )
        .unwrap();
        assert_eq!(cfg.temperature.sensor, "k10temp");
        assert_eq!(cfg.temperature.warn, 80.0);
        assert_eq!(cfg.temperature.critical, 85.0, "unset fields keep their defaults");
        assert_eq!(cfg.temperature.unit.from_celsius(100.0), 212.0);
        assert_eq!(cfg.temperature.unit.format(20.0), "68°F");
    }

    #[test]
    fn lock_status_shows_both_keys_until_told_otherwise() {
        let d: Config = toml::from_str("").unwrap();
        assert!(d.lock_status.caps && d.lock_status.num);
        assert!(
            !d.lock_status.hide_inactive,
            "an indicator nobody can see until they press the key is not discoverable"
        );

        let cfg: Config =
            toml::from_str("[lock_status]\nnum = false\nhide_inactive = true\n").unwrap();
        assert!(cfg.lock_status.caps);
        assert!(!cfg.lock_status.num);
        assert!(cfg.lock_status.hide_inactive);
    }

    #[test]
    fn battery_ships_with_warnings_and_never_acts_unasked() {
        let d: Config = toml::from_str("").unwrap();
        assert!(d.battery.enabled);
        assert_eq!(
            d.battery.warn_levels.iter().map(|w| w.level).collect::<Vec<_>>(),
            vec![20, 10],
            "a laptop shell that silently runs a battery flat is a bug"
        );
        assert_eq!(
            d.battery.critical_level, 0,
            "suspending the machine is opt-in, not a default"
        );
        assert!(d.battery.critical_action.is_empty());

        let cfg: Config = toml::from_str(
            "[battery]\ncritical_level = 3\ncritical_action = \"suspend\"\n\
             [[battery.warn_levels]]\nlevel = 15\ntitle = \"Low\"\ncritical = true\n",
        )
        .unwrap();
        assert_eq!(cfg.battery.critical_level, 3);
        assert_eq!(cfg.battery.critical_action, "suspend");
        assert_eq!(
            cfg.battery.warn_levels.len(),
            1,
            "declaring thresholds replaces the defaults rather than adding to them"
        );
        assert_eq!(cfg.battery.warn_levels[0].title(15), "Low");
    }

    #[test]
    fn a_section_holding_an_array_of_tables_survives_a_save() {
        // `[[battery.warn_levels]]` is the first list-of-tables in the config, and TOML only accepts a table's
        // scalar keys *before* its arrays of tables — a naive serializer would emit `critical_level` inside the
        // last warning. Both the whole-file write and the format-preserving per-section save must get it right.
        let dir = std::env::temp_dir().join(format!("hyprshell-aot-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "# kept\n[theme]\naccent = \"orange\"\n").unwrap();

        let battery = BatteryConfig {
            critical_level: 4,
            critical_action: "suspend".to_string(),
            ..BatteryConfig::default()
        };
        Config::save_section(&path, "battery", &battery).unwrap();

        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.contains("# kept"), "the untouched file survives");
        let reloaded: Config = toml::from_str(&out).expect("what was written parses back");
        assert_eq!(reloaded.battery.critical_level, 4);
        assert_eq!(reloaded.battery.critical_action, "suspend");
        assert_eq!(reloaded.battery.warn_levels.len(), 2);
        assert_eq!(reloaded.battery.warn_levels[1].level, 10);
        assert_eq!(reloaded.theme.accent, "orange", "the other section is untouched");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn workspace_labels_specialise_by_state_and_fall_back_to_the_general_one() {
        let cfg = WorkspacesConfig {
            label: "{id}".to_string(),
            occupied_label: "•{id}".to_string(),
            active_label: "[{id}]".to_string(),
            ..WorkspacesConfig::default()
        };
        assert_eq!(cfg.render_label(3, "3", 2, false, false), "3");
        assert_eq!(cfg.render_label(3, "3", 2, true, false), "•3");
        assert_eq!(cfg.render_label(3, "3", 2, true, true), "[3]");
        assert_eq!(
            cfg.render_label(3, "3", 2, false, true),
            "[3]",
            "the active template wins whether or not the workspace holds windows"
        );

        // Setting only `active_label` leaves every other pill rendering the general template.
        let only_active = WorkspacesConfig {
            active_label: "<{id}>".to_string(),
            ..WorkspacesConfig::default()
        };
        assert_eq!(only_active.render_label(2, "2", 1, true, false), "2");
        assert_eq!(only_active.render_label(2, "2", 1, true, true), "<2>");

        // And an active pill with only `occupied_label` set takes that rather than dropping to `label`.
        let only_occupied = WorkspacesConfig {
            occupied_label: "•{id}".to_string(),
            ..WorkspacesConfig::default()
        };
        assert_eq!(only_occupied.render_label(2, "2", 1, true, true), "•2");
    }

    #[test]
    fn capitalisation_applies_after_the_template() {
        let cfg = WorkspacesConfig {
            label: "{name}".to_string(),
            capitalize: Capitalize::Title,
            ..WorkspacesConfig::default()
        };
        assert_eq!(
            cfg.render_label(1, "my WEB workspace", 0, false, false),
            "My Web Workspace"
        );

        assert_eq!(Capitalize::None.apply("mixed Case"), "mixed Case");
        assert_eq!(Capitalize::Upper.apply("code"), "CODE");
        assert_eq!(Capitalize::Lower.apply("CODE"), "code");
        assert_eq!(
            Capitalize::Title.apply("my-notes  2"),
            "My-notes  2",
            "separators and runs of whitespace survive intact"
        );
        assert_eq!(Capitalize::Title.apply(""), "");
    }

    #[test]
    fn a_glob_anchors_both_ends_and_only_a_star_spans() {
        assert!(glob_matches("nm-applet", "nm-applet"));
        assert!(
            !glob_matches("nm-applet", "nm-applet-2"),
            "a pattern without a star is a whole-string match"
        );
        assert!(glob_matches("steam_app_*", "steam_app_12345"));
        assert!(glob_matches("*applet", "nm-applet"));
        assert!(glob_matches("chrome*icon*", "chrome_status_icon_1"));
        assert!(!glob_matches("chrome*icon", "chrome_status_icon_1"), "a trailing literal anchors the end");
        assert!(glob_matches("*", "anything at all"));
        assert!(glob_matches("NM-Applet", "nm-applet"), "matching ignores case");

        // The two anchors must not overlap: `a*t` needs at least `at`, not just `a`.
        assert!(glob_matches("a*t", "at"));
        assert!(!glob_matches("nm*applet", "nm-apple"));
    }

    #[test]
    fn tray_hiding_and_icon_substitution_match_ids_as_patterns() {
        let cfg: Config = toml::from_str(
            "[tray]\nhidden = [\"steam_app_*\", \"blueman\"]\n\
             [tray.icon_subs]\n\"nm-applet\" = \"mdi:wifi\"\n\"*\" = \"mdi:apps\"\n",
        )
        .unwrap();
        assert!(cfg.tray.is_hidden("steam_app_440"));
        assert!(cfg.tray.is_hidden("blueman"));
        assert!(!cfg.tray.is_hidden("nm-applet"));

        assert_eq!(
            cfg.tray.icon_sub_for("nm-applet"),
            Some("mdi:wifi"),
            "the specific pattern beats the catch-all whatever the map's order"
        );
        assert_eq!(cfg.tray.icon_sub_for("anything-else"), Some("mdi:apps"));
        assert_eq!(TrayConfig::default().icon_sub_for("nm-applet"), None);
    }

    #[test]
    fn the_tray_is_on_by_default_and_hides_nothing() {
        let d: Config = toml::from_str("").unwrap();
        assert!(d.tray.enabled);
        assert!(d.tray.hidden.is_empty() && d.tray.icon_subs.is_empty());
        assert!(
            !d.tray.recolour,
            "tinting every icon would flatten an application that reports state in colour"
        );
        assert!(!d.tray.compact && !d.tray.background);
    }

    #[test]
    fn general_defaults_keep_bars_under_fullscreen_windows() {
        let d: Config = toml::from_str("").unwrap();
        assert!(
            !d.general.show_over_fullscreen,
            "a fullscreen game is meant to cover the bar unless asked otherwise"
        );
        assert!(d.general.logo.is_empty(), "an empty logo means auto-detect");
    }

    #[test]
    fn a_parse_error_is_returned_rather_than_swallowed() {
        let dir = std::env::temp_dir().join(format!("hyprshell-load-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[bars.top\nstart = [\"clock\"]\n").unwrap();

        let error = Config::load(&path).expect_err("a malformed file must not parse");
        assert!(
            matches!(error, LoadError::Parse(_)),
            "the caller needs to distinguish a typo from a missing file"
        );
        // `load_or_default` is the lossy convenience wrapper — it answers a typo with the starter bar, throwing
        // the user's layout away. That is exactly why the running shell uses `load`: so it can keep the last
        // config that worked and report the error instead.
        let lossy = Config::load_or_default(&path);
        assert_eq!(
            lossy.bars.top.start,
            Config::starter().bars.top.start,
            "the wrapper substitutes the starter, losing whatever the user had"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_missing_file_seeds_the_starter_config_on_disk() {
        let dir = std::env::temp_dir().join(format!("hyprshell-seed-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");

        let seeded = Config::load(&path).expect("a fresh install is not an error");
        assert_eq!(ids(&seeded.bars.top.start), ["workspaces"]);
        assert!(path.exists(), "the starter is written for the user to edit");
        assert!(
            Config::load(&path).is_ok(),
            "and what was written parses back"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn notifications_defaults_to_top_right_with_sensible_limits() {
        let d: Config = toml::from_str("").unwrap();
        assert_eq!(d.notifications.edge, Edge::Top);
        assert_eq!(d.notifications.align, Align::End, "align=end is the right side");
        assert_eq!(d.notifications.max_visible, 4);
        assert_eq!(d.notifications.timeout_ms, 5000);
        assert!(d.notifications.critical_sticky);

        let cfg: Config =
            toml::from_str("[notifications]\nmax_visible = 2\ntimeout_ms = 0\nedge = \"bottom\"\n")
                .unwrap();
        assert_eq!(cfg.notifications.max_visible, 2);
        assert_eq!(cfg.notifications.timeout_ms, 0, "0 ms = sticky popups");
        assert_eq!(cfg.notifications.edge, Edge::Bottom);
        assert!(cfg.notifications.critical_sticky, "unset fields keep defaults");
    }

    #[test]
    fn osd_position_parses_edge_and_align() {
        let cfg: Config =
            toml::from_str("[osd]\nedge = \"bottom\"\nalign = \"end\"\ntimeout_ms = 0\n").unwrap();
        assert_eq!(cfg.osd.edge, Edge::Bottom);
        assert_eq!(cfg.osd.align, Align::End);
        assert_eq!(cfg.osd.timeout_ms, 0, "0 ms = no auto-dismiss");
        let d: Config = toml::from_str("").unwrap();
        assert_eq!(d.osd.edge, Edge::Top);
        assert_eq!(d.osd.align, Align::Center);
        assert_eq!(d.osd.timeout_ms, 1200);
    }

    #[test]
    fn partial_override_takes_precedence_field_by_field() {
        let toml = r#"
[shape]
mode = "bar"
gap = 0
spacing = 6
radius = 10

[bars.top]
center = ["clock"]
[bars.top.shape]
mode = "sections"
gap = 8
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        let top = cfg.shape_for(Edge::Top);
        assert_eq!(top.mode, Shape::Sections);
        assert_eq!(top.gap, 8, "gap overridden");
        assert_eq!(top.spacing, 6.0, "spacing inherits the global");
        assert_eq!(top.radius, 10.0, "radius inherits the global");
        let bottom = cfg.shape_for(Edge::Bottom);
        assert_eq!(bottom.mode, Shape::Bar);
        assert_eq!(bottom.gap, 0);
    }

    #[test]
    fn hug_and_opacity_track_gap_and_frame() {
        let toml = r#"
[shape]
gap = 8
radius = 12
[bars.top]
center = ["clock"]
[bars.bottom]
start = ["clock"]
[bars.bottom.shape]
gap = 0
radius = 0
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(!cfg.hugs(Edge::Top));
        assert!(!cfg.bar_surface_opaque(Edge::Top));
        assert!(cfg.hugs(Edge::Bottom));
        assert!(cfg.bar_surface_opaque(Edge::Bottom));
    }

    #[test]
    fn frame_forces_hug_on_every_edge() {
        let toml = r#"
[shape]
frame = true
gap = 8
[bars.top]
center = ["clock"]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert!(cfg.hugs(Edge::Top), "frame forces hug even at gap>0");
    }

    #[test]
    fn derived_padding_and_chip_radius() {
        let s = ResolvedShape {
            mode: Shape::Chips,
            gap: 0,
            spacing: 6.0,
            radius: 12.0,
        };
        assert_eq!(s.padding(), 3.0, "round(6/2)");
        assert_eq!(s.chip_radius(), 9.0, "max(0, 12 - 3)");
        let tight = ResolvedShape {
            mode: Shape::Chips,
            gap: 0,
            spacing: 30.0,
            radius: 4.0,
        };
        assert_eq!(tight.chip_radius(), 0.0, "radius floors at 0, never negative");
    }

    #[test]
    fn module_override_parses_variant_and_accent() {
        let cfg: Config = toml::from_str(
            "[bars.top]\ncenter=[\"clock\"]\n[modules.battery]\nvariant=\"filled\"\naccent=\"orange\"\n",
        )
        .unwrap();
        assert_eq!(cfg.variant_for("battery"), Variant::Filled);
        assert_eq!(cfg.accent_name_for("battery"), "orange");
        assert_eq!(cfg.variant_for("clock"), Variant::Default);
        assert_eq!(cfg.accent_name_for("clock"), "cyan");
    }

    #[test]
    fn corner_owner_prefers_horizontal_then_vertical() {
        let cfg: Config = toml::from_str(
            "[bars.top]\ncenter=[\"clock\"]\n[bars.left]\nstart=[\"workspaces\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.corner_owner(Corner::TopLeft), Some(Edge::Top), "top wins over left");
        assert_eq!(cfg.corner_owner(Corner::BottomLeft), Some(Edge::Left));
        assert_eq!(cfg.corner_owner(Corner::BottomRight), None);
    }

    #[test]
    fn corner_modules_route_to_owning_bar_ends() {
        let cfg: Config = toml::from_str(
            "[bars.top]\ncenter=[\"clock\"]\n[bars.right]\nstart=[\"ws\"]\n\
             [corners]\ntop_left=\"logo\"\nbottom_right=\"tray\"\n",
        )
        .unwrap();
        assert_eq!(cfg.corner_modules_for(Edge::Top), (Some("logo"), None));
        assert_eq!(cfg.corner_modules_for(Edge::Right), (None, Some("tray")));
        assert_eq!(cfg.corner_modules_for(Edge::Left), (None, None));
    }

    #[test]
    fn panel_gap_tracks_the_bar_gap_and_falls_back_when_hugging() {
        let floating: Config =
            toml::from_str("[shape]\ngap=12\n[bars.top]\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(floating.edge_gap(Edge::Top), 12);
        assert_eq!(floating.panel_gap(Edge::Top), 12, "a floating bar's panels float in step");
        assert_eq!(
            floating.edge_reserved(Edge::Top),
            12 + 34,
            "reserved = outer gap + thickness"
        );

        let hugging: Config = toml::from_str("[bars.top]\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(hugging.edge_gap(Edge::Top), 0);
        assert_eq!(
            hugging.panel_gap(Edge::Top),
            DEFAULT_PANEL_GAP,
            "a hugging bar's panels still get a breathing gap"
        );
        assert_eq!(hugging.edge_reserved(Edge::Top), 34);
    }

    #[test]
    fn frame_edge_reserves_thickness_without_a_gap() {
        let cfg: Config =
            toml::from_str("[shape]\nframe=true\ngap=8\n[bars.top]\ncenter=[\"clock\"]\n").unwrap();
        assert_eq!(cfg.edge_gap(Edge::Top), 0, "frame forces a hug, so no outer gap");
        assert_eq!(cfg.edge_reserved(Edge::Top), 34);
        assert_eq!(cfg.panel_gap(Edge::Top), DEFAULT_PANEL_GAP);
    }

    #[test]
    fn frame_gives_empty_edges_inactive_strips() {
        let toml = r#"
[shape]
frame = true
inactive_size = 6
[bars.top]
center = ["clock"]
"#;
        let cfg: Config = toml::from_str(toml).unwrap();
        assert_eq!(cfg.edge_thickness(Edge::Top), 34, "active edge keeps its size");
        assert_eq!(
            cfg.edge_thickness(Edge::Bottom),
            6,
            "empty edge becomes an inactive strip under frame"
        );
    }
}
