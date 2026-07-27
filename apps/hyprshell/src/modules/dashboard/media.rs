//! The Media page: the track, the art, the playhead and the transport.
//!
//! The playhead is the one thing here the MPRIS service cannot broadcast: `Position` advances continuously and
//! emits no change signal, so following it means asking, and asking on everyone's behalf would wake the whole
//! shell several times a second for a number no bar chip shows. This page therefore owns the only ticker, at
//! the rate `[dashboard] media_update_interval` sets, and it dies with the surface.

use std::time::Duration;

use rsx::{
    AlignItems, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    ReactiveList, RectStyle, RwSignal, SizeDimension, StyledContainer, Text, TextStyle, box_item,
    signal, track_layout,
};

use super::card::{self, Card, METER_HEIGHT};
use crate::core::config::Config;
use crate::shared::art::{self, ArtState};
use crate::shared::icon::icon_view;
use crate::shared::reactive::{Live, derive, fixed, fixed_text};
use crate::shared::services::mpris::{self, LoopStatus, Playback, Player};
use crate::shared::theme::{FontRole, NordTheme};
use crate::shared::{picture, widget};

const COVER: f32 = 96.0;
const TRANSPORT_ICON: f32 = 22.0;
const PRIMARY_ICON: f32 = 30.0;

pub fn page(config: &Config, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let player = signal(mpris::current().unwrap_or_default());
    let sink = player.clone();
    platform_layershell::watch(mpris::subscribe, move |p| sink.set(p));

    let position = signal(mpris::position().unwrap_or(0));
    let ticker = position.clone();
    let interval = config.dashboard.media_interval();
    platform_layershell::watch(
        move |tx| poll_position(tx, interval),
        move |micros| ticker.set(micros),
    );

    card::page(vec![now_playing(player, position, theme)?])
}

/// Reads the playhead on one long-lived thread. `mpris::position` opens its own session connection per call,
/// which is the right shape for a one-off read from a click handler and the wrong one several times a second —
/// so the loop pays for it once and the poll is a property get.
fn poll_position(tx: platform_layershell::EventSender<i64>, interval: Duration) {
    loop {
        if !tx.send(mpris::position().unwrap_or(0)) {
            return;
        }
        std::thread::sleep(interval);
    }
}

fn now_playing(
    player: RwSignal<Player>,
    position: RwSignal<i64>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let title = derive(player.clone(), |p| {
        let title = p.title.trim();
        if title.is_empty() {
            rsx::t!("popout.nothing_playing")
        } else {
            title.to_string()
        }
    });
    let artist = derive(player.clone(), |p| non_empty(&p.artist));
    let album = derive(player.clone(), |p| non_empty(&p.album));
    let identity = derive(player.clone(), |p| non_empty(&p.identity));

    let heading = Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .gap(14.0)
            .width(SizeDimension::Percent(1.0)),
        vec![
            cover(player.clone(), theme)?,
            Box::new(Container::new(
                LayoutStyle::new().flex_column().flex_grow(1.0).gap(4.0),
                vec![
                    text(title, theme.font(FontRole::Title), theme.text, true)?,
                    text(artist, theme.font(FontRole::Body), theme.subtle, false)?,
                    text(album, theme.font(FontRole::Caption), theme.muted, false)?,
                ],
            )?),
        ],
    )?;

    let card = Card::new(fixed_text(rsx::t!("dashboard.now_playing")))
        .icon("disc-3")
        .trailing(identity)
        .child(Box::new(heading))
        .child(scrubber(player.clone(), position, theme)?)
        .child(transport(player, theme)?);
    card.build(theme)
}

/// The cover, or a placeholder while there is nothing to show. A keyed list over the resolved file rather than
/// a plain image, so the art is decoded once per picture instead of once per repaint, and so the placeholder is
/// swapped for the real thing when the download lands.
fn cover(player: RwSignal<Player>, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = derive(player, |p| p.art_url.clone());
    let rows = ReactiveList::with_gap(
        move || vec![art_file(&source.get())],
        |path: &Option<String>| path.clone().unwrap_or_default(),
        move |path: Option<String>| match path.and_then(|p| picture::square(p.as_ref(), COVER)) {
            Some(image) => Ok(image),
            None => placeholder(theme),
        },
        0.0,
    )?;
    Ok(Box::new(rows))
}

/// The local file for an `artUrl`, starting a download the first time one is seen. Returns `None` while it is
/// still coming, which the card draws as the placeholder rather than as a gap that pops.
fn art_file(url: &str) -> Option<String> {
    match art::art(url).get() {
        ArtState::Ready(path) => Some(path.to_string_lossy().into_owned()),
        ArtState::Loading | ArtState::Missing => None,
    }
}

fn placeholder(theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = icon_view(|| "music".to_string(), move || theme.muted, COVER * 0.45)?;
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .width(COVER)
            .height(COVER)
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_r| RectStyle::filled(theme.overlay, 8.0),
        vec![icon],
    )?))
}

