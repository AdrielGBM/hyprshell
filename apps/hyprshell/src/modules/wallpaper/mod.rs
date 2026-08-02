//! The background surface: the wallpaper, its transition, and anything drawn on top of it.
//!
//! One surface per monitor, at the bottom of the background layer. It paints the image the wallpaper service
//! says this screen should show, cover-cropped over the theme's base colour, and — when `[background.clock]`
//! asks for it — a clock face on top.
//!
//! **The transition is why a wallpaper change is an event and not a rebuild.** A picture chosen at runtime is
//! not a config edit — it is session state — and rebuilding the surface for it would be useless for a
//! cross-fade anyway: a fresh tree has nothing left of the old image to fade *from*. So a runtime wallpaper
//! change arrives as an event on the live surface, carrying an image the service already decoded off the UI
//! thread, and the two layers are simply ping-ponged.

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::SystemTime;

use telar::{
    AlignItems, App, Color, Component, Container, Image, ImageData, ImageFilter, JustifyContent,
    LayoutError, LayoutItem, LayoutStyle, ObjectFit, RectStyle, Shadow, SizeDimension,
    StyledContainer, Text, WindowConfig, box_item, motion::Animated, reset_layout_runtime,
    set_theme, signal,
};

use crate::core::app::SurfaceRoot;
use crate::core::config::{Align, Config, WallpaperTransition};
use crate::core::surfaces::LiveConfig;
use crate::shared::reactive::{derive, fixed};
use crate::shared::services::{clock, visualiser, wallpaper};
use crate::shared::theme::FontRole;

/// How far a wipe travels when the compositor has not said how wide this screen is. Only reached before the
/// output list has been read, and a wipe that starts slightly off-screen is invisible either way.
const FALLBACK_TRAVEL: f32 = 1920.0;

/// Reading where the hand-over between the two image layers has got to, and moving it. `Rc` on the reading half
/// because both layers hold one; `Box` on the writing half because only the frame consumer does.
type FadeControl = (Rc<dyn Fn() -> f32>, Box<dyn Fn(f32)>);

/// Per-output wallpaper: a full-screen background surface painting the current image (cover-cropped, aspect preserved) over the theme's base colour, or just the base colour when no image resolves.
pub struct WallpaperApp {
    /// Read at every build rather than held: the surface outlives the config it was first drawn from, and a
    /// reload rebuilds it in place from whatever is in here now.
    pub config: LiveConfig,
    /// The monitor this wallpaper covers, so a `[background.monitors]` entry can target it.
    pub output: Option<String>,
}

impl App for WallpaperApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let config = self.config.get();
        set_theme(config.resolve_theme());
        crate::shared::services::locale::attach(config.language());
        Box::new(SurfaceRoot::new(self.content(&config)).expect("wallpaper layout failed"))
    }

    fn clear_color(&self) -> Option<Color> {
        // The base colour shows before/without an image (and behind any transparency), covering "theme base colour when there is no image".
        Some(self.config.get().resolve_theme().base)
    }

    fn window_config(&self) -> Option<WindowConfig> {
        // Opaque: a wallpaper replaces whatever the compositor draws behind it.
        None
    }
}

impl WallpaperApp {
    fn content(&self, config: &Config) -> Box<dyn LayoutItem> {
        let mut layers: Vec<Box<dyn LayoutItem>> = Vec::new();
        match self.image_layers(config) {
            Ok(Some(images)) => layers.push(images),
            Ok(None) => {}
            Err(e) => tracing::warn!("wallpaper images: {e}"),
        }
        if config.background.clock.enabled {
            match clock_face(config) {
                Ok(face) => layers.push(face),
                Err(e) => tracing::warn!("desktop clock: {e}"),
            }
        }
        if config.background.visualiser.enabled {
            match visualiser_row(config) {
                Ok(row) => layers.push(row),
                Err(e) => tracing::warn!("background visualiser: {e}"),
            }
        }
        Container::new(fill(), layers)
            .map(|container| Box::new(container) as Box<dyn LayoutItem>)
            .expect("wallpaper root container")
    }

