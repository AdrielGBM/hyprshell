pub use smithay_client_toolkit::shell::wlr_layer::{Anchor, KeyboardInteractivity, Layer};

#[derive(Clone)]
pub struct LayerConfig {
    pub output: Option<String>,
    pub layer: Layer,
    pub anchor: Anchor,
    pub exclusive_zone: i32,
    pub size: (u32, u32),
    pub margin: (i32, i32, i32, i32),
    pub keyboard_interactivity: KeyboardInteractivity,
    pub namespace: String,
    /// Layer surface reserves exclusive_zone only once mapped (needs a buffer).
    pub reserve_only: bool,
    /// Empty input region routes pointer/touch through to surfaces beneath.
    pub input_transparent: bool,
    /// Carves the input region from the surface's interactive widgets each frame (via `rsx::interactive_rects`):
    /// pointer input lands on pressable content, everything else falls through. For click-through overlays with
    /// tappable parts, such as notification popups. Takes precedence over `input_transparent`.
    pub interactive_input_region: bool,
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
            reserve_only: false,
            input_transparent: false,
            interactive_input_region: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct OutputDescriptor {
    pub name: Option<String>,
    pub logical_size: Option<(i32, i32)>,
    pub position: (i32, i32),
    pub scale: i32,
}
