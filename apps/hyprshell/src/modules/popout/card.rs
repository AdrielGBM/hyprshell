//! The shape every popout takes.
//!
//! A popout is a glance, not a panel: a heading, at most one meter, and a handful of label/value rows. Saying
//! that once — as data a module fills in rather than a tree each module builds — is what keeps twelve of them
//! looking like one shell instead of twelve small designs.

use rsx::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ProgressProps, ReadSignal, RectStyle, SizeDimension, StyledContainer, Text, TextStyle, box_item,
    progress, signal,
};

use crate::shared::icon::icon_view;
use crate::shared::theme::{FontRole, NordTheme};

const METER_HEIGHT: f32 = 6.0;
const HEADER_ICON: f32 = 26.0;

/// A value that may or may not change while the popout is up. Most rows are live — the point of hovering the
/// volume chip is watching the number move as you scroll it — but a few (a device name, a mount point) are
/// settled by the time the card is built.
pub fn fixed(text: impl Into<String>) -> ReadSignal<String> {
    signal(text.into()).read_only()
}

/// One popout's content, described rather than built.
pub struct Card {
    icon: Option<ReadSignal<String>>,
    icon_tint: Option<ReadSignal<Color>>,
    title: ReadSignal<String>,
    subtitle: Option<ReadSignal<String>>,
    meter: Option<(ReadSignal<f32>, ReadSignal<Color>)>,
    rows: Vec<(ReadSignal<String>, ReadSignal<String>)>,
}

impl Card {
    pub fn new(title: ReadSignal<String>) -> Self {
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
        Self::new(fixed(title))
    }

    pub fn icon(mut self, glyph: ReadSignal<String>) -> Self {
        self.icon = Some(glyph);
        self
    }

    pub fn icon_tint(mut self, tint: ReadSignal<Color>) -> Self {
        self.icon_tint = Some(tint);
        self
    }

    pub fn subtitle(mut self, text: ReadSignal<String>) -> Self {
        self.subtitle = Some(text);
        self
    }

    /// A 0..1 bar under the heading. Only one per card: a popout with two meters is a dashboard card (F8), and
    /// this is not the surface for one.
    pub fn meter(mut self, fraction: ReadSignal<f32>, tint: ReadSignal<Color>) -> Self {
        self.meter = Some((fraction, tint));
        self
    }

    pub fn row(mut self, label: ReadSignal<String>, value: ReadSignal<String>) -> Self {
        self.rows.push((label, value));
        self
    }

    pub fn build(self, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
        let mut children: Vec<Box<dyn LayoutItem>> = vec![header(&self, theme)?];
        if let Some((fraction, tint)) = self.meter {
            children.push(meter(fraction, tint, theme)?);
        }
        for (label, value) in self.rows {
            children.push(row(label, value, theme)?);
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

/// The bar under the heading. `progress` scales its fill with a transform rather than re-laying out a narrower
/// box, which is what makes it cheap enough to follow a value that moves on every wheel notch.
fn meter(
    fraction: ReadSignal<f32>,
    tint: ReadSignal<Color>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let value = signal(fraction.get().clamp(0.0, 1.0));
    let bound = value.clone();
    rsx::effect(move || bound.set(fraction.get().clamp(0.0, 1.0)));
    let bar = progress(ProgressProps {
        value: Some(value),
        color: Box::new(move || tint.get()),
        track_color: Box::new(move || theme.overlay),
        width: 0.0,
        height: METER_HEIGHT,
    })?;
    // The component sizes its track in px; a full-width holder is what makes the bar follow the card instead.
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0)),
        vec![bar],
    )?))
}

fn row(
    label: ReadSignal<String>,
    value: ReadSignal<String>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let caption = theme.font(FontRole::Caption);
    let label = Text::auto(
        move || label.get(),
        LayoutStyle::new().flex_shrink(0.0),
        move || TextStyle::new(caption, theme.muted),
    )?;
    let value = Text::auto(
        move || value.get(),
        LayoutStyle::new(),
        move || TextStyle::new(caption, theme.text),
    )?;
    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .width(SizeDimension::Percent(1.0))
            .gap(10.0),
        vec![box_item(label), box_item(value)],
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
