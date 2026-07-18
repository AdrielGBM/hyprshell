use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
#[derive(Deserialize, Serialize, Clone, Debug)]
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

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub bars: BarsConfig,
    pub theme: ThemeConfig,
    pub shape: ShapeConfig,
    pub corners: CornersConfig,
    pub panels: PanelsConfig,
    pub osd: OsdConfig,
    pub icons: IconsConfig,
    pub notifications: NotificationsConfig,
    pub background: BackgroundConfig,
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
    pub start: Vec<String>,
    pub center: Vec<String>,
    pub end: Vec<String>,
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
                    start: vec!["workspaces".to_string()],
                    center: vec!["clock".to_string()],
                    end: vec!["notes".to_string()],
                    shape: BarShape::default(),
                },
                ..BarsConfig::default()
            },
            theme: ThemeConfig::default(),
            shape: ShapeConfig::default(),
            corners: CornersConfig::default(),
            panels: PanelsConfig::default(),
            osd: OsdConfig::default(),
            icons: IconsConfig::default(),
            notifications: NotificationsConfig::default(),
            background: BackgroundConfig::default(),
            modules: HashMap::new(),
        }
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

    /// Which zone (start/center/end) a module occupies on `edge`, for deriving its drawer's alignment.
    pub fn zone_of(&self, edge: Edge, module_id: &str) -> Option<Zone> {
        let bar = self.bars.get(edge);
        if bar.start.iter().any(|m| m == module_id) {
            Some(Zone::Start)
        } else if bar.center.iter().any(|m| m == module_id) {
            Some(Zone::Center)
        } else if bar.end.iter().any(|m| m == module_id) {
            Some(Zone::End)
        } else {
            None
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

    pub fn load_or_default(path: &Path) -> Self {
        match std::fs::read_to_string(path) {
            Ok(text) => match toml::from_str(&text) {
                Ok(cfg) => cfg,
                Err(e) => {
                    tracing::warn!("config parse error ({e}); using the starter config");
                    Config::starter()
                }
            },
            Err(_) => {
                let cfg = Config::starter();
                if let Ok(text) = toml::to_string_pretty(&cfg) {
                    if let Some(parent) = path.parent() {
                        let _ = std::fs::create_dir_all(parent);
                    }
                    let _ = std::fs::write(path, text);
                }
                cfg
            }
        }
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
        assert_eq!(cfg.bars.top.start, vec!["workspaces".to_string()]);
        assert_eq!(cfg.bars.top.center, vec!["clock".to_string()]);
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
        assert_eq!(cfg.bars.left.start, vec!["workspaces".to_string()]);
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
