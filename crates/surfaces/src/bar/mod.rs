mod app;
mod autohide;

pub use app::BarApp;
pub use autohide::{AutoHide, RevealMargins};

use telar::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, Slots, StyledContainer, track_layout,
};

use config::theme::NordTheme;
use config::{Config, Edge, ModuleEntry, ResolvedShape, Shape, Zone};
use ui::module::{
    DragOpen, ModuleClick, ModuleCtx, ModuleDef, ModuleRegistry, module_foreground, set_module_fg,
};
use ui::{ModuleShellProps, module_shell};

/// The bar the running config draws, for [`crate::preview`] — every chip the user put on it, in the zones and
/// the shape they configured, against the registry the app installed.
pub(crate) fn preview() -> Result<Box<dyn LayoutItem>, LayoutError> {
    let env = ui::preview::bar_chip();
    let theme = env.config.resolve_theme();
    ui::module::with_registry(|registry| {
        build_bar(&env.config, env.edge, theme.accent, registry, theme)
    })
}

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
        (start.as_slice(), Zone::Start),
        (bar.center.as_slice(), Zone::Center),
        (end.as_slice(), Zone::End),
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

/// A bar's three zones, each the entries placed in it.
type Zones<'a> = [(&'a [ModuleEntry], Zone); 3];

fn justify(zone: Zone) -> JustifyContent {
    match zone {
        Zone::Start => JustifyContent::START,
        Zone::Center => JustifyContent::CENTER,
        Zone::End => JustifyContent::END,
    }
}

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

/// What a bar paints its own background with: the token at `[bars] opacity`, or nothing at all while a frame
/// is up, because the frame draws the ring covering exactly these strips and two fills stacking is a darker
/// band along every edge they share.
fn bar_fill(config: &Config, token: Color) -> Color {
    if config.shape.frame {
        return Color::TRANSPARENT;
    }
    token.with_alpha(config.opacity())
}

