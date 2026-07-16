use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use platform_layershell::request_close;
use rsx::{
    SurfaceFrameStyle, SurfacePlacement, SurfaceSize, SurfaceToken, open_surface, set_theme,
    surface_frame,
};

use crate::modules::drawer::module_panel;
use crate::shared::module::surface_env;
use crate::shared::theme::NordTheme;

const FLOAT_W: u32 = 360;
const FLOAT_H: u32 = 240;

thread_local! {
    // Keyed by module id. Dropping a token closes its window; several named windows can be open at once.
    static FLOAT_WINDOWS: RefCell<HashMap<String, SurfaceToken>> = RefCell::new(HashMap::new());
}

/// Toggles the floating window for `module_id`: opens as a centred, titled, closable window, or closes it if already open; the shell only declares the placement, the rsx surface host and `surface_frame` realize the window chrome.
pub fn toggle_float(module_id: &str) {
    let Some(env) = surface_env() else { return };
    FLOAT_WINDOWS.with(|slot| {
        let mut slot = slot.borrow_mut();
        let already_open = slot.get(module_id).is_some_and(|t| !t.is_closing());
        slot.remove(module_id); // drops any existing token → closes that window
        if !already_open {
            let accent = NordTheme::new().accent_by_name(&env.config.theme.accent);
            let title = module_id.to_string();
            let module = module_id.to_string();
            let placement = SurfacePlacement::float().size(SurfaceSize::Fixed(FLOAT_W, FLOAT_H));
            let token = open_surface(
                placement,
                Box::new(move || {
                    let theme = NordTheme {
                        accent,
                        ..NordTheme::new()
                    };
                    set_theme(theme);
                    let body = module_panel(&module).expect("float panel build failed");
                    let style = SurfaceFrameStyle {
                        background: theme.surface,
                        title_bar: theme.overlay,
                        title_text: theme.text,
                        close: theme.muted,
                        radius: 14.0,
                        font_size: 14.0,
                    };
                    let close: Rc<dyn Fn()> = Rc::new(request_close);
                    surface_frame(title, style, close, body).expect("surface frame build failed")
                }),
            );
            slot.insert(module_id.to_string(), token);
        }
    });
}

#[cfg(test)]
mod tests {
    use crate::shared::theme::NordTheme;
    use crate::test_support::{render_png, render_png_frames};
    use rsx::{
        App, Color, Component, LayoutStyle, RectStyle, StyledContainer, SurfaceFrameStyle,
        SurfaceRoot, WindowConfig, box_item, reset_layout_runtime, surface_frame,
    };

    /// The reusable `surface_frame` chrome (title bar + ✕) around a placeholder body, the same tree the surface host mounts for a float.
    struct FloatPreviewApp {
        animate: bool,
    }

    impl App for FloatPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            let theme = NordTheme::new();
            let body = box_item(
                StyledContainer::new(
                    LayoutStyle::new().width(220.0).height(90.0),
                    move |_| RectStyle::filled(theme.overlay, 8.0),
                    vec![],
                )
                .unwrap(),
            );
            let style = SurfaceFrameStyle {
                background: theme.surface,
                title_bar: theme.overlay,
                title_text: theme.text,
                close: theme.muted,
                radius: 14.0,
                font_size: 14.0,
            };
            let frame = surface_frame("Clock", style, std::rc::Rc::new(|| {}), body).unwrap();
            let root = SurfaceRoot::new(frame).expect("float surface root failed");
            let root = if self.animate { root.animate_in() } else { root };
            Box::new(root)
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            None
        }
    }

    /// Renders a floating window's frame (§5), settled. Gated on its own env var.
    #[test]
    fn visual_float_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_FLOAT_OUT") else {
            eprintln!("set RSX_VISUAL_FLOAT_OUT to render the float; skipping");
            return;
        };
        render_png(FloatPreviewApp { animate: false }, 360, 240, &out);
    }

    /// Renders the float with its enter animation, driving enough frames to settle — a guard that the animated path lands fully visible, not stuck transparent. Gated on its own env var.
    #[test]
    fn visual_float_anim_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_FLOAT_ANIM_OUT") else {
            eprintln!("set RSX_VISUAL_FLOAT_ANIM_OUT to render the animated float; skipping");
            return;
        };
        let frames = std::env::var("HYPRSHELL_VISUAL_FRAMES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        render_png_frames(FloatPreviewApp { animate: true }, 360, 240, &out, frames);
    }
}
