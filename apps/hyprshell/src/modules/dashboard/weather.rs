//! The Weather page: what it is doing now, and what it will do.
//!
//! Everything here already ships — the service, its disk cache, the condition glyphs and the translated
//! descriptions — so the page adds no I/O of its own. The unit toggle is deliberately local to the surface
//! rather than a config write: pressing a reading to check it in the other scale is a glance, not a preference,
//! and `[temperature] unit` stays what the bar and the OSD follow.

use chrono::NaiveDate;
use rsx::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ReactiveList, RectStyle, RwSignal, SizeDimension, StyledContainer, Text, TextStyle, box_item,
    signal,
};

use super::card::{self, Card};
use crate::core::config::{Config, TemperatureUnit};
use crate::shared::glyph;
use crate::shared::icon::icon_view;
use crate::shared::reactive::{Live, derive, derive_pair, fixed_text};
use crate::shared::services::weather::{self, Day, Weather};
use crate::shared::theme::{FontRole, NordTheme};
use crate::shared::widget;

const CONDITION_ICON: f32 = 52.0;
const FORECAST_ICON: f32 = 20.0;

pub fn page(config: &Config, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if !config.weather.enabled {
        return card::page(vec![card::frame(
            vec![card::detail(
                fixed_text(rsx::t!("dashboard.weather_off")),
                theme,
            )?],
            theme,
        )?]);
    }

    let state = signal(weather::current().unwrap_or_default());
    let sink = state.clone();
    platform_layershell::watch(weather::subscribe, move |w| sink.set(w));
    let unit = signal(config.temperature.unit);

    card::page(vec![
        current_card(state.clone(), unit.clone(), theme)?,
        forecast_card(state, unit, config.weather.forecast_days(), theme)?,
    ])
}

fn current_card(
    state: RwSignal<Weather>,
    unit: RwSignal<TemperatureUnit>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon_state = derive(state.clone(), |w| {
        glyph::weather(w.condition(), w.is_day).to_string()
    });
    let icon = icon_view(
        move || icon_state.get(),
        move || theme.accent,
        CONDITION_ICON,
    )?;

    let reading = derive_pair(state.read_only(), unit.read_only(), |w, unit| {
        unit.format(w.temperature)
    });
    let condition = derive(state.clone(), |w| w.condition().label());
    let place = derive(state.clone(), |w| {
        let place = w.place.trim();
        if place.is_empty() {
            rsx::t!("sysinfo.no_reading")
        } else {
            place.to_string()
        }
    });

    let headline = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(14.0)
            .width(SizeDimension::Percent(1.0)),
        vec![
            icon,
            Box::new(Container::new(
                LayoutStyle::new().flex_column().flex_grow(1.0).gap(2.0),
                vec![
                    unit_toggle(reading, unit.clone(), theme)?,
                    box_item(Text::auto(
                        move || condition.get(),
                        LayoutStyle::new(),
                        move || TextStyle::new(theme.font(FontRole::Body), theme.subtle),
                    )?),
                ],
            )?),
        ],
    )?;

    let caption = theme.font(FontRole::Caption);
    let rows: Vec<Box<dyn LayoutItem>> = vec![
        widget::label_value(
            fixed_text(rsx::t!("dashboard.feels_like")),
            derive_pair(state.read_only(), unit.read_only(), |w, unit| {
                unit.format(w.feels_like)
            }),
            caption,
            theme.muted,
            theme.text,
        )?,
        widget::label_value(
            fixed_text(rsx::t!("dashboard.humidity")),
            derive(state.clone(), |w| format!("{}%", w.humidity)),
            caption,
            theme.muted,
            theme.text,
        )?,
        widget::label_value(
            fixed_text(rsx::t!("dashboard.wind")),
            derive(state.clone(), |w| format!("{:.0} km/h", w.wind)),
            caption,
            theme.muted,
            theme.text,
        )?,
    ];

    let mut card = Card::new(place).icon("map-pin").child(Box::new(headline));
    for row in rows {
        card = card.child(row);
    }
    card.build(theme)
}

