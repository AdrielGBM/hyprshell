use rsx::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, StyledContainer, track_layout,
};

use crate::core::config::{Config, Edge, ModuleEntry, ResolvedShape, Shape};
use crate::shared::module::{
    ChipStyle, DragOpen, ModuleClick, ModuleCtx, ModuleDef, ModuleRegistry, module_foreground,
    module_shell, set_module_fg,
};
use crate::shared::theme::NordTheme;

/// Builds the content tree for the bar, branching on its resolved `mode` (bar/sections/chips); visual properties come from gap/spacing/radius, not mode.
pub fn build_bar(
    config: &Config,
    edge: Edge,
    accent: Color,
    registry: &ModuleRegistry,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let bar = config.bars.get(edge);
    let shape = config.shape_for(edge);
    let ctx = ModuleCtx {
        theme,
        accent,
        bar_size: bar.size,
        edge,
    };
    // `[corners]` sugar: corner modules are routed to the owning bar's start/end zones, not separate surfaces.
    let (lead, trail) = config.corner_modules_for(edge);
    let mut start: Vec<ModuleEntry> = Vec::new();
    start.extend(lead.map(ModuleEntry::bare));
    start.extend(bar.start.iter().cloned());
    let mut end: Vec<ModuleEntry> = bar.end.clone();
    end.extend(trail.map(ModuleEntry::bare));
    let zones: Zones = [
        (start.as_slice(), JustifyContent::START),
        (bar.center.as_slice(), JustifyContent::CENTER),
        (end.as_slice(), JustifyContent::END),
    ];
    let chrome = Chrome { edge, shape, theme };
    match shape.mode {
        Shape::Bar => build_whole_bar(config, &chrome, &zones, registry, &ctx),
        Shape::Sections => build_units(
            config,
            &chrome,
            &zones,
            registry,
            &ctx,
            Granularity::Section,
        ),
        Shape::Chips => build_units(config, &chrome, &zones, registry, &ctx, Granularity::Chip),
    }
}

/// A bar's three zones, each the entries placed in it and how they pack along the bar.
type Zones<'a> = [(&'a [ModuleEntry], JustifyContent); 3];

#[derive(Clone, Copy)]
struct Chrome {
    edge: Edge,
    shape: ResolvedShape,
    theme: NordTheme,
}

#[derive(Clone, Copy)]
enum Granularity {
    Section,
    Chip,
}

fn build_whole_bar(
    config: &Config,
    chrome: &Chrome,
    zones: &Zones,
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Chrome { edge, shape, theme } = *chrome;
    let spacing = shape.spacing;
    let mut slots = Vec::with_capacity(3);
    for (entries, justify) in zones {
        // Modules blend into the shared surface (transparent rest); STRETCH makes every chip the bar's height so text pills and icon chips line up. The hover/press (and Filled) highlight rounds at the theme's chip radius, matching chip mode.
        let items = build_items(
            config,
            entries,
            registry,
            ctx,
            Color::TRANSPARENT,
            shape.chip_radius(),
        )?;
        slots.push(zone(edge, *justify, spacing, AlignItems::STRETCH, items)?);
    }
    let radius = shape.radius;
    let style = axis(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .padding_all(shape.padding()),
        edge,
    );
    Ok(Box::new(StyledContainer::new(
        style,
        move |_r| RectStyle::filled(theme.base, radius),
        slots,
    )?))
}

fn build_units(
    config: &Config,
    chrome: &Chrome,
    zones: &Zones,
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
    granularity: Granularity,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Chrome { edge, shape, theme } = *chrome;
    let spacing = shape.spacing;
    // Section: modules share a per-zone surface panel (wrapped in `unit`); Chip: each module is its own free-standing pill, no `unit`.
    let (rest, shell_radius) = match granularity {
        Granularity::Section => (Color::TRANSPARENT, shape.chip_radius()),
        Granularity::Chip => (theme.surface, shape.chip_radius()),
    };
    let mut slots = Vec::with_capacity(3);
    for (entries, justify) in zones {
        let items = build_items(config, entries, registry, ctx, rest, shell_radius)?;
        let content: Vec<Box<dyn LayoutItem>> = if items.is_empty() {
            Vec::new()
        } else {
            match granularity {
                Granularity::Section => {
                    vec![unit(edge, shape.radius, spacing, theme.surface, items)?]
                }
                // The shells already are the chips; place them directly.
                Granularity::Chip => items,
            }
        };
        // STRETCH ensures height is parent-driven by bar size, not content-driven.
        slots.push(zone(edge, *justify, spacing, AlignItems::STRETCH, content)?);
    }
    let style = axis(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::STRETCH)
            .gap(spacing),
        edge,
    );
    Ok(Box::new(Container::new(style, slots)?))
}

