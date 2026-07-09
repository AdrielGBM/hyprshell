// Re-exported so a shell configures its surfaces without depending on smithay-client-toolkit directly.
pub use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

/// How one layer surface (a bar / panel / OSD / background) is placed on its output.
#[derive(Clone)]
pub struct LayerConfig {
    /// Output to pin to, by connector name (e.g. `"DP-1"`). `None` lets the compositor choose (usually the
    /// focused output).
    pub output: Option<String>,
    /// Compositor layer: background / bottom / top / overlay.
    pub layer: Layer,
    /// Which screen edges the surface is anchored to (a bar anchors to 3 edges; a centered popup to none).
    pub anchor: Anchor,
    /// Screen edge to reserve, in logical px, so windows don't overlap the surface. `0` reserves nothing;
    /// `-1` ignores other surfaces' exclusive zones (lets the surface overlap them).
    pub exclusive_zone: i32,
    /// Surface size in logical px. `0` on an axis means "span it" (requires anchoring both ends of that axis).
    pub size: (u32, u32),
    /// `(top, right, bottom, left)` margins in logical px.
    pub margin: (i32, i32, i32, i32),
    /// Whether the surface takes keyboard focus.
    pub keyboard_interactivity: KeyboardInteractivity,
    /// Layer namespace, surfaced to the compositor (e.g. for `hyprctl layers`).
    pub namespace: String,
}

impl Default for LayerConfig {
    fn default() -> Self {
        Self {
            output: None,
            layer: Layer::Top,
            anchor: Anchor::TOP.union(Anchor::LEFT).union(Anchor::RIGHT),
            exclusive_zone: 0,
            size: (0, 40),
            margin: (0, 0, 0, 0),
            keyboard_interactivity: KeyboardInteractivity::None,
            namespace: String::from("hyprshell"),
        }
    }
}

/// A connected output (monitor), returned by [`crate::enumerate_outputs`] so a shell can build one surface per
/// monitor.
#[derive(Debug, Clone)]
pub struct OutputDescriptor {
    /// Connector name, e.g. `"DP-1"` / `"HDMI-A-1"`. `None` on compositors that don't report it.
    pub name: Option<String>,
    /// Logical size in px, if the compositor reports it (via xdg-output).
    pub logical_size: Option<(i32, i32)>,
    /// Logical top-left position in the compositor's global space.
    pub position: (i32, i32),
    /// Integer scale factor.
    pub scale: i32,
}
