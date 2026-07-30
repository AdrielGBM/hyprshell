use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

use rsx::Color;
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, Item};

use crate::shared::paths;
use crate::shared::scheme;
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
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct PanelsConfig {
    /// Gap a panel keeps from the bar and the screen edges. Unset (the default) derives it — the bar's own outer gap when it floats, else [`DEFAULT_PANEL_GAP`] — so panels sit off the bar just like tiled apps; set a value to pin a fixed gap on every edge regardless of the bar.
    pub gap: Option<u32>,
    /// How far a chip must be dragged away from the bar before letting go opens its panel, in px. `0` switches
    /// the gesture off. One threshold for every panel rather than one each: the gesture is the same everywhere
    /// on the bar, and a per-panel distance would make the bar feel inconsistent under the same finger.
    pub drag_threshold: f32,
    /// How opaque a panel's background is, `0`–`1`. `1` (the default) is solid.
    ///
    /// This is also the half of "blurred panels" that belongs to the shell. The blur itself is the
    /// compositor's — hyprshell already names each surface, so Hyprland can be told to blur them:
    ///
    /// ```text
    /// layer_rule = blur, hyprshell-drawer
    /// layer_rule = blur, hyprshell-float
    /// layer_rule = blur, hyprshell-popup
    /// layer_rule = blur, hyprshell-osd
    /// ```
    ///
    /// Drawing the blur here instead would mean copying the screen behind every panel each frame and blurring
    /// it on the CPU, to reproduce something the compositor is already doing on the GPU. What the compositor
    /// cannot do is see through an opaque panel, which is what this key is for: without it the rules above
    /// blur a region nothing shows.
    pub opacity: f32,
    pub drawer: DrawerConfig,
    pub float: FloatConfig,
}

impl Default for PanelsConfig {
    fn default() -> Self {
        Self {
            gap: None,
            drag_threshold: 48.0,
            opacity: 1.0,
            drawer: DrawerConfig::default(),
            float: FloatConfig::default(),
        }
    }
}

impl PanelsConfig {
    /// The drag distance that opens a panel, or `None` when the gesture is off. Floored well above the tap
    /// slop: a threshold a stray press could cross would open a panel every time a chip was clicked slightly
    /// unsteadily.
    pub fn drag_threshold(&self) -> Option<f32> {
        (self.drag_threshold.is_finite() && self.drag_threshold > 0.0)
            .then(|| self.drag_threshold.clamp(16.0, 400.0))
    }
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

/// Whether a popup still appears while a fullscreen window has focus. The three values escalate: `on` never
/// holds anything back, `off` holds back everything but `critical` (don't interrupt a game or a film unless it
/// matters), `never` holds back all of it. Suppression only affects the *popup* — the notification is recorded
/// and waits in the history either way, exactly as Do-Not-Disturb does.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum FullscreenPopups {
    On,
    #[default]
    Off,
    Never,
}

impl FullscreenPopups {
    /// Whether a notification of `urgency` may still pop while a fullscreen window is focused.
    pub fn allows(self, urgency: crate::shared::services::notifications::Urgency) -> bool {
        use crate::shared::services::notifications::Urgency;
        match self {
            Self::On => true,
            Self::Off => urgency == Urgency::Critical,
            Self::Never => false,
        }
    }
}

/// Notification popups: where the stack anchors (defaults to top-right), how many show at once before the rest queue, the per-popup auto-dismiss (`0` = sticky), whether `critical` urgency ignores that timeout, and the card width. Popups follow the focused monitor.
///
/// The history panel's own behaviour lives here too, since it draws the same cards: `group_by_app` collapses an
/// application's notifications under one header with a count, a mute and a clear, showing `group_preview_num`
/// of them until the group is expanded; `action_on_click` makes tapping a card body invoke the notification's
/// `default` action rather than only dismissing it; `body_lines`/`open_expanded` bound (or release) how much of
/// a long body a card shows.
///
/// `sound` is a command run — detached, through `sh -c` — each time a notification actually pops. Empty is
/// silent, which is the default: a shell that started making noise on upgrade would be a bug, and the right
/// command is per-machine (`canberra-gtk-play -i message`, `paplay /usr/share/sounds/…`).
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
    pub fullscreen: FullscreenPopups,
    pub group_by_app: bool,
    pub group_preview_num: u32,
    pub action_on_click: bool,
    /// Lines of body a card shows before ellipsizing. Ignored while `open_expanded` is on.
    pub body_lines: u32,
    pub open_expanded: bool,
    pub sound: String,
    /// How far sideways a card must be dragged before letting go dismisses it, as a fraction of its width.
    /// `0` switches the gesture off, which is what a touchpad user who keeps catching it wants.
    pub clear_threshold: f32,
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
            fullscreen: FullscreenPopups::default(),
            group_by_app: true,
            group_preview_num: 3,
            action_on_click: true,
            body_lines: 4,
            open_expanded: false,
            sound: String::new(),
            clear_threshold: 0.35,
        }
    }
}

impl NotificationsConfig {
    /// How many of a group's cards show before it is expanded. At least one, so a cap of `0` collapses a group
    /// to its header instead of hiding the notifications behind a row that says nothing is there.
    pub fn group_preview(&self) -> usize {
        self.group_preview_num.max(1) as usize
    }

    /// The card's body cap, or `None` when `open_expanded` asks for the whole thing. Clamped so a `0` cannot
    /// render a card with no body at all.
    pub fn body_max_lines(&self) -> Option<u16> {
        (!self.open_expanded).then(|| self.body_lines.clamp(1, 100) as u16)
    }

    /// The command to run when a notification pops, if one is configured.
    pub fn sound_command(&self) -> Option<&str> {
        let command = self.sound.trim();
        (!command.is_empty()).then_some(command)
    }

    /// The swipe distance that dismisses a card, in px for a card `width` wide, or `None` when the gesture is
    /// off. Bounded below the full width: a threshold you cannot reach is a gesture that never fires, which
    /// reads as the card being stuck rather than as the setting being wrong.
    pub fn swipe_distance(&self, width: f32) -> Option<f32> {
        if !self.clear_threshold.is_finite() || self.clear_threshold <= 0.0 {
            return None;
        }
        Some(width * self.clear_threshold.min(0.9))
    }
}

/// How one wallpaper gives way to the next.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WallpaperTransition {
    /// The new image simply replaces the old one.
    None,
    /// Cross-fade.
    #[default]
    Fade,
    /// The new image sweeps across from one side.
    Wipe,
}

impl WallpaperTransition {
    pub const ALL: [WallpaperTransition; 3] = [
        WallpaperTransition::None,
        WallpaperTransition::Fade,
        WallpaperTransition::Wipe,
    ];

    pub fn id(self) -> &'static str {
        match self {
            WallpaperTransition::None => "none",
            WallpaperTransition::Fade => "fade",
            WallpaperTransition::Wipe => "wipe",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        match id.trim().to_ascii_lowercase().as_str() {
            "none" => Some(WallpaperTransition::None),
            "fade" => Some(WallpaperTransition::Fade),
            "wipe" => Some(WallpaperTransition::Wipe),
            _ => None,
        }
    }
}

/// Full-screen wallpaper behind everything, one surface per monitor. Off by default so the compositor's own background shows through; setting an `image` — or `enabled = true` for a plain themed background — turns it on. `[background.monitors]` maps output names to per-monitor images, each falling back to the global `image`, and `hyprshell wallpaper set` overrides both at runtime. Paths may use `~`.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BackgroundConfig {
    pub enabled: bool,
    pub image: Option<PathBuf>,
    pub monitors: HashMap<String, PathBuf>,
    /// How a change from one wallpaper to the next is drawn: `fade` (the default), `wipe` or `none`.
    pub transition: WallpaperTransition,
    /// How long that transition runs, before `[animation] duration_scale`. Ignored while `[animation] enabled` is off, which makes every change instant.
    pub transition_ms: u64,
    pub clock: DesktopClockConfig,
}

impl Default for BackgroundConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            image: None,
            monitors: HashMap::new(),
            transition: WallpaperTransition::default(),
            transition_ms: 600,
            clock: DesktopClockConfig::default(),
        }
    }
}

impl BackgroundConfig {
    /// Whether hyprshell paints a background surface at all; opt-in so it never clobbers the compositor's wallpaper unless asked (an image, a per-monitor entry or the desktop clock implies it).
    pub fn is_enabled(&self) -> bool {
        self.enabled
            || self.image.is_some()
            || !self.monitors.is_empty()
            || self.clock.enabled
    }

    /// The image `[background]` alone would paint on `output`: its per-monitor entry, else the global `image`.
    /// The runtime override lives in the wallpaper service, so read
    /// [`wallpaper::current_image`](crate::shared::services::wallpaper::current_image) rather than this at a
    /// call site that draws.
    pub fn image_for(&self, output: Option<&str>) -> Option<&PathBuf> {
        output
            .and_then(|name| self.monitors.get(name))
            .or(self.image.as_ref())
    }
}

/// Where on the screen a widget on the background surface sits.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Placement {
    TopLeft,
    TopCenter,
    TopRight,
    CenterLeft,
    #[default]
    Center,
    CenterRight,
    BottomLeft,
    BottomCenter,
    BottomRight,
}

impl Placement {
    pub const ALL: [Placement; 9] = [
        Placement::TopLeft,
        Placement::TopCenter,
        Placement::TopRight,
        Placement::CenterLeft,
        Placement::Center,
        Placement::CenterRight,
        Placement::BottomLeft,
        Placement::BottomCenter,
        Placement::BottomRight,
    ];

    pub fn id(self) -> &'static str {
        match self {
            Placement::TopLeft => "top_left",
            Placement::TopCenter => "top_center",
            Placement::TopRight => "top_right",
            Placement::CenterLeft => "center_left",
            Placement::Center => "center",
            Placement::CenterRight => "center_right",
            Placement::BottomLeft => "bottom_left",
            Placement::BottomCenter => "bottom_center",
            Placement::BottomRight => "bottom_right",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Placement::ALL
            .into_iter()
            .find(|placement| placement.id() == id.trim().to_ascii_lowercase().replace('-', "_"))
    }

    /// The row and column this placement occupies, as `flex` alignment values.
    pub fn alignment(self) -> (Align, Align) {
        let vertical = match self {
            Placement::TopLeft | Placement::TopCenter | Placement::TopRight => Align::Start,
            Placement::CenterLeft | Placement::Center | Placement::CenterRight => Align::Center,
            _ => Align::End,
        };
        let horizontal = match self {
            Placement::TopLeft | Placement::CenterLeft | Placement::BottomLeft => Align::Start,
            Placement::TopCenter | Placement::Center | Placement::BottomCenter => Align::Center,
            _ => Align::End,
        };
        (vertical, horizontal)
    }
}

/// A clock drawn on the wallpaper itself (`[background.clock]`), the way a lock screen or a phone's home screen
/// shows one. Off by default. `format`/`date_format` fall back to `[clock]`, so the desktop face and the bar
/// chip read the same unless one is deliberately given its own.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct DesktopClockConfig {
    pub enabled: bool,
    /// One of the nine positions: `top_left` … `bottom_right`, `center` being the default.
    pub position: Placement,
    /// Multiplies the theme's display size, so the face can be made as large as the screen allows.
    pub scale: f32,
    /// How far the face is kept from the screen edges, in px.
    pub margin: u32,
    /// Draw the face in the theme's base colour instead of its text colour — for a pale wallpaper, where light
    /// text disappears.
    pub invert: bool,
    pub show_date: bool,
    /// Overrides `[clock] format` for the desktop face only. A desktop clock usually wants `%H:%M` where the bar chip wants seconds.
    pub format: Option<String>,
    /// Overrides `[clock] date_format` for the desktop face only.
    pub date_format: Option<String>,
    /// Paint a plate behind the face, so it stays legible over a busy photograph.
    pub background: bool,
    /// How opaque that plate is, `0`–`1`.
    pub background_opacity: f32,
    /// How far the plate's edge is feathered into the wallpaper, in px. `0` gives a hard-edged card.
    pub background_blur: f32,
    /// Drop a shadow under the glyphs, which is what keeps a plateless face readable over a light wallpaper.
    pub shadow: bool,
}

impl Default for DesktopClockConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            position: Placement::Center,
            scale: 3.0,
            margin: 48,
            invert: false,
            show_date: true,
            format: None,
            date_format: None,
            background: false,
            background_opacity: 0.35,
            background_blur: 0.0,
            shadow: true,
        }
    }
}