/// Shared surface panel behind a zone's modules (sections mode); children STRETCH with no inner padding so a filled chip reaches the panel edges instead of leaving a thin sliver.
fn unit(
    edge: Edge,
    radius: f32,
    spacing: f32,
    fill: Color,
    items: Vec<Box<dyn LayoutItem>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = axis(
        LayoutStyle::new()
            .align_items(AlignItems::STRETCH)
            .justify_content(JustifyContent::CENTER)
            .gap(spacing),
        edge,
    );
    Ok(Box::new(StyledContainer::new(
        style,
        move |_r| RectStyle::filled(fill, radius),
        items,
    )?))
}

fn zone(
    edge: Edge,
    justify: JustifyContent,
    spacing: f32,
    cross: AlignItems,
    items: Vec<Box<dyn LayoutItem>>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = axis(
        LayoutStyle::new()
            .flex_grow(1.0)
            .align_items(cross)
            .justify_content(justify)
            .gap(spacing),
        edge,
    );
    Ok(Box::new(Container::new(style, items)?))
}

/// An invisible box wrapped around a module's own content to carry what its chip cannot: a wheel handler for a
/// self-managed module (which has no [`module_shell`] to put one on), and the pointer tracking behind a hover
/// popout. Both live here rather than on the chip so a self-managed module gets them on the same terms as any
/// other; the wrapper shrink-wraps its child, so the rect it tracks is the chip's own.
///
/// `cross` is what the wrapper would otherwise silently change. A chip is a direct zone child under
/// `AlignItems::STRETCH`, so it fills the bar's thickness; a wrapper that centred it instead would shrink every
/// popout-bearing chip to its content. A self-managed module lays itself out and is centred, as it was before
/// any wrapper existed.
///
/// It runs along the bar for the same reason [`zone`] does. A wrapper fixed to a row applies `cross` across the
/// *screen's* vertical, so on a left or right bar it stretched each chip's height — which is already its
/// content — and left its width free: every wrapped chip then sat at its own content width, ragged against the
/// bar's inner edge, with the wide ones running off the screen. Thirteen chips carry a popout, so on a vertical
/// bar that was most of them.
fn chip_wrapper(
    content: Box<dyn LayoutItem>,
    module_id: &str,
    on_scroll: Option<fn(f32, f32)>,
    popout: bool,
    cross: AlignItems,
    edge: Edge,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let style = axis(LayoutStyle::new().align_items(cross).flex_shrink(0.0), edge);
    let mut wrapper = StyledContainer::new(
        style,
        |_r| RectStyle::filled(Color::TRANSPARENT, 0.0),
        vec![content],
    )?;
    if let Some(on_scroll) = on_scroll {
        wrapper = wrapper.on_scroll(on_scroll);
    }
    if popout {
        // Tracked before the handler that reads it is attached, so the popout has a rect the first time the pointer arrives.
        let rect = track_layout(wrapper.layout_node())
            .expect("a container registers its rect")
            .read_only();
        let module = module_id.to_string();
        wrapper = wrapper
            .on_hover(move |entered| crate::modules::popout::hover(&module, rect.get(), entered));
    }
    Ok(Box::new(wrapper))
}

/// The drag-to-open gesture for a chip, when it has a panel to open and the gesture is switched on.
///
/// Only a module whose click *opens a panel* gets one: dragging a volume chip has nothing to open, and arming
/// a gesture that can only do nothing would still cancel the tap that does something.
fn drag_open_for(id: &str, def: Option<&ModuleDef>, edge: Edge) -> Option<DragOpen> {
    if !matches!(def?.click, Some(ModuleClick::Panel)) {
        return None;
    }
    let threshold = crate::shared::module::surface_env()?
        .config
        .panels
        .drag_threshold()?;
    Some(DragOpen {
        module: id.to_string(),
        edge,
        threshold,
    })
}

