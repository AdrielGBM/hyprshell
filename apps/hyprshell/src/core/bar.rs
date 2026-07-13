use rsx::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, StyledContainer,
};

use crate::core::config::{Config, Edge, ResolvedShape, Shape};
use crate::core::module::{
    ModuleClick, ModuleCtx, ModuleRegistry, module_foreground, module_shell, set_module_fg,
};
use crate::core::theme::NordTheme;

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
    let mut start: Vec<String> = Vec::new();
    start.extend(lead.map(str::to_string));
    start.extend(bar.start.iter().cloned());
    let mut end: Vec<String> = bar.end.clone();
    end.extend(trail.map(str::to_string));
    let zones = [
        (start.as_slice(), JustifyContent::START),
        (bar.center.as_slice(), JustifyContent::CENTER),
        (end.as_slice(), JustifyContent::END),
    ];
    let chrome = Chrome { edge, shape, theme };
    match shape.mode {
        Shape::Bar => build_whole_bar(config, &chrome, &zones, registry, &ctx),
        Shape::Sections => {
            build_units(config, &chrome, &zones, registry, &ctx, Granularity::Section)
        }
        Shape::Chips => build_units(config, &chrome, &zones, registry, &ctx, Granularity::Chip),
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

fn build_whole_bar(
    config: &Config,
    chrome: &Chrome,
    zones: &[(&[String], JustifyContent); 3],
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Chrome { edge, shape, theme } = *chrome;
    let spacing = shape.spacing as f32;
    let mut slots = Vec::with_capacity(3);
    for (ids, justify) in zones {
        // Modules blend into the shared bar surface (transparent rest); their shell just adds the hover
        // highlight and a small corner radius. STRETCH so every chip is the bar's height — text pills and
        // square icon chips line up (same as sections/chips modes).
        let items = build_items(config, ids, registry, ctx, Color::TRANSPARENT, 6.0)?;
        slots.push(zone(edge, *justify, spacing, AlignItems::STRETCH, items)?);
    }
    let radius = shape.radius as f32;
    let style = axis(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .padding_all(shape.padding() as f32),
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
    zones: &[(&[String], JustifyContent); 3],
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
    granularity: Granularity,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let Chrome { edge, shape, theme } = *chrome;
    let spacing = shape.spacing as f32;
    // Section: modules blend into a shared per-zone surface panel (transparent rest, wrapped in a `unit`).
    // Chip: each module IS its own free-standing surface pill — its shell provides the background, no `unit`.
    let (rest, shell_radius) = match granularity {
        Granularity::Section => (Color::TRANSPARENT, 6.0),
        Granularity::Chip => (theme.surface, shape.chip_radius() as f32),
    };
    let mut slots = Vec::with_capacity(3);
    for (ids, justify) in zones {
        let items = build_items(config, ids, registry, ctx, rest, shell_radius)?;
        let content: Vec<Box<dyn LayoutItem>> = if items.is_empty() {
            Vec::new()
        } else {
            match granularity {
                Granularity::Section => {
                    vec![unit(edge, shape.radius as f32, spacing, theme.surface, items)?]
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

/// A shared surface panel behind a zone's modules (sections mode). Its chips STRETCH to fill it (no inner
/// padding) so a filled chip reaches the panel's edges — otherwise the surface shows as a thin sliver beside
/// it. The panel groups the zone; the modules provide their own chip.
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

fn axis(style: LayoutStyle, edge: Edge) -> LayoutStyle {
    if edge.is_horizontal() {
        style.flex_row()
    } else {
        style.flex_column()
    }
}

/// Builds each module's content and wraps it in its base container (§3), resolving the per-module variant
/// and accent from config. `rest` is the container's resting background and `radius` its corner radius,
/// both chosen by the caller for the bar mode. A self-managed module (workspaces) is placed bare.
fn build_items(
    config: &Config,
    ids: &[String],
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
    rest: Color,
    radius: f32,
) -> Result<Vec<Box<dyn LayoutItem>>, LayoutError> {
    let mut items: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(ids.len());
    for id in ids {
        let variant = config.variant_for(id);
        let accent = ctx.theme.accent_by_name(config.accent_name_for(id));
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
        if def.is_some_and(|d| d.self_managed) {
            items.push(content);
            continue;
        }
        let on_press: Option<Box<dyn Fn()>> = match def.and_then(|d| d.click) {
            Some(ModuleClick::Drawer) => {
                let id = id.clone();
                Some(Box::new(move || crate::toggle_drawer(&id)))
            }
            Some(ModuleClick::Action(action)) => Some(Box::new(action)),
            None => None,
        };
        let square = def.is_some_and(|d| d.icon);
        items.push(module_shell(
            content, variant, rest, accent, ctx.theme, radius, square, on_press,
        )?);
    }
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::module::ModuleDef;
    use rsx::reset_layout_runtime;

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

    #[test]
    fn every_mode_builds_a_tree() {
        for mode in ["bar", "sections", "chips"] {
            let toml = format!(
                "[shape]\nmode=\"{mode}\"\ngap=6\nradius=10\nspacing=8\n\
                 [bars.top]\nstart=[\"dummy\"]\ncenter=[\"dummy\"]\nend=[\"dummy\"]\n"
            );
            let cfg: Config = toml::from_str(&toml).unwrap();
            reset_layout_runtime();
            let bar = build_bar(&cfg, Edge::Top, NordTheme::new().accent, &registry(), NordTheme::new());
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
            let bar = build_bar(&cfg, Edge::Top, NordTheme::new().accent, &registry(), NordTheme::new());
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
            build_bar(&cfg, Edge::Top, NordTheme::new().accent, &registry(), NordTheme::new()).is_ok()
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
                build_bar(&cfg, Edge::Left, NordTheme::new().accent, &registry(), NordTheme::new())
                    .is_ok(),
                "vertical {mode} builds"
            );
        }
    }
}
