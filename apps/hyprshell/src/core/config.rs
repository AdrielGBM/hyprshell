use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A screen edge; top/bottom use horizontal layout, left/right use vertical.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
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

    /// The two corners at this bar's leading and trailing ends, in main-axis order.
    pub fn corners(self) -> (Corner, Corner) {
        match self {
            Edge::Top => (Corner::TopLeft, Corner::TopRight),
            Edge::Bottom => (Corner::BottomLeft, Corner::BottomRight),
            Edge::Left => (Corner::TopLeft, Corner::BottomLeft),
            Edge::Right => (Corner::TopRight, Corner::BottomRight),
        }
    }
}

/// One of the four screen corners; routed to the adjacent bar that owns it.
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

/// Global shape settings; defaults (gap=0, radius=0) reproduce today's edge-to-edge bar — floating is opt-in.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ShapeConfig {
    pub mode: Shape,
    pub frame: bool,
    pub gap: u32,
    pub spacing: u32,
    pub radius: u32,
    pub inactive_size: u32,
}

impl Default for ShapeConfig {
    fn default() -> Self {
        Self {
            mode: Shape::Bar,
            frame: false,
            gap: 0,
            spacing: 6,
            radius: 0,
            inactive_size: 6,
        }
    }
}

/// Per-bar shape override; each field falls back to the global `[shape]`.
#[derive(Deserialize, Serialize, Clone, Copy, Debug, Default)]
#[serde(default)]
pub struct BarShape {
    pub mode: Option<Shape>,
    pub gap: Option<u32>,
    pub spacing: Option<u32>,
    pub radius: Option<u32>,
}

/// A bar's resolved shape after merging its override with the global defaults.
#[derive(Clone, Copy, Debug)]
pub struct ResolvedShape {
    pub mode: Shape,
    pub gap: u32,
    pub spacing: u32,
    pub radius: u32,
}

impl ResolvedShape {
    /// Inner padding = round(spacing / 2).
    pub fn padding(self) -> u32 {
        (self.spacing as f32 / 2.0).round() as u32
    }

    /// Chip radius shrunk to nest inside a unit.
    pub fn chip_radius(self) -> u32 {
        self.radius.saturating_sub(self.padding())
    }
}

/// Optional module per corner, routed to the leading/trailing zone of the bar that owns that corner.
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

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub bars: BarsConfig,
    pub theme: ThemeConfig,
    pub shape: ShapeConfig,
    pub corners: CornersConfig,
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

/// A bar's thickness and three module zones.
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

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct ThemeConfig {
    pub name: String,
    pub accent: String,
}

impl Default for ThemeConfig {
    fn default() -> Self {
        Self {
            name: "nord".to_string(),
            accent: "cyan".to_string(),
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
                    end: Vec::new(),
                    shape: BarShape::default(),
                },
                ..BarsConfig::default()
            },
            theme: ThemeConfig::default(),
            shape: ShapeConfig::default(),
            corners: CornersConfig::default(),
        }
    }

    /// Effective shape for edge: per-bar override if set, else global default.
    pub fn shape_for(&self, edge: Edge) -> ResolvedShape {
        let g = &self.shape;
        let b = &self.bars.get(edge).shape;
        ResolvedShape {
            mode: b.mode.unwrap_or(g.mode),
            gap: b.gap.unwrap_or(g.gap),
            spacing: b.spacing.unwrap_or(g.spacing),
            radius: b.radius.unwrap_or(g.radius),
        }
    }

    /// Whether a bar hugs its edge; frame forces hug, otherwise only at gap == 0.
    pub fn hugs(&self, edge: Edge) -> bool {
        self.shape.frame || self.shape_for(edge).gap == 0
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

    /// Whether edge has any surface (active content or frame strip).
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
        s.mode == Shape::Bar && (self.shape.frame || (s.gap == 0 && s.radius == 0))
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(cfg.shape.radius, 0);
        let top = cfg.shape_for(Edge::Top);
        assert_eq!(top.mode, Shape::Bar);
        assert_eq!(top.gap, 0);
        assert_eq!(top.radius, 0);
        assert!(cfg.hugs(Edge::Top));
        assert!(cfg.bar_surface_opaque(Edge::Top));
    }

    #[test]
    fn partial_override_takes_precedence_field_by_field() {
        // top overrides mode+gap; spacing/radius fall back to the global.
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
        assert_eq!(top.spacing, 6, "spacing inherits the global");
        assert_eq!(top.radius, 10, "radius inherits the global");
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
            spacing: 6,
            radius: 12,
        };
        assert_eq!(s.padding(), 3, "round(6/2)");
        assert_eq!(s.chip_radius(), 9, "max(0, 12 - 3)");
        let tight = ResolvedShape {
            mode: Shape::Chips,
            gap: 0,
            spacing: 30,
            radius: 4,
        };
        assert_eq!(tight.chip_radius(), 0, "radius floors at 0, never negative");
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