impl DesktopClockConfig {
    /// The `strftime` pattern the face renders: its own override, else `[clock]`'s answer without seconds — a
    /// desktop clock that ticks every second is a wallpaper that repaints every second.
    pub fn time_format<'a>(&'a self, clock: &'a ClockConfig) -> &'a str {
        if let Some(format) = &self.format {
            return format;
        }
        if clock.format.is_some() {
            return clock.time_format();
        }
        if clock.twelve_hour { "%I:%M %p" } else { "%H:%M" }
    }

    pub fn date_format<'a>(&'a self, clock: &'a ClockConfig) -> &'a str {
        self.date_format.as_deref().unwrap_or(&clock.date_format)
    }

    /// The plate's fill, bounded so `background = true` cannot resolve to an invisible plate or a fully opaque
    /// one the user did not ask for.
    pub fn plate_opacity(&self) -> f32 {
        if self.background_opacity.is_finite() {
            self.background_opacity.clamp(0.05, 1.0)
        } else {
            0.35
        }
    }

    pub fn resolved_scale(&self) -> f32 {
        if self.scale.is_finite() {
            self.scale.clamp(0.5, 20.0)
        } else {
            1.0
        }
    }
}

/// The wallpaper library: which folder is browsed and how (`[wallpaper]`). The folder itself is `[paths] wallpapers`, so the two settings that name a directory stay in one place.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct WallpaperConfig {
    /// Whether the library is scanned at all. Off means `[background] image` is the only wallpaper the shell knows, which is what it did before the library existed.
    pub enabled: bool,
    /// Descend into sub-folders. On, because a wallpaper collection is almost always filed by theme or by artist.
    pub recursive: bool,
    /// How many images the library holds at most, so pointing it at a picture archive cannot cost a scan of the whole disk.
    pub max_entries: u32,
    /// The edge length of a cached thumbnail, in px.
    pub thumbnail_size: u32,
    /// The file extensions counted as wallpapers, lowercase and without the dot.
    pub extensions: Vec<String>,
}

impl Default for WallpaperConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            recursive: true,
            max_entries: 2000,
            thumbnail_size: 320,
            extensions: ["png", "jpg", "jpeg", "webp"]
                .into_iter()
                .map(str::to_string)
                .collect(),
        }
    }
}

impl WallpaperConfig {
    /// Whether `path` names a file the library should list.
    pub fn accepts(&self, path: &Path) -> bool {
        let Some(extension) = path.extension().and_then(|e| e.to_str()) else {
            return false;
        };
        let extension = extension.to_ascii_lowercase();
        self.extensions
            .iter()
            .any(|allowed| allowed.trim().trim_start_matches('.').eq_ignore_ascii_case(&extension))
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
    ///
    /// Superseded by `[general.apps] terminal`, and still read when that one is unset — a config written
    /// before the section existed keeps working rather than silently reverting to `xterm`.
    pub terminal: String,
    pub apps: AppsConfig,
}

/// The real applications the shell hands off to (`[general.apps]`).
///
/// Every "open the real thing" affordance — a volume card's mixer button, a recording's folder, a media card
/// opening its player — needs a command, and a shell that guessed one per affordance would be unconfigurable.
/// One section, one command each, resolved through [`Config::app_command`].
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct AppsConfig {
    pub terminal: String,
    pub file_manager: String,
    pub audio_mixer: String,
    pub media_player: String,
    pub browser: String,
    pub editor: String,
}

/// A well-known helper application the shell can hand off to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HelperApp {
    Terminal,
    FileManager,
    AudioMixer,
    MediaPlayer,
    Browser,
    Editor,
}

impl HelperApp {
    /// The command to run when nothing is configured. Each is either the freedesktop indirection that works
    /// everywhere (`xdg-open`) or the near-universal tool for the job; a machine without it gets a failed
    /// launch and a log line, which is better than an affordance that silently does nothing.
    fn fallback(self) -> &'static str {
        match self {
            Self::Terminal => "xterm",
            Self::FileManager => "xdg-open",
            Self::AudioMixer => "pavucontrol",
            Self::MediaPlayer => "xdg-open",
            Self::Browser => "xdg-open",
            Self::Editor => "xdg-open",
        }
    }
}

/// The `active_window` module. `compact` shows the app's class instead of the document title — stable while you
/// move around inside one app, and much narrower. `max_chars` bounds the one bar value with no natural size: a
/// browser tab title can be a paragraph, and letting it size the chip would push every other module off the bar.
///
/// `inverted` puts the icon after the title instead of before it. Which reads better depends on where the chip
/// sits: leading the icon points into the bar from the left, and trailing it does the same from the right, so
/// a chip in the end zone usually wants this on.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct ActiveWindowConfig {
    pub compact: bool,
    pub show_icon: bool,
    pub inverted: bool,
    pub max_chars: u32,
}

impl Default for ActiveWindowConfig {
    fn default() -> Self {
        Self {
            compact: false,
            show_icon: true,
            inverted: false,
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
    /// Tint a pill that holds windows differently from an empty one. Ignored while `indicator` is on, which
    /// needs every pill transparent to slide under them; the label colour carries occupancy there.
    pub occupied_background: bool,
    /// Mark the active workspace with one box that slides between pills instead of recolouring each pill in
    /// place. Off restores the older look exactly — the pill paints its own accent and nothing moves.
    pub indicator: bool,
    /// How far the indicator stretches along its direction of travel, as a fraction of the distance still to
    /// cover. `0` keeps it exactly one pill wide the whole way; the default gives it a little speed.
    pub indicator_trail: f32,
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
            indicator: true,
            indicator_trail: 0.35,
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
    /// The trail fraction, bounded: at `1` the box would reach the whole way to its goal on every frame and
    /// read as one long bar rather than as a pill in motion, and the indicator has to be off entirely for a
    /// trail to mean nothing.
    pub fn trail(&self) -> f32 {
        if !self.indicator || !self.indicator_trail.is_finite() {
            return 0.0;
        }
        self.indicator_trail.clamp(0.0, 0.9)
    }

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
    /// Control external monitors over DDC/CI with `ddcutil`, so a desktop with no backlight is dimmable at all. Costs one detection (a few seconds, on the service's own thread) at startup; does nothing when ddcutil is not installed.
    pub external: bool,
}

impl Default for BrightnessConfig {
    fn default() -> Self {
        Self {
            increment: 5,
            external: true,
        }
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

/// The lock screen (`[lock]`): what it authenticates against, and what it shows while it waits.
///
/// The screen only comes up on a compositor that implements `ext-session-lock-v1` and with a PAM library the
/// shell can load. Both are checked *before* the lock is taken, because the failure mode of finding out
/// afterwards is a user staring at a screen with no way back in.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct LockConfig {
    /// The PAM service to authenticate against — a file under `/etc/pam.d`. Empty picks the first of
    /// `hyprshell`, `swaylock`, `login` that exists, so a machine with no hyprshell-specific stack still
    /// unlocks instead of refusing every password.
    pub pam_service: String,
    /// Where `libpam` is. Empty tries `libpam.so.0`, `libpam.so` and NixOS's
    /// `/run/current-system/sw/lib/libpam.so.0`, which between them cover every machine met so far; set it only
    /// if `hyprshell lock status` says the library could not be loaded.
    pub pam_library: String,
    /// Attempts before the field locks itself out for `lockout_seconds`. `0` never locks out.
    pub max_tries: u32,
    pub lockout_seconds: u64,
    /// Verify a fingerprint through fprintd alongside the password, when a reader is enrolled.
    pub fingerprint: bool,
    pub max_fprint_tries: u32,
    /// The Howdy face-unlock command, run with the user name appended; empty disables it. Exit status 0 is a
    /// successful match, as Howdy's own PAM module treats it.
    pub howdy_command: String,
    pub max_howdy_tries: u32,
    /// Attempt face unlock as soon as the lock screen appears, rather than only when asked.
    pub trigger_on_wake: bool,
    /// Lock before the machine suspends, so the screen is already covered when it wakes.
    pub lock_before_sleep: bool,
    pub show_avatar: bool,
    pub show_media: bool,
    pub show_weather: bool,
    pub show_resources: bool,
    pub show_notifications: bool,
    /// Start with the notification dock collapsed — the lock screen is the one surface where a stranger can
    /// read what arrived without unlocking.
    pub hide_notifs: bool,
}

impl Default for LockConfig {
    fn default() -> Self {
        Self {
            pam_service: String::new(),
            pam_library: String::new(),
            max_tries: 5,
            lockout_seconds: 30,
            fingerprint: false,
            max_fprint_tries: 3,
            howdy_command: String::new(),
            max_howdy_tries: 3,
            trigger_on_wake: false,
            lock_before_sleep: true,
            show_avatar: true,
            show_media: true,
            show_weather: false,
            show_resources: false,
            show_notifications: true,
            // Bodies hidden by default: the count and the app are enough to know something arrived.
            hide_notifs: true,
        }
    }
}

/// One idle timeout, declared as an `[[idle.stages]]` table: what to run once the seat has been idle that long,
/// and what to run when it stops being.
///
/// Both actions are request lines the shell already answers — the same strings `hyprshell` takes on the command
/// line — so a stage needs no new vocabulary and anything bindable to a key is bindable to a timeout. `hyprshell
/// --list` is the full menu; `lock on`, `shell dpms off` and `session do suspend` are the usual three.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct IdleStage {
    pub timeout: u64,
    pub action: String,
    /// Run when the seat wakes, if this stage had fired. Empty leaves the action standing — which is right for
    /// a lock and wrong for a blanked screen, so the dpms stage below pairs them.
    pub return_action: String,
}

impl Default for IdleStage {
    fn default() -> Self {
        Self {
            timeout: 300,
            action: String::new(),
            return_action: String::new(),
        }
    }
}

impl IdleStage {
    pub fn duration(&self) -> std::time::Duration {
        std::time::Duration::from_secs(self.timeout.max(1))
    }
}

/// Idle behaviour (`[idle]`): the timeouts, and what keeps them from firing.
///
/// `respect_inhibitors` is not a condition the shell evaluates — it selects which question is asked of the
/// compositor. `ext-idle-notify-v1` has one request that stays quiet while any client holds an idle inhibitor
/// and another that reports raw input idleness, and the compositor is the only thing that can tell them apart.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct IdleConfig {
    pub enabled: bool,
    pub stages: Vec<IdleStage>,
    /// Hold every stage while something is playing audio — a film should not be interrupted by a lock screen.
    pub inhibit_when_audio: bool,
    /// Hold every stage while the machine is on mains power.
    pub inhibit_when_charging: bool,
    /// Honour idle inhibitors taken out by other applications.
    pub respect_inhibitors: bool,
}

impl Default for IdleConfig {
    fn default() -> Self {
        Self {
            // Off out of the box: a shell that locks a machine the user never asked it to lock is a bug, and
            // the timeouts below are a starting point rather than a policy anyone consented to.
            enabled: false,
            stages: vec![
                IdleStage {
                    timeout: 300,
                    action: "lock on".to_string(),
                    return_action: String::new(),
                },
                IdleStage {
                    timeout: 360,
                    action: "shell dpms off".to_string(),
                    return_action: "shell dpms on".to_string(),
                },
            ],
            inhibit_when_audio: true,
            inhibit_when_charging: false,
            respect_inhibitors: true,
        }
    }
}

/// The network (`[network]`): the wireless list its panel shows.
///
/// `rescan_seconds` is a trade, not a preference: an access point that goes out of range emits nothing, so only
/// a fresh scan notices it left. But a scan takes the radio off its channel, which on a busy link is a visible
/// stutter and on some drivers worse — so the background interval is deliberately slow, and the moment that
/// actually needs a fresh list (opening the panel) triggers a scan of its own. Turn it down only if a stale
/// list bothers you more than the scans do.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct NetworkConfig {
    /// Switches the NetworkManager layer off entirely: no D-Bus connection, no rescan timer, no threads. The
    /// bar chip keeps working — it reads sysfs and never needed NetworkManager.
    pub enabled: bool,
    pub rescan_seconds: u32,
    /// Upper bound on the networks the panel lists.
    pub max_networks: u32,
    /// List networks that broadcast no SSID. Off by default: a hidden network cannot be joined by picking it
    /// out of a list anyway, so it is a row that can only disappoint.
    pub show_hidden: bool,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            rescan_seconds: 300,
            max_networks: 20,
            show_hidden: false,
        }
    }
}

