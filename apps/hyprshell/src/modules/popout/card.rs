//! The shape every popout takes.
//!
//! A popout is a glance, not a panel: a heading, at most one meter, and a handful of label/value rows. Saying
//! that once — as data a module fills in rather than a tree each module builds — is what keeps twelve of them
//! looking like one shell instead of twelve small designs.

use rsx::{
    AlignItems, Color, Container, LayoutError, LayoutItem, LayoutStyle, RectStyle, SizeDimension,
    StyledContainer, Text, TextStyle, box_item,
};

use crate::shared::icon::icon_view;
use crate::shared::reactive::{Live, fixed_text};
use crate::shared::theme::{FontRole, NordTheme};
use crate::shared::widget;

const METER_HEIGHT: f32 = 6.0;
const HEADER_ICON: f32 = 26.0;

/// One popout's content, described rather than built.
pub struct Card {
    icon: Option<Live<String>>,
    icon_tint: Option<Live<Color>>,
    title: Live<String>,
    subtitle: Option<Live<String>>,
    meter: Option<(Live<f32>, Live<Color>)>,
    rows: Vec<(Live<String>, Live<String>)>,
}

impl Card {
    pub fn new(title: Live<String>) -> Self {
        Self {
            icon: None,
            icon_tint: None,
            title,
            subtitle: None,
            meter: None,
            rows: Vec::new(),
        }
    }

    pub fn titled(title: impl Into<String>) -> Self {
        Self::new(fixed_text(title))
    }

    pub fn icon(mut self, glyph: Live<String>) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub fn icon_tint(mut self, tint: Live<Color>) -> Self {
        self.icon_tint = Some(tint);
        self
    }

    pub fn subtitle(mut self, text: Live<String>) -> Self {
        self.subtitle = Some(text);
        self
    }

    /// A 0..1 bar under the heading. Only one per card: a popout with two meters is a dashboard card (F8), and
    /// this is not the surface for one.
    pub fn meter(mut self, fraction: Live<f32>, tint: Live<Color>) -> Self {
        self.meter = Some((fraction, tint));
        self
    }

    pub fn row(mut self, label: Live<String>, value: Live<String>) -> Self {
        self.rows.push((label, value));
        self
    }

    pub fn build(self, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let caption = theme.font(FontRole::Caption);
        let mut children: Vec<Box<dyn LayoutItem>> = vec![header(&self, theme)?];
        if let Some((fraction, tint)) = self.meter {
            children.push(widget::meter(fraction, tint, theme.overlay, METER_HEIGHT)?);
        }
        for (label, value) in self.rows {
            children.push(widget::label_value(
                label,
                value,
                caption,
                theme.muted,
                theme.text,
            )?);
        }
        Ok(Box::new(Container::new(
            LayoutStyle::new()
                .flex_column()
                .width(SizeDimension::Percent(1.0))
                .gap(8.0),
            children,
        )?))
    }
}

fn header(card: &Card, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let mut labels: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(2);
    let title = card.title.clone();
    labels.push(box_item(Text::auto(
        move || title.get(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Title), theme.text),
    )?));
    if let Some(subtitle) = card.subtitle.clone() {
        labels.push(box_item(Text::auto(
            move || subtitle.get(),
            LayoutStyle::new(),
            move || TextStyle::new(theme.font(FontRole::Caption), theme.subtle),
        )?));
    }

    let mut content: Vec<Box<dyn LayoutItem>> = Vec::with_capacity(2);
    if let Some(glyph) = card.icon.clone() {
        let tint = card.icon_tint.clone();
        content.push(icon_view(
            move || glyph.get(),
            move || tint.as_ref().map(|t| t.get()).unwrap_or(theme.text),
            HEADER_ICON,
        )?);
    }
    content.push(Box::new(Container::new(
        LayoutStyle::new().flex_column().flex_grow(1.0).gap(1.0),
        labels,
    )?));

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .width(SizeDimension::Percent(1.0))
            .gap(10.0),
        content,
    )?))
}

/// The card's own box: the surface token at the bar's radius, and the pointer tracking that keeps the popout
/// up while it is hovered. `on_hover` is also what registers the box as an interactive target, which is how
/// the surface knows which part of itself to take input over.
pub fn frame(
    content: Box<dyn LayoutItem>,
    theme: NordTheme,
    width: f32,
    radius: f32,
    on_hover: impl Fn(bool) + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new()
                .flex_column()
                .width(width)
                .padding_all(12.0)
                .flex_shrink(0.0),
            move |_r| RectStyle::filled(theme.surface, radius),
            vec![content],
        )?
        .on_hover(on_hover),
    ))
}