fn axis(style: LayoutStyle, edge: Edge) -> LayoutStyle {
    if edge.is_horizontal() {
        style.flex_row()
    } else {
        style.flex_column()
    }
}

/// Builds each entry's content and wraps it in its base container. The variant and accent come from the entry
/// when it names them and from `[modules.<id>]` otherwise, which is what lets the same module sit on a bar
/// twice looking different.
fn build_items(
    config: &Config,
    entries: &[ModuleEntry],
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
    rest: Color,
    radius: f32,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let mut items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(entries.len());
    for entry in entries {
        let id = &entry.id;
        let variant = config.entry_variant(entry);
        let accent = ctx.theme.accent_by_name(config.entry_accent_name(entry));
        // Set the foreground BEFORE building the content so `module_fg()` snapshots this module's color.
        set_module_fg(module_foreground(variant, accent, ctx.theme));
        let content = match registry.build(id, ctx) {
            Some(Ok(content)) => content,
            Some(Err(e)) => return Err(e),
            None => {
                tracing::warn!("unknown module id: {id}");
                continue;
            }
        };
        let def = registry.def(id);
        let popout = def.is_some_and(|d| d.popout) && config.popouts.enabled;
        if def.is_some_and(|d| d.self_managed) {
            // A self-managed module skips `module_shell` — it paints its own layout — so its wheel handler and popout tracking go on a bare wrapper with no padding, fill or hover state.
            let scroll = def.and_then(|d| d.scroll);
            if scroll.is_none() && !popout {
                items.push(content);
            } else {
                items.push(chip_wrapper(
                    content,
                    id,
                    scroll,
                    popout,
                    AlignItems::CENTER,
                    ctx.edge,
                )?);
            }
            continue;
        }
        let on_press: Option<Box<dyn Fn()>> = match def.and_then(|d| d.click) {
            Some(ModuleClick::Panel) => {
                let id = id.clone();
                Some(Box::new(move || crate::toggle_panel(&id)))
            }
            Some(ModuleClick::Action(action)) => Some(Box::new(action)),
            None => None,
        };
        let style = ChipStyle {
            variant,
            rest,
            accent,
            theme: ctx.theme,
            radius,
            square: def.is_some_and(|d| d.icon),
        };
        let chip = module_shell(
            content,
            style,
            on_press,
            def.and_then(|d| d.scroll),
            drag_open_for(id, def, ctx.edge),
        )?;
        // Outside the chip rather than on it: the chip's own hover already swaps its paint, and stacking a second meaning onto that callback would tie the two together.
        items.push(if popout {
            chip_wrapper(chip, id, None, true, AlignItems::STRETCH, ctx.edge)?
        } else {
            chip
        });
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::module::ModuleDef;
    use rsx::{AvailableSpace, compute_layout, reset_layout_runtime, set_theme};

    fn dummy(_ctx: &ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(Box::new(StyledContainer::new(
            LayoutStyle::new().width(20.0).height(20.0),
            |_r| RectStyle::filled(rsx::Color::from_rgb_u8(255, 255, 255), 0.0),
            vec![],
        )?))
    }

    fn registry() -> ModuleRegistry {
        let mut r = ModuleRegistry::new();
        r.register("dummy", ModuleDef::new(dummy));
        r
    }

    /// A chip that carries a hover popout is wrapped in an extra box to track the pointer. That box sits
    /// between the zone and the chip, so a press has to pass through it — and a wrapper that swallowed one
    /// would leave every popout-bearing chip (volume, brightness, media, mic, battery) looking dead to a
    /// click while still opening its card on hover.
    #[test]
    fn a_popout_wrapper_lets_a_click_through_to_the_chip() {
        use rsx::{AvailableSpace, Event, PointerButton, PointerSource, compute_layout};
        use std::cell::Cell;
        use std::rc::Rc;

        let clicked = Rc::new(Cell::new(false));
        let sink = Rc::clone(&clicked);
        reset_layout_runtime();
        let chip = module_shell(
            dummy(&ModuleCtx {
                theme: NordTheme::new(),
                accent: NordTheme::new().accent,
                bar_size: 32,
                edge: Edge::Top,
            })
            .unwrap(),
            ChipStyle {
                variant: crate::core::config::Variant::Default,
                rest: Color::TRANSPARENT,
                accent: NordTheme::new().accent,
                theme: NordTheme::new(),
                radius: 8.0,
                square: true,
            },
            Some(Box::new(move || sink.set(true))),
            None,
            None,
        )
        .unwrap();
        let mut wrapped =
            chip_wrapper(chip, "volume", None, true, AlignItems::STRETCH, Edge::Top).unwrap();

        let node = wrapped.layout_node();
        compute_layout(
            node,
            AvailableSpace::Definite(200.0),
            AvailableSpace::Definite(32.0),
        )
        .unwrap();

        let (x, y) = (10.0, 10.0);
        wrapped.on_event(&Event::PointerPressed {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        wrapped.on_event(&Event::PointerReleased {
            x,
            y,
            button: PointerButton::Primary,
            source: PointerSource::Mouse,
        });
        assert!(clicked.get(), "the chip's own press handler never fired");
    }

    #[test]
    fn every_mode_builds_a_tree() {
        for mode in ["bar", "sections", "chips"] {
            let toml = format!(
                "[shape]\nmode=\"{mode}\"\ngap=6\nradius=10\nspacing=8\n\
                 [bars.top]\nstart=[\"dummy\"]\ncenter=[\"dummy\"]\nend=[\"dummy\"]\n"
            );
            let cfg: Config = toml::from_str(&toml).unwrap();
            reset_layout_runtime();
            let bar = build_bar(
                &cfg,
                Edge::Top,
                NordTheme::new().accent,
                &registry(),
                NordTheme::new(),
            );
            assert!(bar.is_ok(), "mode {mode} builds a tree");
        }
    }

    #[test]
    fn corner_module_routes_into_owning_bar() {
        for mode in ["bar", "sections", "chips"] {
            let cfg: Config = toml::from_str(&format!(
                "[shape]\nmode=\"{mode}\"\n[bars.top]\ncenter=[\"dummy\"]\n[corners]\ntop_left=\"dummy\"\n"
            ))
            .unwrap();
            reset_layout_runtime();
            let bar = build_bar(
                &cfg,
                Edge::Top,
                NordTheme::new().accent,
                &registry(),
                NordTheme::new(),
            );
            assert!(bar.is_ok(), "corner routing builds in mode {mode}");
        }
    }

    #[test]
    fn center_only_sections_builds_a_notch() {
        let cfg: Config = toml::from_str(
            "[shape]\nmode=\"sections\"\ngap=8\nradius=12\n[bars.top]\ncenter=[\"dummy\"]\n",
        )
        .unwrap();
        reset_layout_runtime();
        assert!(
            build_bar(
                &cfg,
                Edge::Top,
                NordTheme::new().accent,
                &registry(),
                NordTheme::new()
            )
            .is_ok()
        );
    }

    #[test]
    fn vertical_bar_builds_in_every_mode() {
        for mode in ["bar", "sections", "chips"] {
            let toml = format!(
                "[shape]\nmode=\"{mode}\"\nradius=8\n[bars.left]\nsize=44\nstart=[\"dummy\"]\nend=[\"dummy\"]\n"
            );
            let cfg: Config = toml::from_str(&toml).unwrap();
            reset_layout_runtime();
            assert!(
                build_bar(
                    &cfg,
                    Edge::Left,
                    NordTheme::new().accent,
                    &registry(),
                    NordTheme::new()
                )
                .is_ok(),
                "vertical {mode} builds"
            );
        }
    }

    /// Every chip fills the bar's thickness, whichever edge it is on.
    ///
    /// The regression: the wrapper a popout-bearing chip sits in was fixed to a row, so on a left or right bar
    /// it stretched the chip's height (already its content) and left the width free. Each wrapped chip then
    /// took its own content width, ragged against the bar's inner edge, and the wide ones ran off the screen.
    /// Building proves none of that — the wrapper builds happily either way — so this lays a real bar out and
    /// measures the chips.
    #[test]
    fn a_wrapped_chip_fills_the_bar_thickness_on_a_vertical_bar() {
        let side = 44.0;
        // A chip wider than the bar is the case that shows the bug: a clock, a window title, a netspeed
        // readout. Its own width is content-driven, so only `align-items: stretch` on the right axis reins
        // it in.
        let content_width = 120.0;

        for edge in [Edge::Left, Edge::Top] {
            reset_layout_runtime();
            set_theme(NordTheme::new());

            let inner =
                Container::new(LayoutStyle::new().width(content_width).height(20.0), vec![])
                    .unwrap();
            let chip =
                Container::new(LayoutStyle::new().flex_row(), vec![Box::new(inner)]).unwrap();
            let chip_node = chip.layout_node();
            let chip_rect = track_layout(chip_node).expect("the chip registers its rect");

            let wrapped = chip_wrapper(
                Box::new(chip),
                "clock",
                None,
                true,
                AlignItems::STRETCH,
                edge,
            )
            .unwrap();
            // The zone the bar puts a chip in: along the bar, stretching its children across it.
            let zone = zone(
                edge,
                JustifyContent::START,
                0.0,
                AlignItems::STRETCH,
                vec![wrapped],
            )
            .unwrap();
            let (w, h) = if edge.is_vertical() {
                (side, 600.0)
            } else {
                (600.0, side)
            };
            compute_layout(
                zone.layout_node(),
                AvailableSpace::Definite(w),
                AvailableSpace::Definite(h),
            )
            .unwrap();

            let rect = chip_rect.get();
            let across = if edge.is_vertical() {
                rect.width
            } else {
                rect.height
            };
            assert_eq!(
                across, side,
                "{edge:?}: a {content_width}px chip measures {across} across a {side}px bar — it should be \
                 reined in to the bar's thickness, not left at its content width to spill off the screen"
            );
        }
    }

    use crate::core::app::BarApp;
    use crate::test_support::render_png;
    use std::sync::Arc;

    const DEMO: &str = r#"
[shape]
mode = "chips"
gap = 8
spacing = 8
radius = 12

[bars.top]
size = 40
start = ["workspaces"]
center = ["clock"]
end = ["clock"]
"#;

    fn edge_from_env() -> Edge {
        match std::env::var("HYPRSHELL_VISUAL_EDGE").as_deref() {
            Ok("bottom") => Edge::Bottom,
            Ok("left") => Edge::Left,
            Ok("right") => Edge::Right,
            _ => Edge::Top,
        }
    }

    fn size_for(edge: Edge, config: &Config) -> (u32, u32) {
        if let Ok(s) = std::env::var("HYPRSHELL_VISUAL_SIZE")
            && let Some((w, h)) = s.split_once('x')
            && let (Ok(w), Ok(h)) = (w.parse(), h.parse())
        {
            return (w, h);
        }
        let thickness = config.bars.get(edge).size;
        if edge.is_horizontal() {
            (1280, thickness)
        } else {
            (thickness, 800)
        }
    }

    /// Renders a bar surface for eyeballing. Env: `HYPRSHELL_VISUAL_CONFIG` (a config.toml, else a demo), `HYPRSHELL_VISUAL_EDGE` (top|bottom|left|right), `HYPRSHELL_VISUAL_SIZE` (WxH). Gated on `RSX_VISUAL_OUT`.
    #[test]
    fn visual_bar_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_OUT") else {
            eprintln!("set RSX_VISUAL_OUT to write a PNG; skipping");
            return;
        };
        let toml = std::env::var("HYPRSHELL_VISUAL_CONFIG")
            .ok()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .unwrap_or_else(|| DEMO.to_string());
        let config: Config = toml::from_str(&toml).expect("config parses");
        let edge = edge_from_env();
        let (w, h) = size_for(edge, &config);
        render_png(
            BarApp {
                config: Arc::new(config),
                edge,
                output: None,
            },
            w,
            h,
            &out,
        );
    }
}