impl NetworkConfig {
    /// Clamped on read, with a floor well above what a config typo could reach: a rescan every few seconds
    /// keeps the radio off its channel often enough to hurt the connection it is scanning from.
    pub fn rescan(&self) -> Duration {
        Duration::from_secs(self.rescan_seconds.clamp(60, 3600) as u64)
    }

    pub fn network_limit(&self) -> usize {
        self.max_networks.max(1) as usize
    }
}

/// Timed lyrics (`[lyrics]`).
///
/// Where hand-kept `.lrc` files live is `[paths] lyrics`, with every other folder the shell owns — a file sitting
/// next to the audio track is found without either. `online` is the only part of the feature that asks a third party
/// anything (it sends the artist, title, album and length of what is playing to LRCLIB), so it is a switch of its
/// own rather than part of `enabled`.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct LyricsConfig {
    pub enabled: bool,
    /// Look the words up on LRCLIB when there is no local file. Sends the track's tags to that service.
    pub online: bool,
}

impl Default for LyricsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            online: true,
        }
    }
}

/// The weather (`[weather]`).
///
/// `location` is a place name (`"Madrid"`), geocoded once; `latitude`/`longitude` skip that step. With none of
/// them set the service asks an IP-geolocation endpoint where this connection is — which is the only part of
/// the feature that tells a third party anything, and setting either of the others avoids it.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct WeatherConfig {
    pub enabled: bool,
    pub location: String,
    pub latitude: Option<f32>,
    pub longitude: Option<f32>,
    /// Minutes between refreshes. Clamped on read: the forecast changes hourly, and hammering a free service
    /// every few seconds is how a shell gets its users rate-limited.
    pub refresh_minutes: u32,
    pub forecast_days: u32,
}

impl Default for WeatherConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            location: String::new(),
            latitude: None,
            longitude: None,
            refresh_minutes: 15,
            forecast_days: 7,
        }
    }
}

impl WeatherConfig {
    /// The configured point, when both halves of it are given. One without the other is not a location, so it
    /// falls through to the place name rather than being read as a point on the equator.
    pub fn coordinates(&self) -> Option<crate::shared::services::weather::Coordinates> {
        Some(crate::shared::services::weather::Coordinates {
            latitude: self.latitude?,
            longitude: self.longitude?,
        })
    }

    pub fn refresh(&self) -> Duration {
        Duration::from_secs(self.refresh_minutes.clamp(5, 24 * 60) as u64 * 60)
    }

    /// Open-Meteo serves up to 16 days; asking for none would make the daily block empty.
    pub fn forecast_days(&self) -> u32 {
        self.forecast_days.clamp(1, 16)
    }
}

/// One page of the dashboard. Named in `[dashboard] tabs`, so the ids are the config surface and stay stable
/// regardless of the UI language — the same reason [`Condition::id`](crate::shared::services::weather::Condition::id) is not the translated label.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DashboardTab {
    #[default]
    Dash,
    Media,
    Performance,
    Weather,
}

impl DashboardTab {
    pub const ALL: [DashboardTab; 4] = [Self::Dash, Self::Media, Self::Performance, Self::Weather];

    pub fn id(self) -> &'static str {
        match self {
            Self::Dash => "dash",
            Self::Media => "media",
            Self::Performance => "performance",
            Self::Weather => "weather",
        }
    }

    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|tab| tab.id() == id.trim())
    }

    /// The glyph the tab strip draws.
    pub fn icon(self) -> &'static str {
        match self {
            Self::Dash => "layout-dashboard",
            Self::Media => "disc-3",
            Self::Performance => "activity",
            Self::Weather => "cloud-sun",
        }
    }
}

/// The dashboard surface (`[dashboard]`).
///
/// The two intervals are here rather than on the services they read because the cost is the dashboard's, not
/// theirs: the performance cards redraw a sparkline per tick, and the playhead is an MPRIS property no player
/// signals — following it means asking for it. A bar chip's own rate is unaffected either way, and neither
/// ticker exists while the dashboard is closed.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct DashboardConfig {
    /// Which pages the dashboard offers, in order; an id it doesn't know is dropped with a warning rather than
    /// failing the whole config parse, which would cost the user every other section over one typo.
    pub tabs: Vec<String>,
    /// Milliseconds between playhead reads while the media card is up.
    pub media_update_interval: u64,
    /// Milliseconds between resource refreshes in the performance cards.
    pub resource_update_interval: u64,
    /// Which column the calendar starts on: `monday`, `sunday` or `saturday`.
    pub first_day_of_week: String,
    /// The image the user card shows. Empty means the usual places — `~/.face`, `~/.face.icon`, then the
    /// AccountsService icon a display manager writes.
    pub avatar: String,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            tabs: DashboardTab::ALL
                .iter()
                .map(|t| t.id().to_string())
                .collect(),
            media_update_interval: 500,
            resource_update_interval: 1000,
            first_day_of_week: "monday".to_string(),
            avatar: String::new(),
        }
    }
}

impl DashboardConfig {
    /// The configured pages, unknown ids warned about and dropped. An empty list falls back to all of them: a
    /// dashboard with no pages is a surface that opens onto nothing.
    pub fn tabs(&self) -> Vec<DashboardTab> {
        let resolved: Vec<DashboardTab> = self
            .tabs
            .iter()
            .filter_map(|id| match DashboardTab::from_id(id) {
                Some(tab) => Some(tab),
                None => {
                    tracing::warn!("unknown dashboard tab '{id}'");
                    None
                }
            })
            .collect();
        if resolved.is_empty() {
            return DashboardTab::ALL.to_vec();
        }
        resolved
    }

    /// Clamped on read: below ~100 ms a playhead poll is a D-Bus round-trip per frame for a number that moves
    /// one pixel, and above a few seconds the scrubber visibly lags the audio.
    pub fn media_interval(&self) -> Duration {
        Duration::from_millis(self.media_update_interval.clamp(100, 5_000))
    }

    /// Clamped to the resource service's own tick at the fast end — asking more often than it publishes cannot
    /// produce a new reading, only more repaints.
    pub fn resource_interval(&self) -> Duration {
        Duration::from_millis(self.resource_update_interval.clamp(1_000, 60_000))
    }

    pub fn first_weekday(&self) -> chrono::Weekday {
        match self.first_day_of_week.trim().to_ascii_lowercase().as_str() {
            "sunday" | "sun" => chrono::Weekday::Sun,
            "saturday" | "sat" => chrono::Weekday::Sat,
            _ => chrono::Weekday::Mon,
        }
    }
}

/// Where the shell reads and writes user content (`[paths]`).
///
/// Every entry is empty by default, meaning "work it out": the wallpaper, screenshot and recording directories
/// resolve through the user's own XDG directories, so they land in `Imágenes/Capturas` on a Spanish desktop
/// rather than in a `Pictures` nobody has. `~` is expanded on read.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct PathsConfig {
    pub wallpapers: String,
    pub lyrics: String,
    pub recordings: String,
    pub screenshots: String,
    /// Searched before the shell's built-in assets, so one can be substituted without rebuilding.
    pub assets: String,
}

/// The graphics processor (`[gpu]`).
///
/// `backend` is `auto` (the default), one of `amd` / `intel` / `nvidia`, or `none` to switch the service off
/// entirely. Forcing one matters on a laptop with switchable graphics: the kernel lists the integrated GPU
/// first, and "the first card with a driver we can read" is then the wrong one. `card` names the `drm` entry
/// (`card1`) directly, which is the only way to tell two cards from the same vendor apart.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct GpuConfig {
    pub enabled: bool,
    pub backend: String,
    pub card: String,
}

impl Default for GpuConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            backend: "auto".to_string(),
            card: String::new(),
        }
    }
}

/// Bluetooth (`[bluetooth]`): the chip and the device list its panel shows.
///
/// `scan_on_open` is what makes "pair something new" one gesture instead of two — a panel opened to find a
/// device starts looking, and stops when it closes, so no scan outlives the surface that asked for it.
/// `show_unnamed` is off because a scan in a public place turns up dozens of devices BlueZ knows only an
/// address for, and a list of addresses is not a list anyone can choose from.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct BluetoothConfig {
    pub enabled: bool,
    pub scan_on_open: bool,
    /// Upper bound on the rows the panel lists, so a busy room can't grow the panel past the screen.
    pub max_devices: u32,
    pub show_unnamed: bool,
}

impl Default for BluetoothConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            scan_on_open: true,
            max_devices: 12,
            show_unnamed: false,
        }
    }
}

impl BluetoothConfig {
    /// At least one row, so a misconfigured cap cannot produce an empty panel with devices behind it.
    pub fn device_limit(&self) -> usize {
        self.max_devices.max(1) as usize
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
    /// Show the calculator's answer above the app matches when the query reads as arithmetic. Unit conversions (`3 km in mi`) count as arithmetic.
    pub calculator: bool,
    /// Fall back to `qalc` (Qalculate) for an explicit `=` query the built-in evaluator cannot answer — currencies, constants, dates. Silently does nothing when qalc is not installed.
    pub qalc: bool,
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
            qalc: true,
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
    /// What the wheel over the chip does: `volume`, `track`, `seek`, or `none`.
    pub scroll: MediaScroll,
    /// Scroll a title longer than `max_chars` instead of cutting it. Off by default: a bar that never moves is
    /// easier to read past, and a marquee costs a repaint per step for as long as the track is playing.
    pub marquee: bool,
    /// Milliseconds per character of marquee travel.
    pub marquee_speed_ms: u32,
    /// Seconds the wheel moves the playhead per notch, when `scroll = "seek"`.
    pub seek_seconds: u32,
    pub aliases: HashMap<String, String>,
}

impl Default for MediaConfig {
    fn default() -> Self {
        Self {
            preferred_player: String::new(),
            max_chars: 40,
            scroll: MediaScroll::default(),
            marquee: false,
            marquee_speed_ms: 220,
            seek_seconds: 5,
            aliases: HashMap::new(),
        }
    }
}

impl MediaConfig {
    /// Clamped on read: below about 60 ms the text is a blur, and above a few seconds it reads as stuck.
    pub fn marquee_step(&self) -> Duration {
        Duration::from_millis(self.marquee_speed_ms.clamp(60, 2000) as u64)
    }

    /// The wheel's seek step in microseconds, which is MPRIS's unit.
    pub fn seek_micros(&self) -> i64 {
        self.seek_seconds.clamp(1, 600) as i64 * 1_000_000
    }
}

#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum MediaScroll {
    #[default]
    Volume,
    Track,
    /// Move the playhead. Needs a player that reports `CanSeek`; on one that does not, the wheel does nothing
    /// rather than pretending.
    Seek,
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

/// Screenshots (`[screenshot]`).
///
/// `backend` is `auto` (the default), `screencopy` to insist on the Wayland protocol, or `grim` to insist on the
/// tool. `auto` prefers the protocol — it needs nothing installed and hands the shell the pixels rather than a
/// file — and falls back to `grim` on a compositor that does not implement it.
///
/// `annotator` is the command a saved capture is handed to, with `{file}` where the path goes (appended when the
/// command does not name it): `satty --filename {file}`, `swappy -f`. Empty means the capture is simply saved.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ScreenshotConfig {
    /// Put every capture on the clipboard, as well as saving it.
    pub copy: bool,
    pub save: bool,
    pub include_cursor: bool,
    /// Hold the last frame on screen while a region is being selected, so a menu or a hover state can be
    /// captured without disappearing the moment the overlay takes the pointer.
    pub freeze: bool,
    /// Say where the file went, through the shell's own notification daemon.
    pub notify: bool,
    pub backend: String,
    /// `strftime` pattern for the file's stem; the extension is always `.png`.
    pub file_name: String,
    pub annotator: String,
}

impl Default for ScreenshotConfig {
    fn default() -> Self {
        Self {
            copy: true,
            save: true,
            include_cursor: false,
            freeze: true,
            notify: true,
            backend: "auto".to_string(),
            file_name: "screenshot_%Y-%m-%d_%H-%M-%S".to_string(),
            annotator: String::new(),
        }
    }
}

impl ScreenshotConfig {
    fn backend_id(&self) -> String {
        self.backend.trim().to_ascii_lowercase()
    }

    /// Whether `grim` is the route to take first, because the user asked for it by name.
    pub fn prefers_grim(&self) -> bool {
        self.backend_id() == "grim"
    }

