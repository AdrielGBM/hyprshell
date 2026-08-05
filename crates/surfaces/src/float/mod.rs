use std::rc::Rc;

use platform_wayland::{request_close, request_size};
use telar::{
    LayoutError, LayoutItem, LayoutStyle, RectStyle, StyledContainer, SurfaceFrameStyle,
    SurfaceToken, box_item, open_surface, set_theme, surface_content, surface_frame, use_theme,
};

use crate::drawer::{module_panel, panel_wants_keyboard};
use config::SurfaceEnv;
use config::theme::{FontRole, NordTheme};
use ui::placement::Placement;

/// Opens `module_id`'s panel as a centred, titled, closable window on the bar's own monitor, sized per its `[modules.<id>]` override or `[panels.float]`; the shell only declares the placement, the rsx surface host and `surface_frame` realize the window chrome. Toggle/close is the caller's job ([`crate::panel::toggle_panel`]) via the returned token.
///
/// `[modules.<id>]` (or `[panels.float]`) is the size the window *opens* at, not the size it is stuck at: the
/// frame's corner grip renegotiates the layer surface as it is dragged. That size lasts as long as the window
/// does and is deliberately not written back — persisting it would mean a config write, and a reload, per drag.
pub(crate) fn open_float(env: &SurfaceEnv, module_id: &str) -> SurfaceToken {
    let module = module_id.to_string();
    let edge = env.edge;
    let output = env.output.clone();
    let (width, height) = env.config.float_size_for(module_id);
    let placement = Placement::window((width, height), panel_wants_keyboard(module_id))
        .output(env.output.clone())
        .hosted_placement();
    open_surface(
        placement,
        // Resolved per build rather than captured: the window outlives the config it opened under, and a
        // rebuild is how it follows an edit. What is captured is which module it shows and where it is.
        surface_content(move || {
            let config = config::config_for(output.as_deref());
            let theme = config.resolve_theme();
            let radius = config.panel_radius(edge);
            set_theme(theme);
            // So panel content that rounds to the bar radius (e.g. notification cards) matches inside the float too.
            crate::drawer::set_content_radius(radius);
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

/// The window chrome a float is presented in — title bar, ✕ and a placeholder body — for [`crate::preview`].
/// The chrome rather than a module's panel, because *which* panel a float shows is the caller's choice and
/// every one of them already previews on its own.
pub(crate) fn frame_preview() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let body = box_item(StyledContainer::new(
        LayoutStyle::new().width(220.0).height(90.0),
        move |_| RectStyle::filled(theme.overlay, 8.0),
        vec![],
    )?);
    let style = SurfaceFrameStyle {
        background: theme.surface,
        title_bar: theme.overlay,
        title_text: theme.text,
        close: theme.muted,
        radius: 14.0,
        font_size: theme.font(FontRole::Title),
    };
    surface_frame("Clock", style, Rc::new(|| {}), body, None)
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use platform_headless::{FrameSink, HeadlessPlatform};
    use telar::{
        App, AppConfig, AppPathsProvider, Color, Component, SurfaceRoot, WindowConfig,
        reset_layout_runtime, run_with_platform, set_theme,
    };

    use config::theme::NordTheme;

    struct NullPaths;
    impl AppPathsProvider for NullPaths {
        fn config_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn data_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
        fn cache_dir(&self) -> Option<std::path::PathBuf> {
            None
        }
    }

    /// The float's chrome under the enter animation, which is the one thing a `[preview]` cannot show: the
    /// preview page renders a tree, and this is about what the *surface root* does to it over several frames.
    struct AnimatedFloat;

    impl App for AnimatedFloat {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(NordTheme::new());
            let frame = super::frame_preview().expect("float frame build failed");
            Box::new(
                SurfaceRoot::new(frame)
                    .expect("float surface root failed")
                    .animate_in(),
            )
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

    /// A float that animates in must *land*. The enter transition fades the whole surface from transparent, so
    /// a transition that never completes leaves a window the user cannot see — and every other check passes,
    /// because the tree is built, laid out and drawn exactly as it should be. Only the pixels say otherwise.
    #[test]
    fn a_float_that_animates_in_ends_up_visible() {
        const SIDE: u32 = 240;
        let sink: FrameSink = Arc::new(Mutex::new(None));
        // The headless platform paces at a real 60fps, so 20 frames is a comfortable margin over the 200ms
        // enter transition.
        let platform = HeadlessPlatform::new(SIDE, SIDE)
            .with_frames(20)
            .capture_into(sink.clone());
        run_with_platform::<_, _, ()>(
            platform,
            AppConfig::default(),
            Box::new(NullPaths) as Box<dyn AppPathsProvider>,
            AnimatedFloat,
            "hyprshell-float-test",
        )
        .expect("headless run");

        let pixels = sink.lock().unwrap().take().expect("a frame was captured");
        let opaque = pixels.chunks_exact(4).filter(|px| px[3] > 250).count();
        assert!(
            opaque > (SIDE * SIDE / 10) as usize,
            "the settled frame is {opaque} solid pixels of {}: the enter transition never finished",
            SIDE * SIDE
        );
    }
}