    /// The two stacked image slots and the animation that hands the screen from one to the other.
    ///
    /// A ping-pong rather than an "outgoing / incoming" pair: `fade` runs `0` → slot A visible, `1` → slot B
    /// visible, and each new image lands in whichever slot is currently hidden. One animation, no bookkeeping
    /// about which layer is on the way out, and an interrupted transition — a second change arriving mid-fade —
    /// simply retargets from wherever it had got to instead of snapping.
    fn image_layers(&self, config: &Config) -> Result<Option<Box<dyn LayoutItem>>, LayoutError> {
        let initial = wallpaper::current_image(config, self.output.as_deref());
        let first = initial
            .as_deref()
            .and_then(|path| decoded(self.output.as_deref(), path));
        if first.is_none() && initial.is_some() {
            tracing::warn!(
                "wallpaper '{}' could not be loaded; using the theme base colour",
                initial
                    .as_ref()
                    .map(|p| p.display().to_string())
                    .unwrap_or_default()
            );
        }

        let slot_a = signal(first);
        let slot_b = signal(None::<Arc<ImageData>>);
        let transition = config.background.transition;
        let (read_fade, set_fade) = Self::fade_control(config);
        let travel = self.output_width();

        let layer_a = image_layer(
            slot_a.read_only(),
            read_fade.clone(),
            0.0,
            transition,
            travel,
        )?;
        let layer_b = image_layer(slot_b.read_only(), read_fade, 1.0, transition, travel)?;

        // Which slot holds the newest image. A plain `Cell`: it only ever changes on the driver thread, from
        // the consumer below, so a signal would buy reactivity that nothing reads.
        let showing_b = Rc::new(Cell::new(false));
        platform_layershell::watch(
            wallpaper::frames(self.output.clone(), initial),
            move |frame: Option<wallpaper::Frame>| {
                // `None` is the producer's liveness heartbeat, not a wallpaper.
                let Some(frame) = frame else { return };
                let next_is_b = !showing_b.get();
                if next_is_b {
                    slot_b.set(Some(frame.image));
                } else {
                    slot_a.set(Some(frame.image));
                }
                showing_b.set(next_is_b);
                set_fade(if next_is_b { 1.0 } else { 0.0 });
            },
        );

        let stack = Container::new(fill(), vec![layer_a, layer_b])?;
        Ok(Some(Box::new(stack)))
    }

    /// How the layers are handed over: reading the current position, and moving it.
    ///
    /// Two shapes behind one pair of closures. An animated transition drives an `Animated`, built at `0.0` and
    /// retargeted — never at its destination, which would leave it inert. `transition = "none"` (and animation
    /// switched off globally) drives a plain signal instead of an `Animated` with a zero-length tween, because a
    /// tween that has no duration to divide by is a division waiting to happen, and "no transition" should not
    /// go anywhere near the ticker.
    fn fade_control(config: &Config) -> FadeControl {
        let instant = config.background.transition == WallpaperTransition::None
            || !config.animation.enabled
            || config.background.transition_ms == 0;
        if instant {
            let at = signal(0.0f32);
            let reading = at.read_only();
            return (
                Rc::new(move || reading.get()),
                Box::new(move |to| at.set(to)),
            );
        }
        let tween = config
            .animation
            .tween_ms(config.background.transition_ms, 10_000);
        let fade = Animated::new(0.0f32, tween);
        let reading = fade.clone();
        (
            Rc::new(move || reading.get()),
            Box::new(move |to| fade.retarget(to)),
        )
    }

    /// This screen's logical width, for how far a wipe has to travel.
    fn output_width(&self) -> f32 {
        platform_layershell::outputs()
            .into_iter()
            .find(|out| out.name == self.output || self.output.is_none())
            .and_then(|out| out.logical_size)
            .map(|(width, _)| width as f32)
            .filter(|width| *width > 0.0)
            .unwrap_or(FALLBACK_TRAVEL)
    }
}

/// The picture this screen opens on, decoded at most once per file.
///
/// A config reload rebuilds this surface's content, and a settings form applies itself while the user is still
/// typing — so decoding the same file again on the UI thread would put a full image decode between every burst
/// of keystrokes and the frame that answers it. One entry per screen, holding what that screen's live surface
/// is already holding, and keyed by mtime as well as path so a picture overwritten in place still lands.
///
/// Only the *opening* image comes through here: a wallpaper chosen at runtime arrives already decoded, off the
/// UI thread, from the wallpaper service.
fn decoded(output: Option<&str>, path: &Path) -> Option<Arc<ImageData>> {
    /// The picture a screen last opened on: which file, when it was written, and its pixels.
    type Opened = (PathBuf, SystemTime, Arc<ImageData>);
    thread_local! {
        static LAST: RefCell<HashMap<Option<String>, Opened>> = RefCell::new(HashMap::new());
    }
    let stamp = std::fs::metadata(path).and_then(|meta| meta.modified()).ok();
    let key = output.map(str::to_string);
    let hit = LAST.with(|last| {
        last.borrow()
            .get(&key)
            .filter(|(cached, at, _)| cached == path && stamp == Some(*at))
            .map(|(_, _, image)| Arc::clone(image))
    });
    if hit.is_some() {
        return hit;
    }
    let image = Arc::new(crate::shared::picture::decode(path)?);
    if let Some(stamp) = stamp {
        LAST.with(|last| {
            last.borrow_mut()
                .insert(key, (path.to_path_buf(), stamp, Arc::clone(&image)))
        });
    }
    Some(image)
}