    /// Whether a failed protocol capture may fall back to `grim`. `screencopy` means "this route or none": a
    /// user who names a backend is usually debugging one, and a silent fallback is what hides the answer.
    pub fn may_use_grim(&self) -> bool {
        self.backend_id() != "screencopy"
    }

    pub fn has_annotator(&self) -> bool {
        !self.annotator.trim().is_empty()
    }
}

/// Screen recording (`[recorder]`).
///
/// `backend` is `auto`, `wf-recorder` or `gpu-screen-recorder`; `auto` takes whichever is installed. Neither is a
/// dependency of the shell — with no recorder present the controls grey out rather than failing on the press.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct RecorderConfig {
    pub backend: String,
    pub audio: bool,
    /// The PipeWire node to record from; empty is the session's default output.
    pub audio_device: String,
    pub fps: u32,
    /// `strftime` pattern for the file's stem; the backend decides the container.
    pub file_name: String,
    pub notify: bool,
    /// How many recordings the list in the utilities panel shows.
    pub max_entries: u32,
}

impl Default for RecorderConfig {
    fn default() -> Self {
        Self {
            backend: "auto".to_string(),
            audio: false,
            audio_device: String::new(),
            fps: 60,
            file_name: "recording_%Y-%m-%d_%H-%M-%S".to_string(),
            notify: true,
            max_entries: 12,
        }
    }
}

impl RecorderConfig {
    /// Clamped to what an encoder will accept: a `0` here would be a recorder that writes no frames, and a
    /// four-figure rate is a typo rather than a request.
    pub fn fps(&self) -> u32 {
        self.fps.clamp(1, 240)
    }

    pub fn entries(&self) -> usize {
        self.max_entries.clamp(1, 200) as usize
    }
}

/// Which in-shell toasts to show (`[toasts.events]`).
///
/// One switch per event rather than a single `enabled`, because the useful set is personal: the point of a toast
/// is that it tells you something you would otherwise miss, and a toast about something you already know is
/// noise. Every one is on by default except the two that fire most often.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct ToastEvents {
    pub config_loaded: bool,
    pub charging: bool,
    pub game_mode: bool,
    pub dnd: bool,
    pub audio_output: bool,
    pub audio_input: bool,
    /// Caps Lock and Num Lock — the one piece of state a keyboard changes and never reports.
    pub lock_keys: bool,
    pub kb_layout: bool,
    pub vpn: bool,
    /// Off by default: a music player already says what it is playing, and every skipped track would be a toast.
    pub now_playing: bool,
    pub screenshot: bool,
    pub recording: bool,
}

impl Default for ToastEvents {
    fn default() -> Self {
        Self {
            config_loaded: true,
            charging: true,
            game_mode: true,
            dnd: true,
            audio_output: true,
            audio_input: true,
            lock_keys: true,
            kb_layout: true,
            vpn: true,
            now_playing: false,
            screenshot: false,
            recording: true,
        }
    }
}

/// In-shell toasts (`[toasts]`): the transient messages the shell says about itself.
///
/// Not notifications. A notification belongs to an application, goes into history and waits under
/// Do-Not-Disturb; "Caps Lock is on" is feedback about a key that was just pressed and is worthless a second
/// later. See `shared::services::toaster`.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ToastsConfig {
    pub enabled: bool,
    pub edge: Edge,
    pub align: Align,
    pub max_toasts: u32,
    pub timeout_ms: u64,
    pub width: f32,
    pub gap: f32,
    pub events: ToastEvents,
}

impl Default for ToastsConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            edge: Edge::Bottom,
            align: Align::Center,
            max_toasts: 3,
            timeout_ms: 2500,
            width: 300.0,
            gap: 8.0,
            events: ToastEvents::default(),
        }
    }
}

impl ToastsConfig {
    /// Whether `event` should reach the screen: toasts on at all, and this event not switched off.
    pub fn allows(&self, event: crate::shared::services::toaster::Event) -> bool {
        use crate::shared::services::toaster::Event;
        self.enabled
            && match event {
                Event::ConfigLoaded => self.events.config_loaded,
                Event::Charging => self.events.charging,
                Event::GameMode => self.events.game_mode,
                Event::Dnd => self.events.dnd,
                Event::AudioOutput => self.events.audio_output,
                Event::AudioInput => self.events.audio_input,
                Event::LockKeys => self.events.lock_keys,
                Event::KbLayout => self.events.kb_layout,
                Event::Vpn => self.events.vpn,
                Event::NowPlaying => self.events.now_playing,
                Event::Screenshot => self.events.screenshot,
                Event::Recording => self.events.recording,
            }
    }

    /// How long a toast stays. Floored rather than allowed to be zero: a toast that expires on the frame it was
    /// posted is a feature that looks broken.
    pub fn lifetime(&self) -> std::time::Duration {
        std::time::Duration::from_millis(self.timeout_ms.clamp(400, 60_000))
    }

    pub fn visible(&self) -> usize {
        self.max_toasts.clamp(1, 10) as usize
    }
}

/// The notification centre (`[sidebar]`): a full-height surface that is the home for the notification history and
/// the quick toggles.
///
/// Distinct from the bell drawer, which is a glance: this is where a user goes to *deal with* what has arrived,
/// so it takes the whole edge, scrolls, and hosts the utilities panel's own toggles rather than a second set.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SidebarConfig {
    pub edge: Edge,
    /// Width for a left/right sidebar, height for a top/bottom one, in px.
    pub size: u32,
    pub show_toggles: bool,
    pub show_history: bool,
}

impl Default for SidebarConfig {
    fn default() -> Self {
        Self {
            edge: Edge::Right,
            size: 400,
            show_toggles: true,
            show_history: true,
        }
    }
}

impl SidebarConfig {
    /// Clamped so a hand-edited `size` cannot produce a sidebar too narrow to read or one that covers the screen.
    pub fn thickness(&self) -> u32 {
        self.size.clamp(240, 1200)
    }
}

/// The utilities panel (`[utilities]`): the quick toggles it lists, and in which order.
///
/// `toggles` is a list of ids rather than a switch per toggle, because the order is the point — the toggles a
/// user reaches for live at the front. Unknown ids are dropped with a warning rather than failing the config, so
/// a name from a newer build costs a line in the log instead of the whole panel.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct UtilitiesConfig {
    pub toggles: Vec<String>,
    /// Show the capture (screenshot / recorder) card under the toggles.
    pub show_capture: bool,
    /// Show the recordings list under the capture controls.
    pub show_recordings: bool,
    /// How many columns the toggle grid uses.
    pub columns: u32,
    /// How often the window info panel re-captures its preview, in ms. `0` takes one still and leaves it, which
    /// is what a machine on battery wants — the preview is a screen capture per refresh.
    pub window_preview_ms: u64,
}

impl Default for UtilitiesConfig {
    fn default() -> Self {
        Self {
            toggles: [
                "wifi",
                "bluetooth",
                "mic",
                "dnd",
                "game_mode",
                "vpn",
                "idle_inhibit",
                "settings",
            ]
            .into_iter()
            .map(str::to_string)
            .collect(),
            show_capture: true,
            show_recordings: true,
            columns: 4,
            window_preview_ms: 1000,
        }
    }
}

impl UtilitiesConfig {
    pub fn grid_columns(&self) -> usize {
        self.columns.clamp(1, 8) as usize
    }

    /// The preview's refresh period, or `None` for a single still. Floored well above a frame: each refresh is a
    /// compositor round trip and a full-window copy, and asking for it 60 times a second would cost more than
    /// the panel showing it.
    pub fn window_preview_interval(&self) -> Option<Duration> {
        (self.window_preview_ms > 0)
            .then(|| Duration::from_millis(self.window_preview_ms.clamp(250, 60_000)))
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    /// The schema the file was written against. `0` (the default) is anything written before versioning
    /// existed; [`migrate`] brings it forward on load. See [`CONFIG_VERSION`].
    pub version: u32,
    /// Design-token overrides read from the sibling `tokens.toml`, not from `config.toml` — skipped from
    /// serialization so a section save can never write them into the user's config file.
    #[serde(skip)]
    pub tokens: TokenOverrides,
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
    pub toasts: ToastsConfig,
    pub screenshot: ScreenshotConfig,
    pub recorder: RecorderConfig,
    pub utilities: UtilitiesConfig,
    pub sidebar: SidebarConfig,
    pub background: BackgroundConfig,
    pub wallpaper: WallpaperConfig,
    pub active_window: ActiveWindowConfig,
    pub clock: ClockConfig,
    pub media: MediaConfig,
    pub lyrics: LyricsConfig,
    pub workspaces: WorkspacesConfig,
    pub launcher: LauncherConfig,
    pub audio: AudioConfig,
    pub brightness: BrightnessConfig,
    pub temperature: TemperatureConfig,
    pub battery: BatteryConfig,
    pub lock_status: LockStatusConfig,
    pub lock: LockConfig,
    pub idle: IdleConfig,
    pub status_icons: StatusIconsConfig,
    pub network: NetworkConfig,
    pub bluetooth: BluetoothConfig,
    pub gpu: GpuConfig,
    pub weather: WeatherConfig,
    pub dashboard: DashboardConfig,
    pub paths: PathsConfig,
    pub tray: TrayConfig,
    pub animation: AnimationConfig,
    pub keynav: KeyNavConfig,
    pub modules: HashMap<String, ModuleOverride>,
}

/// One bar per screen edge; empty bars collapse to zero. Default is all-empty by design (serde fills missing fields), so configs get only what they specify — see [`Config::starter`] for the initial setup.
///
/// `excluded_screens` names outputs that get no bars at all — a TV, a projector, a monitor that only ever shows
/// one fullscreen thing. Each entry matches the connector name (`DP-1`) as a `*` pattern, so `HDMI-*` covers a
/// port whose index moves between reboots. *Which* modules a screen shows is a per-monitor config override
/// (`monitors/<output>/config.toml`) rather than a key here: it is the same `[bars.<edge>]` shape, so there is
/// nothing new to learn and nothing to keep in step.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct BarsConfig {
    pub excluded_screens: Vec<String>,
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

    /// Whether this output should carry no bars. An output the compositor gave no name to is never excluded —
    /// there is nothing to match it by, and dropping the bars off an unnameable screen would look like a bug.
    pub fn excludes(&self, output: Option<&str>) -> bool {
        let Some(output) = output else {
            return false;
        };
        self.excluded_screens
            .iter()
            .any(|pattern| glob_matches(pattern, output))
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

/// The design tokens themselves, overridable from `~/.config/hyprshell/tokens.toml`.
///
/// **Unstable, and deliberately so.** `[theme]` is the supported surface: it names the handful of knobs a
/// theme is *meant* to expose, and those keys will keep working. This file reaches past that into the token
/// set the shell draws from, which exists to serve the widgets and moves when they do — a token can be
/// renamed or dropped in any release. It is here because a user building a palette wants every number in one
/// place without waiting for each to grow a config key, not because it is a stable API.
///
/// Applied last in [`Config::resolve_theme`], after `[theme]` and after `[theme.scale]`, so it always wins.
#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct TokenOverrides {
    pub radius: Option<f32>,
    pub spacing: Option<f32>,
    pub font_size: Option<f32>,
    pub icon_size: Option<f32>,
    pub icon_stroke: Option<f32>,
    /// Palette tokens by the same names [`NordTheme::accent_by_name`](crate::NordTheme) uses.
    pub colors: HashMap<String, String>,
}

impl TokenOverrides {
    /// Reads `tokens.toml` from the config directory. A missing file is the normal case and reads as "no
    /// overrides"; an unparseable one is warned about and ignored, because a token file is a garnish and must
    /// never be the reason a shell refuses to start.
    pub fn load(config_path: &Path) -> Self {
        let path = Self::path(config_path);
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        match toml::from_str(&text) {
            Ok(tokens) => tokens,
            Err(e) => {
                tracing::warn!("{}: {e}; ignoring the token overrides", path.display());
                Self::default()
            }
        }
    }

    pub fn path(config_path: &Path) -> PathBuf {
        config_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("tokens.toml")
    }

    fn is_empty(&self) -> bool {
        self.radius.is_none()
            && self.spacing.is_none()
            && self.font_size.is_none()
            && self.icon_size.is_none()
            && self.icon_stroke.is_none()
            && self.colors.is_empty()
    }

