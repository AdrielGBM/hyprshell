//! Which module has a hover card, and what builds it.
//!
//! The third half of the composition root, beside [`crate::core::registry`] and [`crate::core::panels`]: the
//! popout is one surface that opens under whichever chip was hovered, and this is where it learns what to draw
//! there without naming a module itself.

use ui::popouts::PopoutRegistry;

pub fn default_popouts() -> PopoutRegistry {
    modules::popout_cards::cards()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use config::Config;

    use super::*;

    /// Every card is offered for a module that exists, so no hover can be wired to an id nothing puts on a bar.
    #[test]
    fn every_card_belongs_to_a_registered_module() {
        let popouts = default_popouts();
        let modules = crate::core::registry::default_registry(&popouts);
        for id in popouts.ids() {
            assert!(
                modules.def(&id).is_some(),
                "'{id}' has a popout card but is not a registered module"
            );
        }
    }

    /// Renders one popout card for eyeballing. `HYPRSHELL_VISUAL_POPOUT` names the module (default `volume`);
    /// gated on its own env var like every other visual test.
    #[test]
    fn visual_popout_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_POPOUT_OUT") else {
            eprintln!("set TELAR_VISUAL_POPOUT_OUT to render a popout; skipping");
            return;
        };
        ui::popouts::install(default_popouts());
        let module =
            std::env::var("HYPRSHELL_VISUAL_POPOUT").unwrap_or_else(|_| "volume".to_string());
        let config = Config::starter();
        let (w, h) = (
            config.popouts.card_width() as u32,
            config.popouts.card_height() as u32,
        );
        // Published so the card resolves it exactly as it would on a live screen; a visual render has no
        // reconcile to have put one there.
        config::set_config(Arc::new(config));
        visual::render_png(
            surfaces::popout::PopoutApp {
                module,
                edge: config::Edge::Top,
                bar_size: 34,
                output: None,
            },
            w,
            h,
            &out,
        );
    }
}