/// A full-surface style, used for every layer so they stack rather than sit side by side.
fn fill() -> LayoutStyle {
    LayoutStyle::new()
        .width(SizeDimension::Percent(1.0))
        .height(SizeDimension::Percent(1.0))
}

/// One image slot: absolutely filling the surface, shown in proportion to how close `fade` is to `visible_at`.
///
/// The image itself is rebuilt whenever the slot's signal changes — `Image::new` takes the data as a closure,
/// so the layer is one node for the life of the surface and swapping the picture is a signal write, not a
/// re-layout.
fn image_layer(
    slot: telar::ReadSignal<Option<Arc<ImageData>>>,
    fade: Rc<dyn Fn() -> f32>,
    visible_at: f32,
    transition: WallpaperTransition,
    travel: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let data = slot.clone();
    let image = Image::new(
        fill(),
        move || data.get().unwrap_or_else(blank),
        || ImageFilter::Linear,
        || ObjectFit::Cover,
    )?;

    let present = slot;
    let opacity_fade = Rc::clone(&fade);
    let mut layer = StyledContainer::new(
        fill().absolute_fill(),
        |_| RectStyle::default(),
        vec![box_item(image)],
    )?
    .with_opacity(move || {
        // Both read before the early return: a slot that is empty this frame must still re-run when it fills.
        let at = opacity_fade();
        let filled = present.get().is_some();
        if !filled {
            return 0.0;
        }
        // `visible_at` is 0 or 1, so this is "how close the hand-over has got to me".
        1.0 - (at - visible_at).abs()
    });

    if transition == WallpaperTransition::Wipe {
        // A wipe is the incoming layer sliding over the outgoing one, so only the layer being *left* moves —
        // the arriving one has to end up at rest exactly where the other was.
        layer = layer.with_transform(move |_| {
            let distance = (fade() - visible_at).abs();
            (distance != 0.0).then_some([1.0, 0.0, 0.0, 1.0, distance * travel, 0.0])
        });
    }
    Ok(Box::new(layer))
}

/// A single transparent pixel, stood in for an empty slot.
///
/// Shared rather than built per call: `ImageData::new` mints a new id every time, and a slot that handed the
/// renderer a fresh id on every frame would fill the texture cache with copies of nothing.
fn blank() -> Arc<ImageData> {
    thread_local! {
        static BLANK: Arc<ImageData> = Arc::new(ImageData::new(vec![0, 0, 0, 0], 1, 1));
    }
    BLANK.with(Arc::clone)
}