    /// Stamps these overrides onto a resolved theme.
    fn apply(&self, theme: &mut NordTheme) {
        if let Some(r) = self.radius {
            theme.radius = r;
        }
        if let Some(s) = self.spacing {
            theme.spacing = s;
        }
        if let Some(f) = self.font_size {
            theme.font_size = f;
        }
        if let Some(i) = self.icon_size {
            theme.icon_size = i;
        }
        if self.icon_stroke.is_some() {
            theme.icon_stroke = self.icon_stroke;
        }
        for (name, hex) in &self.colors {
            match Color::from_hex(hex) {
                Some(c) => *theme = theme.with_color(name, c),
                None => tracing::warn!("token color '{name}': invalid hex '{hex}'"),
            }
        }
    }
}

/// One text role's overrides (`[theme.fonts.<role>]`), each unset by default so a role keeps the size the
/// theme derives for it.
///
/// No `family`: rsx's `TextStyle` carries no font family — the family is process-wide, applied through
/// `rsx::set_default_font_family` from `[theme] font_family`. Per-role families need `TextStyle` to carry one
/// and the renderer to select on it, which is an upstream change rather than a config key.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FontSpec {
    pub size: Option<f32>,
    pub weight: Option<u16>,
    pub italic: Option<bool>,
}

impl FontSpec {
    /// The size for this role: the override, bounded to what a screen can actually render, else `derived`.
    pub(crate) fn size_for(self, derived: f32) -> f32 {
        self.size
            .filter(|s| s.is_finite())
            .map(|s| s.clamp(4.0, 200.0))
            .unwrap_or(derived)
    }
}

/// Per-role text overrides (`[theme.fonts]`). The roles are the ones the shell actually draws with, so there is
/// no role here that nothing reads.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default, PartialEq)]
#[serde(default)]
pub struct FontsConfig {
    pub display: FontSpec,
    pub title: FontSpec,
    pub body: FontSpec,
    pub caption: FontSpec,
}

/// Keyboard navigation shared by every list surface (`[keynav]`).
///
/// `vim` is off by default and has to be: the launcher's list sits under a search field, and a list that reads
/// `j` as "down" cannot also let you type `jitsi`. Turning it on is a deliberate trade a vim user makes
/// knowingly — the arrows keep working either way.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct KeyNavConfig {
    pub vim: bool,
}

/// How the shell moves (`[animation]`).
///
/// Two curve families rather than one, because rsx has two motion models and they answer different questions.
/// `curve` names a **spring**, for motion that chases a target that can move mid-flight — the workspace
/// indicator, which has to bend its path when you hold a workspace key rather than restart. `easing` names a
/// **timing function**, for a transition with a start, an end and a duration — a panel opening.
///
/// `duration_scale` multiplies every duration at once, so "make it all a bit quicker" is one number; `enabled
/// = false` collapses every duration to zero, which is the accessibility answer (and what a user on a remote
/// desktop wants) rather than a per-surface opt-out.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct AnimationConfig {
    pub enabled: bool,
    pub duration_scale: f32,
    /// The spring preset for continuous motion: `gentle`, `snappy` or `bouncy`.
    pub curve: String,
    /// The timing function for duration-based transitions: `linear`, `ease-in`, `ease-out`, `ease-in-out`.
    pub easing: String,
    /// How long a panel takes to enter or leave, before `duration_scale`.
    pub panel_duration_ms: u64,
}

impl Default for AnimationConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            duration_scale: 1.0,
            curve: "gentle".to_string(),
            easing: "ease-out".to_string(),
            panel_duration_ms: 180,
        }
    }
}

impl AnimationConfig {
    /// The multiplier, bounded: `0` (or a negative, or NaN) would make every animation instant by accident
    /// rather than by the `enabled` switch that says so, and an unbounded one makes the shell feel broken.
    fn scale(&self) -> f32 {
        if self.duration_scale.is_finite() {
            self.duration_scale.clamp(0.1, 10.0)
        } else {
            1.0
        }
    }

    /// `base` scaled by `duration_scale`, or zero while animation is off. The one place a duration is derived,
    /// so every surface shortens and lengthens together instead of each carrying its own constant.
    pub fn duration(&self, base: Duration) -> Duration {
        if !self.enabled {
            return Duration::ZERO;
        }
        base.mul_f32(self.scale())
    }

    /// The spring every chase-a-moving-target animation uses.
    pub fn spring(&self) -> rsx::motion::Spring {
        match self.curve.trim().to_ascii_lowercase().as_str() {
            "snappy" => rsx::motion::Spring::snappy(),
            "bouncy" => rsx::motion::Spring::bouncy(),
            _ => rsx::motion::Spring::gentle(),
        }
    }

    /// The timing function every duration-based transition uses.
    pub fn easing(&self) -> rsx::motion::Easing {
        match self.easing.trim().to_ascii_lowercase().as_str() {
            "linear" => rsx::motion::Easing::Linear,
            "ease-in" | "ease_in" => rsx::motion::Easing::EaseIn,
            "ease-in-out" | "ease_in_out" => rsx::motion::Easing::EaseInOut,
            _ => rsx::motion::Easing::EaseOut,
        }
    }

    /// A panel's enter/exit transition, ready to hand to `Animated`.
    pub fn panel_tween(&self) -> rsx::motion::Tween {
        self.tween_ms(self.panel_duration_ms, 2_000)
    }

    /// A tween of `base_ms`, scaled and eased by `[animation]`, and bounded by `max_ms` so a mistyped duration
    /// is a slow transition rather than one that never ends. The general form `panel_tween` is a preset of.
    pub fn tween_ms(&self, base_ms: u64, max_ms: u64) -> rsx::motion::Tween {
        rsx::motion::tween(
            self.duration(Duration::from_millis(base_ms.clamp(0, max_ms))),
            self.easing(),
        )
    }
}

/// Proportional multipliers over the theme's numeric tokens (`[theme.scale]`).
///
/// An absolute override answers "what should the radius be"; a scale answers "make everything a bit rounder",
/// which is the question a user actually has and the one that keeps a palette's proportions intact. Applied
/// last in [`Config::resolve_theme`], so scaling a token the user also pinned scales *their* number, not the
/// palette's — otherwise the two settings would silently fight.
///
/// `font` scales the base size every other role steps off, so one number moves all the text at once.
#[derive(Deserialize, Serialize, Clone, Copy, Debug)]
#[serde(default)]
pub struct ScaleConfig {
    pub rounding: f32,
    pub spacing: f32,
    pub font: f32,
    pub icon: f32,
}

impl Default for ScaleConfig {
    fn default() -> Self {
        Self {
            rounding: 1.0,
            spacing: 1.0,
            font: 1.0,
            icon: 1.0,
        }
    }
}

impl ScaleConfig {
    /// Whether every multiplier is the identity, so `resolve_theme` can skip the whole step — and so a config
    /// that never mentions scaling reads exactly as it did before the section existed.
    fn is_identity(self) -> bool {
        [self.rounding, self.spacing, self.font, self.icon]
            .iter()
            .all(|f| *f == 1.0)
    }

    /// A multiplier bounded away from the two ways it breaks a surface: `0` (or negative, or NaN) collapses
    /// what it scales to nothing, and an unbounded one grows a chip past the screen it sits on.
    fn factor(value: f32) -> f32 {
        if value.is_finite() {
            value.clamp(0.25, 4.0)
        } else {
            1.0
        }
    }
}

/// Theme selection and overrides. `name` picks a built-in palette, `custom`, or `dynamic` (a palette generated from the current wallpaper); the rest override individual tokens on top of it — numbers directly, `[theme.scale]` proportionally, and `[theme.colors]` per-token hex (`base = "#2e3440"`), keyed by the same names [`NordTheme::accent_by_name`] uses. Any unset field keeps the built-in's value.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub accent: String,
    /// `dark`, `light`, or `auto` (the default) to keep whatever the named palette already is. A built-in with
    /// a sibling in the asked-for mode switches to it (`gruvbox` ↔ `gruvbox-light`); one without keeps its own.
    pub mode: String,
    /// How much colour a `dynamic` scheme carries: `vibrant` (the default), `content`, `expressive`, `fidelity`
    /// or `muted`. Ignored by the built-in palettes, which carry their own.
    pub variant: String,
    /// The palette a `dynamic` theme falls back to before a wallpaper has been quantised — on the very first
    /// start, or with no wallpaper set at all.
    pub fallback: String,
    pub export: SchemeExportConfig,
    pub radius: Option<u32>,
    pub spacing: Option<u32>,
    pub font_size: Option<f32>,
    pub icon_size: Option<f32>,
    /// Font family the whole shell renders in (must be installed). Unset keeps the renderer's default. Applied process-wide via [`rsx::set_default_font_family`], not carried in the (`Copy`) theme struct.
    pub font_family: Option<String>,
    /// Stroke width forced on stroke-based icon glyphs (e.g. `1.5`). Unset keeps each glyph's own stroke.
    pub icon_stroke: Option<f32>,
    pub scale: ScaleConfig,
    pub fonts: FontsConfig,
    pub colors: HashMap<String, String>,
}

/// Where the resolved palette is written for the rest of the desktop to read (`[theme.export]`).
///
/// A wallpaper-driven scheme is only worth having if the applications around the shell follow it, and none of
/// them reads `config.toml`. Each switch writes one flat file of the same tokens into `dir`: `scheme.json`,
/// `scheme.css` (GTK `@define-color`), `scheme.conf` (an ini for Qt/Kvantum themes) and `scheme.sh` plus
/// `sequences` (shell variables and the OSC escapes that recolour a running terminal). `hooks` are commands run
/// once the files are on disk, which is where a `gsettings`/`makoctl reload` belongs.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct SchemeExportConfig {
    pub enabled: bool,
    /// Where the files land. Empty means the shell's own config directory, next to `config.toml`.
    pub dir: String,
    pub json: bool,
    pub gtk: bool,
    pub qt: bool,
    pub terminal: bool,
    pub hooks: Vec<String>,
}

impl Default for SchemeExportConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            dir: String::new(),
            json: true,
            gtk: true,
            qt: false,
            terminal: false,
            hooks: Vec::new(),
        }
    }
}

impl SchemeExportConfig {
    pub fn resolved_dir(&self) -> PathBuf {
        let configured = self.dir.trim();
        if configured.is_empty() {
            return Config::default_path()
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_default();
        }
        paths::expand_tilde(Path::new(configured))
    }
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "nord".to_string(),
            accent: "cyan".to_string(),
            mode: "auto".to_string(),
            variant: "vibrant".to_string(),
            fallback: "nord".to_string(),
            export: SchemeExportConfig::default(),
            radius: None,
            spacing: None,
            font_size: None,
            icon_size: None,
            font_family: None,
            icon_stroke: None,
            scale: ScaleConfig::default(),
            fonts: FontsConfig::default(),
            colors: HashMap::new(),
        }
    }
}

impl ThemeConfig {
    /// Whether this config asks for a wallpaper-derived palette rather than a built-in one.
    pub fn is_dynamic(&self) -> bool {
        self.name.trim().eq_ignore_ascii_case(scheme::DYNAMIC)
    }

    /// The mode asked for, or `None` for `auto` — "keep whatever the palette already is".
    pub fn requested_mode(&self) -> Option<scheme::Mode> {
        scheme::Mode::from_id(&self.mode)
    }

    pub fn requested_variant(&self) -> scheme::Variant {
        scheme::Variant::from_id(&self.variant).unwrap_or_default()
    }
}

impl Config {
    /// How long a wallpaper transition actually runs, after `[animation]` has had its say. Zero when animation
    /// is off or the transition is `none`, which is what makes "wait for the picture to settle" a no-op there
    /// instead of a pause with nothing happening in it.
    pub fn wallpaper_transition(&self) -> Duration {
        if self.background.transition == WallpaperTransition::None {
            return Duration::ZERO;
        }
        self.animation
            .duration(Duration::from_millis(self.background.transition_ms.min(10_000)))
    }

    /// The mode and variant a dynamic scheme is generated at.
    ///
    /// `auto` resolves through the fallback palette rather than to a hardcoded dark: a user whose fallback is
    /// Catppuccin Latte has already said which end of the ramp they live at, and asking them to say it twice is
    /// how the two settings end up disagreeing.
    pub fn scheme_selection(&self) -> (scheme::Mode, scheme::Variant) {
        let mode = self.theme.requested_mode().unwrap_or_else(|| {
            scheme::Mode::of(&NordTheme::named(&self.theme.fallback))
        });
        (mode, self.theme.requested_variant())
    }

