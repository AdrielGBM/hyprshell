use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// A screen edge a bar can dock to. Top/bottom bars lay their zones out horizontally (start→end = left→right); left/right bars lay them out vertically (start→end = top→bottom).
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
}

#[derive(Deserialize, Serialize, Clone, Debug, Default)]
#[serde(default)]
pub struct Config {
    pub bars: BarsConfig,
    pub theme: ThemeConfig,
}

/// One bar per screen edge. An edge whose zones are all empty is not shown — no surface is created for it (features.md §1: empty bars collapse to zero).
///
/// The `Default` here is all-empty on purpose: `#[serde(default)]` fills missing fields from `Self::default()`, so a hand-written config gets exactly the bars it names (what you write is what you get). The starter bar a fresh install sees lives in [`Config::starter`], not here.
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

/// A bar's thickness (`size`: height for top/bottom, width for left/right) and its three ordered module zones.
#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(default)]
pub struct BarConfig {
    pub size: u32,
    pub start: Vec<String>,
    pub center: Vec<String>,
    pub end: Vec<String>,
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
    /// The config a fresh install starts from: a single top bar with the workspaces and clock modules. Distinct from `Default` (all-empty), which only backs serde's missing-field fill.
    pub fn starter() -> Self {
        Self {
            bars: BarsConfig {
                top: BarConfig {
                    size: 34,
                    start: vec!["workspaces".to_string()],
                    center: vec!["clock".to_string()],
                    end: Vec::new(),
                },
                ..BarsConfig::default()
            },
            theme: ThemeConfig::default(),
        }
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
        // `Default` backs serde's missing-field fill, so it must be empty — otherwise a partial config would inherit bars it never named.
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
        // An explicit config that omits `top` gets an empty top bar — the fresh-install default only seeds a file that does not exist yet.
        assert!(cfg.bars.top.is_empty());
    }

    #[test]
    fn edge_orientation() {
        assert!(Edge::Top.is_horizontal() && Edge::Bottom.is_horizontal());
        assert!(Edge::Left.is_vertical() && Edge::Right.is_vertical());
    }
}