/// The clock drawn on the wallpaper (`[background.clock]`).
///
/// It lives here rather than in the `clock` module because it is not that module: the bar chip is a chip in a
/// row of chips, and this is a face placed on a screen. What they do share — the tick and the `strftime`
/// patterns — they share through the clock *service* and `[clock]`, which is the part that would actually be
/// wrong to duplicate.
fn clock_face(config: &Config) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = config.resolve_theme();
    let settings = config.background.clock.clone();
    let ink = if settings.invert {
        theme.base
    } else {
        theme.text
    };

    let format = settings.time_format(&config.clock).to_string();
    let date_format = settings.date_format(&config.clock).to_string();
    let now = signal(chrono::Local::now().format(&format).to_string());
    let today = signal(chrono::Local::now().format(&date_format).to_string());
    let (tick_time, tick_date) = (now.clone(), today.clone());
    platform_layershell::watch(clock::subscribe, move |at: clock::Now| {
        tick_time.set(at.format(&format).to_string());
        tick_date.set(at.format(&date_format).to_string());
    });

    let size = theme.font(FontRole::Display) * settings.resolved_scale();
    let shadow = settings
        .shadow
        .then(|| Shadow::new(0.0, 2.0, 12.0, Color::BLACK.with_alpha(0.55)));

    let reading = now.read_only();
    let time = Text::auto(
        move || reading.get(),
        LayoutStyle::new(),
        move || {
            let style = theme
                .text_style(FontRole::Display, ink)
                .with_weight(600)
                .with_size(size);
            match shadow {
                Some(shadow) => style.with_shadow(shadow),
                None => style,
            }
        },
    )?;

    let mut lines: Vec<Box<dyn LayoutItem>> = vec![box_item(time)];
    if settings.show_date {
        let reading = today.read_only();
        let date_size = (size * 0.28).max(theme.font(FontRole::Body));
        let date = Text::auto(
            move || reading.get(),
            LayoutStyle::new(),
            move || {
                let style = theme.text_style(FontRole::Title, ink).with_size(date_size);
                match shadow {
                    Some(shadow) => style.with_shadow(shadow),
                    None => style,
                }
            },
        )?;
        lines.push(box_item(date));
    }

    // Every reading is centred inside its own row, since a `Text` in a column takes the column's width and
    // draws its glyphs from the left — the same rule the lock screen's `centred` exists for.
    let mut rows: Vec<Box<dyn LayoutItem>> = Vec::new();
    for line in lines {
        rows.push(Box::new(Container::new(
            LayoutStyle::new()
                .flex_row()
                .justify_content(JustifyContent::CENTER)
                .width(SizeDimension::Percent(1.0)),
            vec![line],
        )?));
    }

    let column = Container::new(LayoutStyle::new().flex_column().gap(size * 0.08), rows)?;

    let plate_radius = theme.radius.max(12.0);
    let opacity = settings.plate_opacity();
    let feather = settings.background_blur.max(0.0);
    // The raised surface, not the base: a plate painted in the colour behind it is invisible on a screen with
    // no image, which is exactly the state a user switching it on for the first time is looking at.
    let plate_fill = if settings.invert {
        theme.text
    } else {
        theme.surface
    };
    let plate = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .padding_horizontal(size * 0.3)
            .padding_vertical(size * 0.1),
        move |_| {
            if !settings.background {
                return RectStyle::default();
            }
            let mut style = RectStyle::filled(plate_fill.with_alpha(opacity), plate_radius);
            if feather > 0.0 {
                // A feathered plate is drawn as its own shadow: same colour, spread to the plate's size, blurred
                // by `background_blur`. That is what "the plate's edge fades into the wallpaper" means with the
                // primitives the renderer has — there is no backdrop blur to sample the image through.
                style.shadow = Some(
                    Shadow::new(0.0, 0.0, feather, plate_fill.with_alpha(opacity))
                        .with_spread(feather * 0.5),
                );
            }
            style
        },
        vec![box_item(column)],
    )?;

    let (vertical, horizontal) = settings.position.alignment();
    let margin = settings.margin as f32;
    let placed = Container::new(
        fill()
            .absolute_fill()
            .flex_row()
            .padding_all(margin)
            .align_items(align_items(vertical))
            .justify_content(justify(horizontal)),
        vec![box_item(plate)],
    )?;
    Ok(Box::new(placed))
}

/// The audio visualiser drawn on the wallpaper (`[background.visualiser]`).
///
/// It is a layer over the images rather than a widget inside them for the same reason the clock is: the two
/// image slots ping-pong, and anything laid out among them would move with a cross-fade.
///
/// **The row hides itself by opacity, never by leaving the tree.** Rebuilding a surface's children on a value
/// that changes with the music is a re-layout per frame; and the spectrum service stops publishing entirely
/// once the sound does, so the last frame it sends is the all-zero one that starts the fade — the row costs
/// exactly one animation after the music stops and nothing at all thereafter.
fn visualiser_row(config: &Config) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let settings = config.background.visualiser;
    let theme = config.resolve_theme();
    let tint = if settings.accent {
        theme.accent
    } else {
        theme.text
    };

    let start = visualiser::Spectrum::quiet(config.visualiser.band_count());
    let bands = signal(start.bars.clone());
    let silent = signal(start.silent);
    let (next_bands, next_silent) = (bands.clone(), silent.clone());
    platform_layershell::watch(
        visualiser::subscribe,
        move |spectrum: visualiser::Spectrum| {
            next_bands.set(spectrum.bars);
            next_silent.set(spectrum.silent);
        },
    );

    let row = crate::shared::widget::spectrum(
        derive(bands.read_only(), |bars| bars),
        fixed(tint.with_alpha(settings.alpha())),
        settings.edge,
        crate::shared::widget::SpectrumStyle {
            gap: settings.gap_px(),
            radius: settings.radius_px(),
            floor: 0.0,
        },
        thickness(settings.edge, settings.reach_px()),
    )?;

    let fade = visualiser_fade(config, silent.read_only());
    let layer = StyledContainer::new(
        fill()
            .absolute_fill()
            .flex_row()
            .padding_all(settings.margin as f32)
            .align_items(align_items(match settings.edge {
                crate::core::config::Edge::Top => Align::Start,
                crate::core::config::Edge::Bottom => Align::End,
                _ => Align::Center,
            }))
            .justify_content(justify(match settings.edge {
                crate::core::config::Edge::Left => Align::Start,
                crate::core::config::Edge::Right => Align::End,
                _ => Align::Center,
            })),
        |_| RectStyle::default(),
        vec![row],
    )?
    .with_opacity(fade);
    Ok(Box::new(layer))
}