    /// Fresh-install starter config (distinct from `Default`, which is all-empty and backs serde's missing-field fill).
    pub fn starter() -> Self {
        Self {
            version: CONFIG_VERSION,
            tokens: TokenOverrides::default(),
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
            toasts: ToastsConfig::default(),
            screenshot: ScreenshotConfig::default(),
            recorder: RecorderConfig::default(),
            utilities: UtilitiesConfig::default(),
            sidebar: SidebarConfig::default(),
            background: BackgroundConfig::default(),
            wallpaper: WallpaperConfig::default(),
            active_window: ActiveWindowConfig::default(),
            clock: ClockConfig::default(),
            media: MediaConfig::default(),
            lyrics: LyricsConfig::default(),
            workspaces: WorkspacesConfig::default(),
            launcher: LauncherConfig::default(),
            audio: AudioConfig::default(),
            brightness: BrightnessConfig::default(),
            temperature: TemperatureConfig::default(),
            battery: BatteryConfig::default(),
            lock_status: LockStatusConfig::default(),
            lock: LockConfig::default(),
            idle: IdleConfig::default(),
            status_icons: StatusIconsConfig::default(),
            network: NetworkConfig::default(),
            bluetooth: BluetoothConfig::default(),
            gpu: GpuConfig::default(),
            weather: WeatherConfig::default(),
            dashboard: DashboardConfig::default(),
            paths: PathsConfig::default(),
            tray: TrayConfig::default(),
            animation: AnimationConfig::default(),
            keynav: KeyNavConfig::default(),
            modules: HashMap::new(),
            general: GeneralConfig::default(),
        }
    }

    /// The command for a well-known helper application: `[general.apps]`, else the legacy `[general] terminal`
    /// for the terminal, else the built-in fallback. Resolved here rather than at each call site so every
    /// affordance that opens a real application agrees on which one that is.
    pub fn app_command(&self, which: HelperApp) -> String {
        let apps = &self.general.apps;
        let configured = match which {
            HelperApp::Terminal => &apps.terminal,
            HelperApp::FileManager => &apps.file_manager,
            HelperApp::AudioMixer => &apps.audio_mixer,
            HelperApp::MediaPlayer => &apps.media_player,
            HelperApp::Browser => &apps.browser,
            HelperApp::Editor => &apps.editor,
        };
        let configured = configured.trim();
        if !configured.is_empty() {
            return configured.to_string();
        }
        if which == HelperApp::Terminal && !self.general.terminal.trim().is_empty() {
            return self.general.terminal.trim().to_string();
        }
        which.fallback().to_string()
    }

    /// The wallpaper library. Falls back to a `Wallpapers` folder inside the user's own pictures directory.
    pub fn wallpaper_dir(&self) -> PathBuf {
        self.resolved_path(&self.paths.wallpapers, || {
            paths::user_dir("XDG_PICTURES_DIR", "Pictures").join("Wallpapers")
        })
    }

    /// Where local `.lrc` files are looked up. Not a user-content directory by convention, so it defaults
    /// inside the shell's own data directory rather than inventing a folder in `$HOME`.
    pub fn lyrics_dir(&self) -> PathBuf {
        self.resolved_path(&self.paths.lyrics, || paths::data_dir().join("lyrics"))
    }

    pub fn recordings_dir(&self) -> PathBuf {
        self.resolved_path(&self.paths.recordings, || {
            paths::user_dir("XDG_VIDEOS_DIR", "Videos").join("Recordings")
        })
    }

    pub fn screenshot_dir(&self) -> PathBuf {
        self.resolved_path(&self.paths.screenshots, || {
            paths::user_dir("XDG_PICTURES_DIR", "Pictures").join("Screenshots")
        })
    }

    /// A directory searched before the shell's built-in assets, when one is configured.
    pub fn assets_dir(&self) -> Option<PathBuf> {
        let configured = self.paths.assets.trim();
        (!configured.is_empty()).then(|| paths::expand_tilde(Path::new(configured)))
    }

