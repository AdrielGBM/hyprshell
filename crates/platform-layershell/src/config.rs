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

#[derive(Debug, Clone)]
pub struct OutputDescriptor {
    pub name: Option<String>,
    pub logical_size: Option<(i32, i32)>,
    pub position: (i32, i32),
    pub scale: i32,
}