/// The row's own box: as long as the edge it stands on, as deep as its reach.
fn thickness(edge: crate::core::config::Edge, reach: f32) -> LayoutStyle {
    if edge.is_horizontal() {
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(reach)
    } else {
        LayoutStyle::new()
            .width(reach)
            .height(SizeDimension::Percent(1.0))
    }
}

/// How opaque the row is: one when there is sound, zero when `hide_when_silent` and there is not.
///
/// The same two shapes `fade_control` has, and for the same reason — with animation off, an `Animated` would be
/// a tween with no duration to divide by.
fn visualiser_fade(config: &Config, silent: telar::ReadSignal<bool>) -> Box<dyn Fn() -> f32> {
    if !config.background.visualiser.hide_when_silent {
        return Box::new(|| 1.0);
    }
    if !config.animation.enabled {
        return Box::new(move || if silent.get() { 0.0 } else { 1.0 });
    }
    let fade = Animated::new(0.0f32, config.animation.tween_ms(400, 5_000));
    let target = fade.clone();
    Box::new(move || {
        // Read out first: the retarget is what makes the row appear, and it has to be registered as a
        // dependency on the frame that draws nothing too.
        let quiet = silent.get();
        target.retarget(if quiet { 0.0 } else { 1.0 });
        fade.get()
    })
}

fn align_items(align: Align) -> AlignItems {
    match align {
        Align::Start => AlignItems::FLEX_START,
        Align::Center => AlignItems::CENTER,
        Align::End => AlignItems::FLEX_END,
    }
}