fn build_whole_bar(
    config: &Config,
    chrome: &Chrome,
    zones: &Zones,
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Chrome { edge, shape, theme } = *chrome;
    // With a frame up, the ring it draws already fills the strip this bar sits in. Painting again on top is
    // what made two translucent fills stack and darken along the edges the two share.
    let base = bar_fill(config, theme.base);
    let spacing = shape.spacing;
    let mut slots = Vec::with_capacity(3);
    for (entries, in_zone) in zones {
        // Modules blend into the shared surface (transparent rest); STRETCH makes every chip the bar's height so text pills and icon chips line up. The hover/press (and Filled) highlight rounds at the theme's chip radius, matching chip mode.
        let items = build_items(
            config,
            entries,
            registry,
            ctx,
            Color::TRANSPARENT,
            shape.chip_radius(),
        )?;
        slots.push(zone(
            edge,
            justify(*in_zone),
            spacing,
            AlignItems::STRETCH,
            items,
        )?);
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
        move |_r| RectStyle::filled(base, radius),
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
    let surface = bar_fill(config, theme.surface);
    let (rest, shell_radius) = match granularity {
        Granularity::Section => (Color::TRANSPARENT, shape.chip_radius()),
        Granularity::Chip => (surface, shape.chip_radius()),
    };
    let mut slots = Vec::with_capacity(3);
    for (entries, in_zone) in zones {
        let items = build_items(config, entries, registry, ctx, rest, shell_radius)?;
        let content: Vec<Box<dyn LayoutItem>> = if items.is_empty() {
            Vec::new()
        } else {
            match granularity {
                Granularity::Section => {
                    vec![unit(edge, shape.radius, spacing, surface, items)?]
                }
                // The shells already are the chips; place them directly.
                Granularity::Chip => items,
            }
        };
        // STRETCH ensures height is parent-driven by bar size, not content-driven.
        slots.push(zone(
            edge,
            justify(*in_zone),
            spacing,
            AlignItems::STRETCH,
            content,
        )?);
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
        wrapper =
            wrapper.on_hover(move |entered| crate::popout::hover(&module, rect.get(), entered));
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
    let threshold = config::surface_env()?.config.panels.drag_threshold()?;
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
        // Handed over bare: the chip shell dispatches every press with its own rect in scope, `Panel` and `Action` alike, so whatever this opens can hang off the chip without being told where it is.
        let on_press: Option<Box<dyn Fn()>> = match def.and_then(|d| d.click) {
            Some(ModuleClick::Panel) => {
                let id = id.clone();
                Some(Box::new(move || crate::panel::toggle_panel(&id)))
            }
            Some(ModuleClick::Action(action)) => Some(Box::new(action)),
            None => None,
        };
        let mut inner = Slots::new();
        inner.push(None, content);
        let chip = module_shell(
            ModuleShellProps {
                variant,
                rest,
                accent,
                radius,
                square: def.is_some_and(|d| d.icon),
                on_press,
                on_scroll: def
                    .and_then(|d| d.scroll)
                    .map(|wheel| Box::new(wheel) as Box<dyn Fn(f32, f32)>),
                drag_open: drag_open_for(id, def, ctx.edge),
            },
            inner,
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
    use telar::{AvailableSpace, compute_layout, reset_layout_runtime, set_theme};
    use ui::module::ModuleDef;

    fn dummy(_ctx: &ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        Ok(Box::new(StyledContainer::new(
            LayoutStyle::new().width(20.0).height(20.0),
            |_r| RectStyle::filled(telar::Color::from_rgb_u8(255, 255, 255), 0.0),
            vec![],
        )?))
    }

    fn registry() -> ModuleRegistry {
        let mut r = ModuleRegistry::new();
        r.register("dummy", ModuleDef::new(dummy));
        r
    }

    thread_local! {
        static BUILT: std::cell::RefCell<Vec<&'static str>> =
            const { std::cell::RefCell::new(Vec::new()) };
    }

    fn wanted(ctx: &ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        BUILT.with(|built| built.borrow_mut().push("wanted"));
        dummy(ctx)
    }

    fn unwanted(ctx: &ModuleCtx) -> Result<Box<dyn LayoutItem>, LayoutError> {
        BUILT.with(|built| built.borrow_mut().push("unwanted"));
        dummy(ctx)
    }

    /// A module no zone names costs nothing at all.
    ///
    /// The registry holds every module the shell ships, so a build that walked *it* rather than the zones would
    /// construct a chip nobody asked for — and constructing a chip is what subscribes it, which turns an unused
    /// module into a running service. That is the residency half of the rule, reached through the one door that
    /// makes it invisible: the widget tree, where an extra chip is off-screen rather than wrong.
    #[test]
    fn a_module_no_zone_names_is_never_built() {
        let mut registry = ModuleRegistry::new();
        registry.register("wanted", ModuleDef::new(wanted));
        registry.register("unwanted", ModuleDef::new(unwanted));

        reset_layout_runtime();
        set_theme(NordTheme::new());
        BUILT.with(|built| built.borrow_mut().clear());
        let config: config::Config =
            toml::from_str("[bars.top]\nsize=34\ncenter=[\"wanted\"]\n").unwrap();

        build_bar(
            &config,
            Edge::Top,
            NordTheme::new().accent,
            &registry,
            NordTheme::new(),
        )
        .expect("the bar builds");

        assert_eq!(
            BUILT.with(|built| built.borrow().clone()),
            vec!["wanted"],
            "only the module the config put on the bar was built"
        );
    }

    /// A chip that carries a hover popout is wrapped in an extra box to track the pointer. That box sits
    /// between the zone and the chip, so a press has to pass through it — and a wrapper that swallowed one
    /// would leave every popout-bearing chip (volume, brightness, media, mic, battery) looking dead to a
    /// click while still opening its card on hover.
    #[test]
    fn a_popout_wrapper_lets_a_click_through_to_the_chip() {
        use std::cell::Cell;
        use std::rc::Rc;
        use telar::{AvailableSpace, Event, PointerButton, PointerSource, compute_layout};

        let clicked = Rc::new(Cell::new(false));
        let sink = Rc::clone(&clicked);
        reset_layout_runtime();
        set_theme(NordTheme::new());
        let mut inner = Slots::new();
        inner.push(
            None,
            dummy(&ModuleCtx {
                theme: NordTheme::new(),
                accent: NordTheme::new().accent,
                bar_size: 32,
                edge: Edge::Top,
            })
            .unwrap(),
        );
        let chip = module_shell(
            ModuleShellProps {
                variant: config::Variant::Default,
                rest: Color::TRANSPARENT,
                accent: NordTheme::new().accent,
                radius: 8.0,
                square: true,
                on_press: Some(Box::new(move || sink.set(true))),
                ..Default::default()
            },
            inner,
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
            set_theme(NordTheme::new());
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
            set_theme(NordTheme::new());
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
        set_theme(NordTheme::new());
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
            set_theme(NordTheme::new());
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
}
