use rsx::{
    AlignItems, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, StyledContainer,
};

use crate::core::config::{BarConfig, Edge};
use crate::core::module::{ModuleCtx, ModuleRegistry};
use crate::core::theme::NordTheme;

pub fn build_bar(
    edge: Edge,
    config: &BarConfig,
    accent: rsx::Color,
    registry: &ModuleRegistry,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ctx = ModuleCtx {
        theme,
        accent,
        bar_size: config.size,
        edge,
    };
    let start = build_zone(&config.start, registry, &ctx, JustifyContent::START)?;
    let center = build_zone(&config.center, registry, &ctx, JustifyContent::CENTER)?;
    let end = build_zone(&config.end, registry, &ctx, JustifyContent::END)?;

    let base = theme.base;
    let style = LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
        .align_items(AlignItems::CENTER);
    // The bar's main axis follows the edge: horizontal bars flow their zones left→right, vertical ones top→bottom. Padding runs along the main axis so the outer zones sit inset from the ends.
    let style = if edge.is_horizontal() {
        style.flex_row().padding_horizontal(12.0)
    } else {
        style.flex_column().padding_vertical(12.0)
    };
    Ok(Box::new(StyledContainer::new(
        style,
        move |_r| RectStyle::filled(base, 0.0),
        vec![start, center, end],
    )?))
}

fn build_zone(
    ids: &[String],
    registry: &ModuleRegistry,
    ctx: &ModuleCtx,
    justify: JustifyContent,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut items: Vec<Box<dyn LayoutItem>> = Vec::new();
    for id in ids {
        match registry.build(id, ctx) {
            Some(Ok(item)) => items.push(item),
            Some(Err(e)) => return Err(e),
            None => tracing::warn!("unknown module id: {id}"),
        }
    }
    let style = LayoutStyle::new()
        .flex_grow(1.0)
        .align_items(AlignItems::CENTER)
        .justify_content(justify)
        .gap(10.0);
    let style = if ctx.edge.is_horizontal() {
        style.flex_row()
    } else {
        style.flex_column()
    };
    Ok(Box::new(Container::new(style, items)?))
}