fn justify(align: Align) -> JustifyContent {
    match align {
        Align::Start => JustifyContent::FLEX_START,
        Align::Center => JustifyContent::CENTER,
        Align::End => JustifyContent::FLEX_END,
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;
    use crate::core::config::{ClockPlacement, Config, DesktopClockConfig};

    fn built(config: Config) -> Box<dyn Component> {
        WallpaperApp {
            config: Arc::new(config).into(),
            output: None,
        }
        .root()
    }

    #[test]
    fn the_surface_builds_with_and_without_an_image_and_with_the_clock_on() {
        // The build is where a layout error would surface, and nothing else runs these closures.
        let _ = built(Config::starter());

        let mut with_clock = Config::starter();
        with_clock.background.clock.enabled = true;
        let _ = built(with_clock);

        for transition in crate::core::config::WallpaperTransition::ALL {
            let mut config = Config::starter();
            config.background.transition = transition;
            config.background.clock.enabled = true;
            let _ = built(config);
        }
    }

    #[test]
    fn the_clock_builds_in_every_position_and_with_every_decoration() {
        for position in ClockPlacement::ALL {
            let mut config = Config::starter();
            config.background.clock = DesktopClockConfig {
                enabled: true,
                position,
                background: true,
                background_blur: 8.0,
                invert: true,
                shadow: true,
                ..DesktopClockConfig::default()
            };
            let _ = built(config);
        }
    }

    #[test]
    fn the_visualiser_builds_on_every_edge_and_with_the_fade_both_ways() {
        // The build is what runs the closures: the opacity closure retargets an animation, and the row's own
        // box swaps its axis per edge, neither of which any other test reaches.
        for edge in crate::core::config::Edge::ALL {
            for hide in [true, false] {
                for animated in [true, false] {
                    let mut config = Config::starter();
                    config.background.visualiser =
                        crate::core::config::BackgroundVisualiserConfig {
                            enabled: true,
                            edge,
                            hide_when_silent: hide,
                            ..crate::core::config::BackgroundVisualiserConfig::default()
                        };
                    config.animation.enabled = animated;
                    let _ = built(config);
                }
            }
        }
    }

    #[test]
    fn switching_the_visualiser_on_is_enough_to_open_the_surface() {
        // Every other way of putting something on the wallpaper implies the surface; a visualiser that needed
        // `enabled = true` beside it would read as a setting that does nothing.
        let mut config = Config::starter();
        assert!(!config.background.is_enabled());
        config.background.visualiser.enabled = true;
        assert!(config.background.is_enabled());
    }

    #[test]
    fn the_desktop_face_drops_the_seconds_the_bar_chip_keeps() {
        let clock = crate::core::config::ClockConfig::default();
        let desktop = DesktopClockConfig::default();
        assert_eq!(clock.time_format(), "%H:%M:%S");
        assert_eq!(
            desktop.time_format(&clock),
            "%H:%M",
            "a wallpaper that repainted every second would be a wallpaper animating"
        );

        // A user who set `[clock] format` has said what a clock looks like; the face follows rather than
        // second-guessing them.
        let explicit = crate::core::config::ClockConfig {
            format: Some("%H.%M".to_string()),
            ..crate::core::config::ClockConfig::default()
        };
        assert_eq!(desktop.time_format(&explicit), "%H.%M");

        // And its own override wins over both.
        let own = DesktopClockConfig {
            format: Some("%I%p".to_string()),
            ..DesktopClockConfig::default()
        };
        assert_eq!(own.time_format(&explicit), "%I%p");
    }

    #[test]
    fn the_plate_opacity_can_never_resolve_to_invisible_or_opaque_by_accident() {
        let bounded = |value: f32| {
            DesktopClockConfig {
                background_opacity: value,
                ..DesktopClockConfig::default()
            }
            .plate_opacity()
        };
        assert_eq!(bounded(0.5), 0.5);
        assert_eq!(
            bounded(0.0),
            0.05,
            "a plate asked for is a plate you can see"
        );
        assert_eq!(bounded(4.0), 1.0);
        assert_eq!(bounded(f32::NAN), 0.35);
    }

    #[test]
    fn the_nine_positions_map_onto_distinct_corners() {
        let corners: Vec<(Align, Align)> = ClockPlacement::ALL
            .into_iter()
            .map(|placement| placement.alignment())
            .collect();
        for (index, corner) in corners.iter().enumerate() {
            for other in &corners[index + 1..] {
                assert_ne!(corner, other, "two placements land in the same spot");
            }
        }
        assert_eq!(
            ClockPlacement::TopLeft.alignment(),
            (Align::Start, Align::Start),
            "the row is the vertical axis and the column the horizontal one"
        );
        assert_eq!(
            ClockPlacement::from_id("bottom-right"),
            Some(ClockPlacement::BottomRight)
        );
        assert_eq!(ClockPlacement::from_id("nowhere"), None);
    }

    /// Renders the wallpaper surface end-to-end (real decode + cover-crop). Point it at an image to eyeball the crop:
    /// `TELAR_VISUAL_WALLPAPER_OUT=/tmp/w.png TELAR_VISUAL_WALLPAPER_IMG=/path/to/wall.png cargo test -p hyprshell --lib visual_wallpaper -- --nocapture`.
    /// Set `TELAR_VISUAL_WALLPAPER_CLOCK=1` to draw the desktop clock over it.
    #[test]
    fn visual_wallpaper_png() {
        let Ok(out) = std::env::var("TELAR_VISUAL_WALLPAPER_OUT") else {
            eprintln!("set TELAR_VISUAL_WALLPAPER_OUT to render the wallpaper; skipping");
            return;
        };
        let mut config = Config::starter();
        config.background.enabled = true;
        config.background.image = std::env::var("TELAR_VISUAL_WALLPAPER_IMG")
            .ok()
            .map(PathBuf::from);
        if std::env::var("TELAR_VISUAL_WALLPAPER_CLOCK").is_ok() {
            config.background.clock.enabled = true;
            config.background.clock.background = true;
        }
        crate::test_support::render_png(
            WallpaperApp {
                config: Arc::new(config).into(),
                output: None,
            },
            640,
            400,
            &out,
        );
    }
}