/// A full-width playhead that seeks where it is pressed.
///
/// The jump is expressed as a *relative* `Seek`, because the absolute `SetPosition` takes the track id from the
/// metadata and refuses the call when it does not match — exactly the race a scrub hits when the track changes
/// under it. Which is also why the current position has to be subtracted here rather than sent as-is.
fn scrubber(
    player: RwSignal<Player>,
    position: RwSignal<i64>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let length = derive(player.clone(), |p| p.length);
    let seekable = derive(player, |p| p.can_seek);

    let for_fraction = length.clone();
    let fraction = derive(position.read_only(), move |micros| {
        let total = for_fraction.get();
        if total <= 0 {
            0.0
        } else {
            (micros as f32 / total as f32).clamp(0.0, 1.0)
        }
    });
    let tint = derive(seekable.clone(), move |can| {
        if can { theme.accent } else { theme.muted }
    });
    let bar = widget::meter(fraction, tint, theme.overlay, METER_HEIGHT)?;

    let track = StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .padding_vertical(6.0),
        move |_r| RectStyle::filled(Color::TRANSPARENT, 0.0),
        vec![bar],
    )?;
    let rect = track_layout(track.layout_node())
        .expect("a container registers its rect")
        .read_only();
    let (seek_length, seek_position) = (length.clone(), position.read_only());
    let track = track.on_drag(move |px, _py| {
        if !seekable.get() {
            return;
        }
        let width = rect.get().width;
        let total = seek_length.get();
        if width <= 0.0 || total <= 0 {
            return;
        }
        let target = ((px / width).clamp(0.0, 1.0) as f64 * total as f64) as i64;
        mpris::seek(target - seek_position.get());
    });

    let elapsed = derive(position.read_only(), clock_label);
    let remaining = derive(length, clock_label);

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(2.0)
            .width(SizeDimension::Percent(1.0)),
        vec![
            Box::new(track),
            widget::label_value(
                elapsed,
                remaining,
                theme.font(FontRole::Caption),
                theme.muted,
                theme.muted,
            )?,
        ],
    )?))
}

fn transport(
    player: RwSignal<Player>,
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let shuffle_tint = derive(player.clone(), move |p| {
        if p.shuffle { theme.accent } else { theme.muted }
    });
    let loop_tint = derive(player.clone(), move |p| {
        if p.loop_status == LoopStatus::Off {
            theme.muted
        } else {
            theme.accent
        }
    });
    let loop_glyph = derive(player.clone(), |p| {
        if p.loop_status == LoopStatus::Track {
            "repeat-1".to_string()
        } else {
            "repeat".to_string()
        }
    });
    let previous_tint = derive(player.clone(), move |p| enabled(p.can_go_previous, theme));
    let next_tint = derive(player.clone(), move |p| enabled(p.can_go_next, theme));
    let play_glyph = derive(player, |p| {
        if p.playback == Playback::Playing {
            "pause".to_string()
        } else {
            "play".to_string()
        }
    });

    let buttons: Vec<Box<dyn LayoutItem>> = vec![
        button(
            fixed_text("shuffle"),
            shuffle_tint,
            TRANSPORT_ICON,
            mpris::toggle_shuffle,
            theme,
        )?,
        button(
            fixed_text("skip-back"),
            previous_tint,
            TRANSPORT_ICON,
            mpris::previous,
            theme,
        )?,
        button(
            play_glyph,
            fixed(theme.text),
            PRIMARY_ICON,
            mpris::play_pause,
            theme,
        )?,
        button(
            fixed_text("skip-forward"),
            next_tint,
            TRANSPORT_ICON,
            mpris::next,
            theme,
        )?,
        button(
            loop_glyph,
            loop_tint,
            TRANSPORT_ICON,
            mpris::cycle_loop,
            theme,
        )?,
    ];

    Ok(Box::new(Container::new(
        LayoutStyle::new()
            .flex_row()
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER)
            .gap(10.0)
            .width(SizeDimension::Percent(1.0)),
        buttons,
    )?))
}

/// A control the player says it cannot honour recedes to muted rather than disappearing: a transport row that
/// changes shape between tracks is harder to aim at than one whose buttons stay put.
fn enabled(can: bool, theme: NordTheme) -> Color {
    if can { theme.text } else { theme.muted }
}

fn button(
    glyph: Live<String>,
    tint: Live<Color>,
    size: f32,
    action: fn(),
    theme: NordTheme,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let icon = icon_view(move || glyph.get(), move || tint.get(), size)?;
    Ok(Box::new(
        StyledContainer::new(
            LayoutStyle::new()
                .padding_all(6.0)
                .align_items(AlignItems::CENTER)
                .justify_content(JustifyContent::CENTER),
            move |_r| RectStyle::filled(Color::TRANSPARENT, 8.0),
            vec![icon],
        )?
        .on_hover_style(move |_r| RectStyle::filled(theme.overlay, 8.0))
        .on_press(action),
    ))
}

fn text(
    value: Live<String>,
    size: f32,
    color: Color,
    bold: bool,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    Ok(box_item(Text::auto(
        move || value.get(),
        LayoutStyle::new(),
        move || {
            let style = TextStyle::new(size, color);
            if bold { style.with_weight(700) } else { style }
        },
    )?))
}

fn non_empty(text: &str) -> String {
    let text = text.trim();
    if text.is_empty() {
        rsx::t!("sysinfo.no_reading")
    } else {
        text.to_string()
    }
}

/// Microseconds as `m:ss`, or `h:mm:ss` past an hour. A track with no reported length (a live stream) reads as
/// `--:--` rather than as zero, which would claim it had just started.
fn clock_label(micros: i64) -> String {
    if micros <= 0 {
        return "--:--".to_string();
    }
    let total = micros / 1_000_000;
    let (hours, minutes, seconds) = (total / 3600, (total % 3600) / 60, total % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_playhead_reads_as_a_clock_and_an_unknown_length_says_so() {
        assert_eq!(clock_label(0), "--:--", "a live stream reports no length");
        assert_eq!(clock_label(-1), "--:--");
        assert_eq!(clock_label(9_000_000), "0:09");
        assert_eq!(clock_label(125_000_000), "2:05");
        assert_eq!(
            clock_label(3_725_000_000),
            "1:02:05",
            "past an hour the field grows rather than wrapping"
        );
    }
}
