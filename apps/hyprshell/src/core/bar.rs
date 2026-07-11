use rsx::{
    AlignItems, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle, RectStyle,
    SizeDimension, StyledContainer,
};

use crate::core::config::BarConfig;
use crate::core::module::{ModuleCtx, ModuleRegistry};
use crate::core::theme::NordTheme;

pub fn build_bar(
    config: &BarConfig,
    accent: rsx::Color,
    registry: &ModuleRegistry,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let ctx = ModuleCtx {
        theme,
        accent,
        bar_height: config.height,
    };
    let left = build_zone(&config.left, registry, &ctx, JustifyContent::START)?;
    let center = build_zone(&config.center, registry, &ctx, JustifyContent::CENTER)?;
    let right = build_zone(&config.right, registry, &ctx, JustifyContent::END)?;

    let base = theme.base;
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0))
            .height(SizeDimension::Percent(1.0))
            .align_items(AlignItems::CENTER)
            .padding_horizontal(12.0),
        move |_r| RectStyle::filled(base, 0.0),
        vec![left, center, right],
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
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .flex_grow(1.0)
            .align_items(AlignItems::CENTER)
            .justify_content(justify)
            .gap(10.0),
        items,
    )?))
}