/// The reading, pressable. A press swaps the scale for this surface only — see the module note.
fn unit_toggle(
    reading: Live<String>,
    unit: RwSignal<TemperatureUnit>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let text = Text::auto(
        move || reading.get(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Display), theme.text).with_weight(700),
    )?;
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new().padding_horizontal(4.0),
            move |_r| RectStyle::filled(Color::TRANSPARENT, 6.0),
            vec![box_item(text)],
        )?
        .on_hover_style(move |_r| RectStyle::filled(theme.overlay, 6.0))
        .on_press(move || unit.set(other_unit(unit.peek()))),
    ))
}

fn other_unit(unit: TemperatureUnit) -> TemperatureUnit {
    match unit {
        TemperatureUnit::Celsius => TemperatureUnit::Fahrenheit,
        TemperatureUnit::Fahrenheit => TemperatureUnit::Celsius,
    }
}

fn forecast_card(
    state: RwSignal<Weather>,
    unit: RwSignal<TemperatureUnit>,
    limit: u32,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = state.read_only();
    let unit_source = unit.read_only();
    let days = ReactiveList::with_gap(
        move || {
            let unit = unit_source.get();
            source
                .get()
                .days
                .into_iter()
                .take(limit as usize)
                .map(|day| (day, unit))
                .collect()
        },
        |(day, unit): &(Day, TemperatureUnit)| format!("{}|{}", day.date, unit.suffix()),
        move |(day, unit): (Day, TemperatureUnit)| forecast_row(day, unit, theme),
        6.0,
    )?;

    let empty = derive(state, |w| {
        if w.days.is_empty() {
            rsx::t!("dashboard.no_forecast")
        } else {
            String::new()
        }
    });

    Card::titled(rsx::t!("dashboard.forecast"))
        .icon("calendar-days")
        .child(Box::new(days))
        .child(card::detail(empty, theme)?)
        .build(theme)
}

/// One day: when, what, how likely to rain, and the range. Kept to a single row so a week reads as a column of
/// comparable lines rather than seven small cards.
fn forecast_row(
    day: Day,
    unit: TemperatureUnit,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = weekday_label(&day.date);
    let icon = icon_view(
        {
            let condition = day.condition();
            move || glyph::weather(condition, true).to_string()
        },
        move || theme.subtle,
        FORECAST_ICON,
    )?;
    let rain = if day.precipitation > 0 {
        format!("{}%", day.precipitation)
    } else {
        String::new()
    };
    let range = format!("{} / {}", unit.format(day.high), unit.format(day.low));
    let caption = theme.font(FontRole::Caption);

    let cells: Vec<Box<dyn LayoutItem>> = vec![
        box_item(Text::auto(
            move || label.clone(),
            LayoutStyle::new().width(40.0).flex_shrink(0.0),
            move || TextStyle::new(caption, theme.text).with_weight(700),
        )?),
        icon,
        box_item(Text::auto(
            move || rain.clone(),
            LayoutStyle::new().flex_grow(1.0),
            move || TextStyle::new(caption, theme.info),
        )?),
        box_item(Text::auto(
            move || range.clone(),
            LayoutStyle::new().flex_shrink(0.0),
            move || TextStyle::new(caption, theme.subtle),
        )?),
    ];

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::SPACE_BETWEEN)
            .gap(10.0)
            .width(SizeDimension::Percent(1.0)),
        cells,
    )?))
}

/// The API's `YYYY-MM-DD` as a weekday name. An unparseable date falls back to the raw string rather than to a
/// weekday it made up.
fn weekday_label(date: &str) -> String {
    match NaiveDate::parse_from_str(date, "%Y-%m-%d") {
        Ok(date) => super::dash::weekday_label(chrono::Datelike::weekday(&date)),
        Err(_) => date.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_forecast_date_reads_as_its_weekday_and_a_broken_one_reads_as_itself() {
        rsx::set_locale("en");
        // 2026-07-27 is a Monday.
        assert_eq!(
            weekday_label("2026-07-27"),
            rsx::t!("dashboard.weekday.mon")
        );
        assert_eq!(
            weekday_label("not-a-date"),
            "not-a-date",
            "a shape the API changed under us shows through instead of becoming Monday"
        );
    }

    #[test]
    fn the_unit_toggle_returns_to_where_it_started() {
        assert_eq!(
            other_unit(TemperatureUnit::Celsius),
            TemperatureUnit::Fahrenheit
        );
        assert_eq!(
            other_unit(other_unit(TemperatureUnit::Celsius)),
            TemperatureUnit::Celsius
        );
    }
}
