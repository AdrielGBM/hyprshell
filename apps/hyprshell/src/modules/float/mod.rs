use std::rc::Rc;

use platform_layershell::{request_close, request_size};
use telar::{
    SurfaceFrameStyle, SurfacePlacement, SurfaceSize, SurfaceToken, open_surface, set_theme,
    surface_content, surface_frame,
};

use crate::modules::drawer::{module_panel, panel_wants_keyboard};
use crate::shared::module::SurfaceEnv;
use crate::shared::theme::FontRole;

/// Opens `module_id`'s panel as a centred, titled, closable window on the bar's own monitor, sized per its `[modules.<id>]` override or `[panels.float]`; the shell only declares the placement, the rsx surface host and `surface_frame` realize the window chrome. Toggle/close is the caller's job ([`crate::toggle_panel`]) via the returned token.
///
/// `[modules.<id>]` (or `[panels.float]`) is the size the window *opens* at, not the size it is stuck at: the
/// frame's corner grip renegotiates the layer surface as it is dragged. That size lasts as long as the window
/// does and is deliberately not written back — persisting it would mean a config write, and a reload, per drag.
pub(crate) fn open_float(env: &SurfaceEnv, module_id: &str) -> SurfaceToken {
    let module = module_id.to_string();
    let edge = env.edge;
    let output = env.output.clone();
    let (width, height) = env.config.float_size_for(module_id);
    let placement = SurfacePlacement::float()
        .size(SurfaceSize::Fixed(width, height))
        .keyboard(panel_wants_keyboard(module_id))
        .output(env.output.clone());
    open_surface(
        placement,
        // Resolved per build rather than captured: the window outlives the config it opened under, and a
        // rebuild is how it follows an edit. What is captured is which module it shows and where it is.
        surface_content(move || {
            let config = crate::core::surfaces::config_for(output.as_deref());
            let theme = config.resolve_theme();
            let radius = config.panel_radius(edge);
            set_theme(theme);
            // So panel content that rounds to the bar radius (e.g. notification cards) matches inside the float too.
            crate::modules::drawer::set_content_radius(radius);
            let body = module_panel(&module).expect("float panel build failed");
            let style = SurfaceFrameStyle {
                background: config.panel_fill(),
                title_bar: theme.overlay,
                title_text: theme.text,
                close: theme.muted,
                radius,
                font_size: theme.font(FontRole::Title),
            };
            let close: Rc<dyn Fn()> = Rc::new(request_close);
            // The grip hands back the size the *surface* should take; rounding up rather than down keeps a half-pixel drag from shrinking the window by one every frame it is held still.
            let resize: Rc<dyn Fn(f32, f32)> =
                Rc::new(|w: f32, h: f32| request_size(w.ceil() as u32, h.ceil() as u32));
            surface_frame(module.clone(), style, close, body, Some(resize))
                .expect("surface frame build failed")
        }),
    )
}

#[cfg(test)]
mod tests {
    use crate::shared::theme::{FontRole, NordTheme};
    use crate::test_support::{render_png, render_png_frames};
    use telar::{
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
                font_size: theme.font(FontRole::Title),
            };
            let frame = surface_frame("Clock", style, std::rc::Rc::new(|| {}), body, None).unwrap();
            let root = SurfaceRoot::new(frame).expect("float surface root failed");
            let root = if self.animate {
                root.animate_in()
            } else {
                root
            };
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
        let Ok(out) = std::env::var("TELAR_VISUAL_FLOAT_OUT") else {
            eprintln!("set TELAR_VISUAL_FLOAT_OUT to render the float; skipping");
            return;
        };
        render_png(FloatPreviewApp { animate: false }, 360, 240, &out);
    }

    /// Renders the float with its enter animation, driving enough frames to settle — a guard that the animated path lands fully visible, not stuck transparent. Gated on its own env var.
    #[test]
    fn visual_float_anim_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_FLOAT_ANIM_OUT") else {
            eprintln!("set TELAR_VISUAL_FLOAT_ANIM_OUT to render the animated float; skipping");
            return;
        };
        let frames = std::env::var("HYPRSHELL_VISUAL_FRAMES")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(20);
        render_png_frames(FloatPreviewApp { animate: true }, 360, 240, &out, frames);
    }
}