    fn resolved_path(&self, configured: &str, fallback: impl FnOnce() -> PathBuf) -> PathBuf {
        let configured = configured.trim();
        if configured.is_empty() {
            return fallback();
        }
        paths::expand_tilde(Path::new(configured))
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

    /// The palette `[theme] name` selects, before any override: the wallpaper's for `dynamic`, else the built-in
    /// — switched to its light or dark sibling when `[theme] mode` asks for one it has.
    ///
    /// A dynamic theme with nothing extracted yet resolves to `[theme] fallback` rather than to a blank or a
    /// hardcoded default, so the first frame after an install is already the palette the user asked to fall back
    /// to instead of a colour scheme they never chose.
    fn base_palette(&self) -> NordTheme {
        if self.theme.is_dynamic() {
            return scheme::theme()
                .unwrap_or_else(|| self.in_requested_mode(&self.theme.fallback));
        }
        self.in_requested_mode(&self.theme.name)
    }

    fn in_requested_mode(&self, name: &str) -> NordTheme {
        match self.theme.requested_mode() {
            Some(mode) => NordTheme::named(NordTheme::in_mode(name, mode)),
            None => NordTheme::named(name),
        }
    }

    /// The theme this config selects, with every `[theme]` override applied — accent, numeric tokens, and per-token `[theme.colors]` hex. The single place a theme is resolved, so its tokens back the config defaults everywhere.
    pub fn resolve_theme(&self) -> NordTheme {
        let t = &self.theme;
        let mut theme = self.base_palette().with_accent(&t.accent);
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
        theme.fonts = t.fonts;
        for (name, hex) in &t.colors {
            match Color::from_hex(hex) {
                Some(c) => theme = theme.with_color(name, c),
                None => tracing::warn!("theme color '{name}': invalid hex '{hex}'"),
            }
        }
        // Last, and over the absolute overrides above: a scale means "relative to the size I chose", so applying it first would leave a pinned token unscaled and the two settings disagreeing.
        if !t.scale.is_identity() {
            theme.radius *= ScaleConfig::factor(t.scale.rounding);
            theme.spacing = (theme.spacing * ScaleConfig::factor(t.scale.spacing)).round();
            theme.font_size *= ScaleConfig::factor(t.scale.font);
            theme.icon_size *= ScaleConfig::factor(t.scale.icon);
        }
        // `tokens.toml` reaches past the supported `[theme]` surface, so it is applied after everything else and always wins — a user editing raw tokens has said which answer they want.
        if !self.tokens.is_empty() {
            self.tokens.apply(&mut theme);
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

    /// The background every panel paints, at the configured `[panels] opacity`.
    ///
    /// Floored well above transparent: a panel faded past readability is indistinguishable from one that
    /// failed to open, and the user's next move is to file a bug rather than to reach for the setting.
    pub fn panel_fill(&self) -> Color {
        let theme = self.resolve_theme();
        let opacity = if self.panels.opacity.is_finite() {
            self.panels.opacity.clamp(0.2, 1.0)
        } else {
            1.0
        };
        theme.surface.with_alpha(opacity)
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
        let mut document: toml::Value = toml::from_str(&text).map_err(LoadError::Parse)?;
        migrate(&mut document);
        let mut config: Config = document.try_into().map_err(LoadError::Parse)?;
        config.tokens = TokenOverrides::load(path);
        Ok(config)
    }

    /// The config as `output` sees it: `config.toml` with `monitors/<output>/config.toml` deep-merged over it.
    ///
    /// A merge rather than a replacement, so a per-monitor file says only what differs — a vertical bar on the
    /// second screen is four lines, not a copy of the whole config that then drifts. Tables merge key by key;
    /// anything else (a scalar, an array, a module list) replaces outright, because a half-overridden array is
    /// not something a user can predict.
    ///
    /// Sections in [`GLOBAL_ONLY_SECTIONS`] are dropped from the override with a warning: one process owns them,
    /// so honouring them per monitor would be a setting that silently did nothing on every screen but one.
    pub fn for_output(path: &Path, output: Option<&str>) -> Result<Self, LoadError> {
        let Some(output) = output else {
            return Config::load(path);
        };
        let override_path = monitor_config_path(path, output);
        let Ok(override_text) = std::fs::read_to_string(&override_path) else {
            return Config::load(path);
        };
        let base_text = std::fs::read_to_string(path).map_err(LoadError::Io)?;
        let mut merged: toml::Value = toml::from_str(&base_text).map_err(LoadError::Parse)?;
        let mut over: toml::Value = toml::from_str(&override_text).map_err(LoadError::Parse)?;
        migrate(&mut merged);
        migrate(&mut over);
        if let Some(table) = over.as_table_mut() {
            for section in GLOBAL_ONLY_SECTIONS {
                if table.remove(*section).is_some() {
                    tracing::warn!(
                        "{}: [{section}] is global-only and was ignored",
                        override_path.display()
                    );
                }
            }
        }
        merge_into(&mut merged, over);
        let mut config: Config = merged.try_into().map_err(LoadError::Parse)?;
        config.tokens = TokenOverrides::load(path);
        Ok(config)
    }

    /// Where a monitor's override lives: `<config dir>/monitors/<output>/config.toml`.
    pub fn monitor_dir(path: &Path) -> PathBuf {
        path.parent()
            .unwrap_or(Path::new("."))
            .join("monitors")
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
        keep_subtables_with_their_parent(&mut doc);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(SaveError::Io)?;
        }
        std::fs::write(path, doc.to_string()).map_err(SaveError::Io)
    }
}

/// Re-numbers every table in `doc` so each one renders where its key sits, with its children under it.
///
/// `toml_edit` carries a render position on every table it *parsed*, and a table built from scratch has none —
/// so replacing `[theme]` with a value carrying `[theme.scale]`, `[theme.export]` and `[theme.fonts.*]` scattered
/// those children through the file between unrelated sections, and printed `[theme]` itself *after* its own
/// children. The result still parses, which is why nothing caught it; it also destroys the layout of a file this
/// function promises to preserve.
///
/// Walking the document once and handing out positions in key order puts every child back under its parent
/// without touching any decor, so the comments and key order the caller was promised survive.
fn keep_subtables_with_their_parent(doc: &mut DocumentMut) {
    fn walk(table: &mut toml_edit::Table, next: &mut isize) {
        for (_, item) in table.iter_mut() {
            match item {
                Item::Table(child) => {
                    child.set_position(Some(*next));
                    *next += 1;
                    walk(child, next);
                }
                // A list of tables (`[[idle.stages]]`) renders with its parent already; only its own children
                // need positions, and it has none.
                Item::ArrayOfTables(_) | Item::Value(_) | Item::None => {}
            }
        }
    }
    let mut next: isize = 0;
    walk(doc.as_table_mut(), &mut next);
}

/// The schema this build writes. A file carrying an older `version` is brought forward by [`migrate`] before
/// it is deserialized; one carrying a *newer* version is read as-is, since guessing at a future schema is how a
/// downgrade destroys a config.
pub const CONFIG_VERSION: u32 = 1;

/// Brings an older config document forward to [`CONFIG_VERSION`], in memory.
///
/// In memory, and never on disk: a shell that silently rewrites the file a user hand-edits is a shell they stop
/// trusting, and the format-preserving save path ([`Config::save_section`]) already writes the current shape
/// whenever they change something. Every step is therefore written to be idempotent — running it against an
/// already-migrated document must be a no-op — so a file that never gets rewritten keeps working forever.
fn migrate(document: &mut toml::Value) {
    let from = document
        .get("version")
        .and_then(toml::Value::as_integer)
        .unwrap_or(0)
        .clamp(0, i64::from(u32::MAX)) as u32;
    if from >= CONFIG_VERSION {
        return;
    }
    if from < 1 {
        migrate_terminal_into_apps(document);
    }
    tracing::info!("config migrated from version {from} to {CONFIG_VERSION}");
}

/// v0 → v1: `[general] terminal` became `[general.apps] terminal` when the other helper applications arrived.
/// The older key wins nothing if the newer one is set, so a config carrying both keeps the deliberate value.
fn migrate_terminal_into_apps(document: &mut toml::Value) {
    let Some(general) = document.get_mut("general").and_then(toml::Value::as_table_mut) else {
        return;
    };
    let Some(legacy) = general.get("terminal").and_then(toml::Value::as_str) else {
        return;
    };
    let legacy = legacy.to_string();
    if legacy.trim().is_empty() {
        return;
    }
    let apps = general
        .entry("apps")
        .or_insert_with(|| toml::Value::Table(toml::map::Map::new()));
    let Some(apps) = apps.as_table_mut() else {
        return;
    };
    let already_set = apps
        .get("terminal")
        .and_then(toml::Value::as_str)
        .is_some_and(|t| !t.trim().is_empty());
    if !already_set {
        apps.insert("terminal".to_string(), toml::Value::String(legacy));
    }
}

/// Sections one process owns, and which a per-monitor file therefore cannot change.
///
/// Each of these is read once for the whole shell rather than once per surface: the UI locale and the helper
/// applications (`general`), the icon store (`icons`), the notification daemon (`notifications`), the launcher
/// — a single overlay, not a per-output surface — the user's directories (`paths`), and every section whose
/// job is to start a background producer. A per-monitor value here would apply on whichever screen happened to
/// be reconciled last and do nothing on the rest, which is worse than not being allowed at all.
pub const GLOBAL_ONLY_SECTIONS: &[&str] = &[
    "general",
    "icons",
    "notifications",
    "launcher",
    "paths",
    "audio",
    "brightness",
    "battery",
    "network",
    "bluetooth",
    "gpu",
    "weather",
];

fn monitor_config_path(path: &Path, output: &str) -> PathBuf {
    Config::monitor_dir(path).join(output).join("config.toml")
}

/// Deep-merges `over` into `base`: tables recurse key by key, everything else replaces.
///
/// Arrays replace rather than concatenate on purpose. A bar's module list is an array, and "the global list
/// plus this monitor's" has no sensible reading — a user overriding `start` means *this* is the start zone.
fn merge_into(base: &mut toml::Value, over: toml::Value) {
    match (base, over) {
        (toml::Value::Table(base), toml::Value::Table(over)) => {
            for (key, value) in over {
                match base.get_mut(&key) {
                    Some(existing) => merge_into(existing, value),
                    None => {
                        base.insert(key, value);
                    }
                }
            }
        }
        (base, over) => *base = over,
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
    fn saving_a_section_keeps_its_sub_tables_under_it_instead_of_scattering_them() {
        // What this catches is not a parse failure — the scattered file still parses, which is why nothing saw
        // it. Saving `[theme]` printed `[theme.export]` between `[panels]` and `[bars.top]`, put
        // `[theme.fonts.title]` inside the bar definitions, and left `[theme]` itself *after* its own children.
        // For a function whose whole promise is "preserving every other section, key order, and comment", that
        // is the failure.
        let dir = std::env::temp_dir().join(format!("hyprshell-save-order-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(
            &path,
            "[shape]\ngap = 8\n\n[panels]\ngap = 8\n\n[theme]\nname = \"nord\"\n\n[workspaces]\nshown = 10\n",
        )
        .unwrap();

        Config::save_section(&path, "theme", &ThemeConfig::default()).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        let headers: Vec<&str> = text
            .lines()
            .filter(|line| line.starts_with('['))
            .collect();
        let at = |name: &str| {
            headers
                .iter()
                .position(|h| *h == name)
                .unwrap_or_else(|| panic!("{name} missing from\n{text}"))
        };

        assert!(at("[theme]") < at("[theme.export]"), "a parent precedes its children:\n{text}");
        assert!(at("[theme]") < at("[theme.scale]"), "{text}");
        assert!(at("[shape]") < at("[panels]"), "{text}");
        assert!(at("[panels]") < at("[theme]"), "{text}");
        assert!(
            headers[at("[panels]") + 1] == "[theme]",
            "a section of theme's leaked between [panels] and [theme]:\n{text}"
        );
        assert!(
            at("[workspaces]") > at("[theme.export]"),
            "an unrelated section was pushed in among theme's children:\n{text}"
        );
        let reloaded: Config = toml::from_str(&text).expect("the saved file parses");
        assert_eq!(reloaded.workspaces.shown, 10);
        assert_eq!(reloaded.panels.gap, Some(8));
        assert_eq!(reloaded.theme.name, "nord");

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
        // An unset coordinate is the one field type TOML has no value for, so it is the one that would break
        // the write of a fresh config rather than merely round-trip oddly.
        assert_eq!(parsed.weather.latitude, None);
        assert_eq!(parsed.weather.refresh_minutes, starter.weather.refresh_minutes);
        assert_eq!(parsed.gpu.backend, "auto");
        assert!(parsed.paths.wallpapers.is_empty());
        assert_eq!(parsed.bluetooth.max_devices, starter.bluetooth.max_devices);
        assert_eq!(parsed.network.rescan_seconds, starter.network.rescan_seconds);
        assert_eq!(parsed.media.seek_seconds, starter.media.seek_seconds);
    }

    /// A6: every section that can start a background producer carries `enabled`, defaults it to on, and reads
    /// it back off a written config. A section that gained a service but not the flag would have no way to be
    /// switched off short of removing the module from the bar.
    #[test]
    fn every_service_section_can_be_switched_off() {
        // Each section's own `Default`, not `Config::default()` — the latter is all-empty by design, since it
        // is what backs serde's missing-field fill.
        for on in [
            NetworkConfig::default().enabled,
            BluetoothConfig::default().enabled,
            GpuConfig::default().enabled,
            WeatherConfig::default().enabled,
        ] {
            assert!(on, "a service section is on unless the user says otherwise");
        }

        let off: Config = toml::from_str(
            "[network]\nenabled=false\n[bluetooth]\nenabled=false\n\
             [gpu]\nenabled=false\n[weather]\nenabled=false\n",
        )
        .expect("parses");
        assert!(!off.network.enabled);
        assert!(!off.bluetooth.enabled);
        assert!(!off.gpu.enabled);
        assert!(!off.weather.enabled);
        // And the flag survives a save, so switching one off in the settings panel sticks.
        let round_tripped: Config =
            toml::from_str(&toml::to_string_pretty(&off).expect("serializes")).expect("re-parses");
        assert!(!round_tripped.weather.enabled);
        assert!(!round_tripped.network.enabled);
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
    fn the_notification_panel_settings_default_to_grouped_and_silent() {
        let d = NotificationsConfig::default();
        assert!(d.group_by_app);
        assert_eq!(d.group_preview(), 3);
        assert!(d.action_on_click, "a tap opens what the notification is about");
        assert_eq!(d.body_max_lines(), Some(4));
        assert_eq!(
            d.sound_command(),
            None,
            "a shell that started making noise on upgrade would be a bug"
        );
        assert_eq!(
            d.fullscreen,
            FullscreenPopups::Off,
            "a fullscreen window is not interrupted unless it matters"
        );

        let zeroed = NotificationsConfig {
            group_preview_num: 0,
            body_lines: 0,
            sound: "   ".to_string(),
            ..NotificationsConfig::default()
        };
        assert_eq!(zeroed.group_preview(), 1);
        assert_eq!(zeroed.body_max_lines(), Some(1));
        assert_eq!(zeroed.sound_command(), None, "a whitespace-only command is silent");

        let expanded = NotificationsConfig {
            open_expanded: true,
            ..NotificationsConfig::default()
        };
        assert_eq!(expanded.body_max_lines(), None, "the whole body, uncapped");
    }

    #[test]
    fn the_fullscreen_policy_reads_as_the_three_words_it_writes() {
        let parsed = |value: &str| {
            toml::from_str::<Config>(&format!("[notifications]\nfullscreen = \"{value}\"\n"))
                .unwrap()
                .notifications
                .fullscreen
        };
        assert_eq!(parsed("on"), FullscreenPopups::On);
        assert_eq!(parsed("off"), FullscreenPopups::Off);
        assert_eq!(parsed("never"), FullscreenPopups::Never);

        let round_tripped = toml::to_string(&NotificationsConfig {
            fullscreen: FullscreenPopups::Never,
            ..NotificationsConfig::default()
        })
        .unwrap();
        assert!(round_tripped.contains("fullscreen = \"never\""), "{round_tripped}");
    }

    /// A config directory with a global file and, optionally, one monitor override.
    fn config_dir(name: &str, global: &str, monitor: Option<(&str, &str)>) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("hyprshell-{name}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("config.toml"), global).unwrap();
        if let Some((output, text)) = monitor {
            let out_dir = dir.join("monitors").join(output);
            std::fs::create_dir_all(&out_dir).unwrap();
            std::fs::write(out_dir.join("config.toml"), text).unwrap();
        }
        dir
    }

    #[test]
    fn a_monitor_override_merges_over_the_global_config_key_by_key() {
        let dir = config_dir(
            "monitor-merge",
            r#"
[bars.top]
size = 34
start = ["workspaces"]
center = ["clock"]

[theme]
accent = "cyan"
name = "nord"
"#,
            Some((
                "DP-2",
                r#"
[bars.top]
size = 44
start = ["cpu", "memory"]

[theme]
accent = "orange"
"#,
            )),
        );
        let path = dir.join("config.toml");

        let global = Config::for_output(&path, None).unwrap();
        assert_eq!(global.bars.top.size, 34);
        assert_eq!(ids(&global.bars.top.start), ["workspaces"]);
        assert_eq!(global.theme.accent, "cyan");

        let overridden = Config::for_output(&path, Some("DP-2")).unwrap();
        assert_eq!(overridden.bars.top.size, 44, "the override wins");
        assert_eq!(
            ids(&overridden.bars.top.start),
            ["cpu", "memory"],
            "an array replaces rather than concatenating"
        );
        assert_eq!(
            ids(&overridden.bars.top.center),
            ["clock"],
            "a key the override never mentions keeps the global value"
        );
        assert_eq!(overridden.theme.accent, "orange");
        assert_eq!(
            overridden.theme.name, "nord",
            "merging is per key, not per section"
        );

        let unknown = Config::for_output(&path, Some("HDMI-A-1")).unwrap();
        assert_eq!(unknown.bars.top.size, 34, "a screen with no file is the global config");

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn a_monitor_override_cannot_change_a_section_one_process_owns() {
        let dir = config_dir(
            "monitor-global-only",
            "[general]\nlanguage = \"en\"\n\n[shape]\ngap = 0\n",
            Some((
                "DP-1",
                "[general]\nlanguage = \"es\"\n\n[notifications]\nmax_visible = 99\n\n[shape]\ngap = 12\n",
            )),
        );
        let path = dir.join("config.toml");
        let cfg = Config::for_output(&path, Some("DP-1")).unwrap();

        assert_eq!(cfg.general.language, "en", "[general] is global-only");
        assert_eq!(
            cfg.notifications.max_visible,
            NotificationsConfig::default().max_visible,
            "[notifications] is global-only — one daemon owns it"
        );
        assert_eq!(cfg.shape.gap, 12, "a visual section is still the monitor's to set");
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn excluded_screens_match_as_patterns_and_never_catch_an_unnamed_output() {
        let bars = BarsConfig {
            excluded_screens: vec!["HDMI-*".to_string(), "DP-3".to_string()],
            ..BarsConfig::default()
        };
        assert!(bars.excludes(Some("HDMI-A-1")));
        assert!(bars.excludes(Some("DP-3")));
        assert!(!bars.excludes(Some("DP-1")));
        assert!(
            !bars.excludes(None),
            "an output the compositor did not name has nothing to match, so it keeps its bars"
        );
        assert!(
            !BarsConfig::default().excludes(Some("DP-1")),
            "no exclusions is every screen"
        );
    }

    #[test]
    fn an_unversioned_config_is_migrated_forward_and_migration_is_idempotent() {
        // v0: the terminal lived at `[general] terminal`, before `[general.apps]` existed.
        let legacy = "[general]\nterminal = \"kitty\"\n";
        let cfg: Config = {
            let mut document: toml::Value = toml::from_str(legacy).unwrap();
            migrate(&mut document);
            document.try_into().unwrap()
        };
        assert_eq!(cfg.general.apps.terminal, "kitty", "moved into its new home");
        assert_eq!(cfg.app_command(HelperApp::Terminal), "kitty");

        let mut twice: toml::Value = toml::from_str(legacy).unwrap();
        migrate(&mut twice);
        let once = twice.clone();
        migrate(&mut twice);
        assert_eq!(twice, once);

        let mut both: toml::Value =
            toml::from_str("[general]\nterminal = \"xterm\"\n\n[general.apps]\nterminal = \"foot\"\n")
                .unwrap();
        migrate(&mut both);
        let cfg: Config = both.try_into().unwrap();
        assert_eq!(cfg.general.apps.terminal, "foot");

        let mut current: toml::Value =
            toml::from_str(&format!("version = {CONFIG_VERSION}\n[general]\nterminal = \"kitty\"\n"))
                .unwrap();
        let before = current.clone();
        migrate(&mut current);
        assert_eq!(current, before);
    }

    #[test]
    fn animation_durations_scale_together_and_collapse_when_switched_off() {
        let base = Duration::from_millis(200);
        let d = AnimationConfig::default();
        assert_eq!(d.duration(base), base, "the default scale moves nothing");

        let quick = AnimationConfig {
            duration_scale: 0.5,
            ..AnimationConfig::default()
        };
        assert_eq!(quick.duration(base), Duration::from_millis(100));

        let off = AnimationConfig {
            enabled: false,
            duration_scale: 4.0,
            ..AnimationConfig::default()
        };
        assert_eq!(
            off.duration(base),
            Duration::ZERO,
            "off wins over any scale — it is the accessibility answer, not a speed"
        );

        // Bounded, so a `0` cannot make everything instant by accident rather than by the switch that says so.
        let broken = AnimationConfig {
            duration_scale: 0.0,
            ..AnimationConfig::default()
        };
        assert_eq!(broken.duration(base), Duration::from_millis(20));
        let nan = AnimationConfig {
            duration_scale: f32::NAN,
            ..AnimationConfig::default()
        };
        assert_eq!(nan.duration(base), base, "an unusable factor is no factor");
    }

    #[test]
    fn the_two_named_curve_families_resolve_and_fall_back() {
        let with = |curve: &str, easing: &str| AnimationConfig {
            curve: curve.to_string(),
            easing: easing.to_string(),
            ..AnimationConfig::default()
        };
        assert_eq!(with("snappy", "").spring(), rsx::motion::Spring::snappy());
        assert_eq!(with("BOUNCY", "").spring(), rsx::motion::Spring::bouncy());
        assert_eq!(
            with("nonsense", "").spring(),
            rsx::motion::Spring::gentle(),
            "an unknown name is the default, not a panic"
        );
        assert_eq!(with("", "linear").easing(), rsx::motion::Easing::Linear);
        assert_eq!(with("", "ease_in_out").easing(), rsx::motion::Easing::EaseInOut);
        assert_eq!(with("", "nonsense").easing(), rsx::motion::Easing::EaseOut);
    }

    #[test]
    fn a_per_role_font_override_changes_only_the_role_it_names() {
        use crate::shared::theme::FontRole;

        let cfg: Config = toml::from_str(
            "[theme]\nfont_size = 13.0\n\n[theme.fonts.caption]\nsize = 20.0\nweight = 700\nitalic = true\n",
        )
        .unwrap();
        let theme = cfg.resolve_theme();
        assert_eq!(theme.font(FontRole::Caption), 20.0, "the named role takes the override");
        assert_eq!(theme.font(FontRole::Body), 13.0, "and every other role is untouched");

        let styled = theme.text_style(FontRole::Caption, theme.text);
        assert_eq!(styled.weight, 700);
        assert!(styled.italic);
        let plain = theme.text_style(FontRole::Body, theme.text);
        assert_eq!(plain.weight, 400, "a role with no override keeps the default weight");
        assert!(!plain.italic);

        // Bounded on read: a size a screen cannot render is not a size.
        let absurd: Config =
            toml::from_str("[theme.fonts.body]\nsize = 100000.0\n").unwrap();
        assert_eq!(absurd.resolve_theme().font(FontRole::Body), 200.0);
    }

    #[test]
    fn the_panel_background_is_solid_by_default_and_never_fades_past_readable() {
        let solid = Config::starter();
        assert_eq!(solid.panel_fill().a, 1.0, "a panel is opaque unless asked otherwise");
        assert_eq!(
            solid.panel_fill().to_rgba8(),
            solid.resolve_theme().surface.to_rgba8(),
            "and it is exactly the surface token, so nothing changes for a config that never sets it"
        );

        let translucent = Config {
            panels: PanelsConfig {
                opacity: 0.75,
                ..PanelsConfig::default()
            },
            ..Config::starter()
        };
        assert_eq!(translucent.panel_fill().a, 0.75);

        // Floored: a panel faded past readability looks like one that failed to open.
        let ghost = Config {
            panels: PanelsConfig {
                opacity: 0.0,
                ..PanelsConfig::default()
            },
            ..Config::starter()
        };
        assert_eq!(ghost.panel_fill().a, 0.2);
        let broken = Config {
            panels: PanelsConfig {
                opacity: f32::NAN,
                ..PanelsConfig::default()
            },
            ..Config::starter()
        };
        assert_eq!(broken.panel_fill().a, 1.0, "an unusable value is no value");
    }

    #[test]
    fn the_two_drag_thresholds_are_bounded_and_switch_off_at_zero() {
        // Drag-to-open: floored well above the tap slop, so an unsteady click cannot cross it.
        assert_eq!(PanelsConfig::default().drag_threshold(), Some(48.0));
        let off = PanelsConfig {
            drag_threshold: 0.0,
            ..PanelsConfig::default()
        };
        assert_eq!(off.drag_threshold(), None);
        let tiny = PanelsConfig {
            drag_threshold: 1.0,
            ..PanelsConfig::default()
        };
        assert_eq!(tiny.drag_threshold(), Some(16.0));
        let nan = PanelsConfig {
            drag_threshold: f32::NAN,
            ..PanelsConfig::default()
        };
        assert_eq!(nan.drag_threshold(), None);

        // Swipe-to-dismiss: a fraction of the card, never the whole width — an unreachable threshold reads as a card that is stuck rather than as a setting that is wrong.
        let n = NotificationsConfig::default();
        assert_eq!(n.swipe_distance(400.0), Some(140.0));
        let full = NotificationsConfig {
            clear_threshold: 2.0,
            ..NotificationsConfig::default()
        };
        assert_eq!(full.swipe_distance(400.0), Some(360.0));
        let disabled = NotificationsConfig {
            clear_threshold: 0.0,
            ..NotificationsConfig::default()
        };
        assert_eq!(disabled.swipe_distance(400.0), None);
    }

    #[test]
    fn the_appearance_scales_multiply_the_tokens_the_user_already_chose() {
        let plain: Config = toml::from_str("").unwrap();
        let base = plain.resolve_theme();
        assert_eq!(
            plain.theme.scale.rounding, 1.0,
            "a config that never mentions scaling is the config it always was"
        );

        let scaled: Config = toml::from_str(
            "[theme]\nradius = 10\nfont_size = 14.0\n\n[theme.scale]\nrounding = 2.0\nfont = 0.5\n",
        )
        .unwrap();
        let theme = scaled.resolve_theme();
        assert_eq!(theme.radius, 20.0, "the scale multiplies the pinned radius, not the palette's");
        assert_eq!(theme.font_size, 7.0);
        assert_eq!(
            theme.icon_size, base.icon_size,
            "a scale left at 1 moves nothing"
        );

        let broken: Config = toml::from_str(
            "[theme]\nfont_size = 12.0\nicon_size = 20.0\n\n[theme.scale]\nfont = 0.0\nicon = nan\n",
        )
        .unwrap();
        let theme = broken.resolve_theme();
        assert_eq!(theme.font_size, 3.0, "clamped to the 0.25 floor");
        assert_eq!(theme.icon_size, 20.0, "an unusable factor is no factor");
    }

    #[test]
    fn a_mode_switches_a_family_to_its_other_side_and_leaves_a_one_sided_palette_alone() {
        use crate::shared::scheme::Mode;
        assert_eq!(NordTheme::in_mode("gruvbox", Mode::Light), "gruvbox-light");
        assert_eq!(NordTheme::in_mode("gruvbox-light", Mode::Dark), "gruvbox");
        assert_eq!(NordTheme::in_mode("catppuccin-frappe", Mode::Light), "catppuccin-latte");
        assert_eq!(NordTheme::in_mode("rose_pine_moon", Mode::Light), "rose-pine-dawn");
        // Nord has no light sibling anyone drew, and inventing one by inversion would be a palette its author
        // never made.
        assert_eq!(NordTheme::in_mode("nord", Mode::Light), "nord");
        assert_eq!(NordTheme::in_mode("tokyo-night", Mode::Light), "tokyo-night");
        // Already on the asked-for side: a no-op, not a round trip through the other one.
        assert_eq!(NordTheme::in_mode("gruvbox-light", Mode::Light), "gruvbox-light");

        let light: Config = toml::from_str("[theme]\nname = \"gruvbox\"\nmode = \"light\"\n").unwrap();
        assert_eq!(
            light.resolve_theme().base,
            NordTheme::gruvbox_light().base,
            "the mode reaches the resolved theme, not just the name"
        );
        let auto: Config = toml::from_str("[theme]\nname = \"gruvbox\"\n").unwrap();
        assert_eq!(
            auto.resolve_theme().base,
            NordTheme::gruvbox().base,
            "'auto' keeps whatever the palette already is"
        );
    }

    #[test]
    fn a_dynamic_theme_falls_back_to_a_real_palette_until_a_wallpaper_has_been_read() {
        // Nothing has been quantised in a unit test, which is also the state of a fresh install's first frame.
        let dynamic: Config =
            toml::from_str("[theme]\nname = \"dynamic\"\nfallback = \"catppuccin-latte\"\n").unwrap();
        assert!(dynamic.theme.is_dynamic());
        assert_eq!(
            dynamic.resolve_theme().base,
            NordTheme::catppuccin_latte().base,
            "the fallback is a setting, not a formality"
        );
        let tuned: Config = toml::from_str(
            "[theme]\nname = \"dynamic\"\nfallback = \"nord\"\nradius = 14\n",
        )
        .unwrap();
        assert_eq!(tuned.resolve_theme().radius, 14.0);
    }

    #[test]
    fn auto_reads_the_mode_a_dynamic_scheme_should_be_generated_at_off_the_fallback() {
        use crate::shared::scheme::{Mode, Variant};
        let dark: Config = toml::from_str("[theme]\nname = \"dynamic\"\nfallback = \"nord\"\n").unwrap();
        assert_eq!(dark.scheme_selection(), (Mode::Dark, Variant::Vibrant));

        // A user whose fallback is a light palette has already said which end of the ramp they live at.
        let light: Config =
            toml::from_str("[theme]\nname = \"dynamic\"\nfallback = \"gruvbox-light\"\n").unwrap();
        assert_eq!(light.scheme_selection().0, Mode::Light);

        let pinned: Config = toml::from_str(
            "[theme]\nname = \"dynamic\"\nfallback = \"gruvbox-light\"\nmode = \"dark\"\nvariant = \"muted\"\n",
        )
        .unwrap();
        assert_eq!(pinned.scheme_selection(), (Mode::Dark, Variant::Muted));
        let nonsense: Config = toml::from_str("[theme]\nvariant = \"sparkly\"\n").unwrap();
        assert_eq!(nonsense.scheme_selection().1, Variant::Vibrant);
    }

    #[test]
    fn a_wallpaper_transition_is_zero_whenever_nothing_should_move() {
        let fading: Config = toml::from_str("[background]\ntransition_ms = 400\n").unwrap();
        assert_eq!(fading.wallpaper_transition(), Duration::from_millis(400));

        let none: Config =
            toml::from_str("[background]\ntransition = \"none\"\ntransition_ms = 400\n").unwrap();
        assert!(none.wallpaper_transition().is_zero());

        let off: Config =
            toml::from_str("[background]\ntransition_ms = 400\n\n[animation]\nenabled = false\n")
                .unwrap();
        assert!(
            off.wallpaper_transition().is_zero(),
            "the global animation switch reaches this like every other duration"
        );

        let scaled: Config =
            toml::from_str("[background]\ntransition_ms = 400\n\n[animation]\nduration_scale = 2.0\n")
                .unwrap();
        assert_eq!(scaled.wallpaper_transition(), Duration::from_millis(800));

        // An absurd duration is a slow transition, never one that outlives the session.
        let absurd: Config = toml::from_str("[background]\ntransition_ms = 999999999\n").unwrap();
        assert_eq!(absurd.wallpaper_transition(), Duration::from_millis(10_000));
    }

    #[test]
    fn the_background_surface_is_opened_by_anything_that_needs_to_draw_on_it() {
        let bare: Config = toml::from_str("").unwrap();
        assert!(!bare.background.is_enabled(), "opt-in, so it never clobbers the compositor's own");

        for toml_text in [
            "[background]\nenabled = true\n",
            "[background]\nimage = \"~/wall.png\"\n",
            "[background.monitors]\nDP-1 = \"~/wall.png\"\n",
            // The clock lives on that surface, so asking for it is asking for the surface.
            "[background.clock]\nenabled = true\n",
        ] {
            let config: Config = toml::from_str(toml_text).unwrap();
            assert!(config.background.is_enabled(), "'{toml_text}' needs the surface");
        }
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
