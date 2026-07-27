//! The small pieces more than one surface draws.
//!
//! A meter and a label/value row are six lines each, which is exactly why they drift: the popout grew one, the
//! dashboard wanted the same thing a size larger, and two copies of "how a fraction reads as a bar" is one copy
//! too many. Each takes its metrics as arguments so a glance card and a full page can share the drawing without
//! sharing a size.

use std::sync::Arc;

use rsx::{
    AlignItems, Canvas, Color, Container, JustifyContent, LayoutError, LayoutItem, LayoutStyle,
    LineCap, LineJoin, PathData, PathStyle, Point, Rect, RectStyle, RenderNode, ShapeStyle,
    SizeDimension, Stroke, StyledContainer, Text, TextStyle, Transform, box_item,
};

use crate::shared::reactive::Live;

/// How much of the tint an area fill keeps under its line. Enough to read as a filled series, light enough that
/// two sparklines stacked in a column don't fight the text between them.
const AREA_ALPHA: f32 = 0.22;
const LINE_WIDTH: f32 = 1.5;

/// A 0..1 bar, full width, that follows its fraction.
///
/// The fill is an absolutely-positioned child scaled horizontally from the left edge rather than a box laid out
/// narrower — the technique `progress` uses, and what makes the bar cheap enough to track a value that moves on
/// every wheel notch. It is spelled out here rather than delegated because `progress` sizes its track in px and
/// binds through an `RwSignal`, and a card wants a percentage-width track fed straight from a [`Live`] value.
pub fn meter(
    fraction: Live<f32>,
    tint: Live<Color>,
    track: Color,
    height: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let radius = height / 2.0;
    let fill = StyledContainer::new(
        LayoutStyle::new().absolute_fill(),
        move |_r| RectStyle::filled(tint.get(), radius),
        vec![],
    )?
    .with_transform(move |r| {
        let value = fraction.get().clamp(0.0, 1.0);
        // `box_transform` pivots scale on the rect centre; a bar has to grow rightward from its left edge.
        Some(Transform::scale_around(value, 1.0, r.x, r.y + r.height / 2.0).to_array())
    });
    Ok(Box::new(StyledContainer::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(height),
        move |_r| RectStyle::filled(track, radius),
        vec![box_item(fill)],
    )?))
}

/// A label on the left, its value on the right. The label never shrinks, so a long value wraps or truncates
/// rather than squeezing the word that says what it is.
pub fn label_value(
    label: Live<String>,
    value: Live<String>,
    size: f32,
    label_color: Color,
    value_color: Color,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::auto(
        move || label.get(),
        LayoutStyle::new().flex_shrink(0.0),
        move || TextStyle::new(size, label_color),
    )?;
    let value = Text::auto(
        move || value.get(),
        LayoutStyle::new(),
        move || TextStyle::new(size, value_color),
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

/// A filled area chart over `values`, oldest first, scaled so `ceiling` reaches the top.
///
/// `ceiling` is a signal rather than a constant because half the series here have no natural one: CPU is a
/// percentage and tops out at 100, but a byte rate's full scale is whatever the last minute peaked at, and a
/// chart that rescaled only when it was rebuilt would flatten the moment traffic dropped.
pub fn sparkline(
    values: Live<Vec<f32>>,
    ceiling: Live<f32>,
    tint: Color,
    height: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let canvas = Canvas::new(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(height),
        move |rect| {
            let series = values.get();
            let Some(line) = series_path(&series, ceiling.get(), rect) else {
                return RenderNode::Empty;
            };
            let area = Arc::new(close_to_baseline(&line, rect));
            RenderNode::group([
                RenderNode::path(
                    area,
                    PathStyle::default().with_fill(tint.with_alpha(AREA_ALPHA)),
                ),
                RenderNode::path(
                    Arc::new(line),
                    PathStyle::default().with_stroke(
                        Stroke::new(tint, LINE_WIDTH)
                            .with_cap(LineCap::Round)
                            .with_join(LineJoin::Round),
                    ),
                ),
            ])
        },
    )?;
    Ok(Box::new(canvas))
}

/// The polyline through `values`. `None` for a series too short to be a line — one reading is a dot, and a
/// chart drawn from it would claim a trend the shell has not measured yet.
fn series_path(values: &[f32], ceiling: f32, rect: Rect) -> Option<PathData> {
    if values.len() < 2 || rect.width <= 0.0 || rect.height <= 0.0 {
        return None;
    }
    // The stroke is centred on the path, so a reading at either extreme would lose half its width off the edge.
    let inset = LINE_WIDTH / 2.0;
    let usable = (rect.height - LINE_WIDTH).max(1.0);
    let ceiling = ceiling.max(f32::EPSILON);
    let step = rect.width / (values.len() - 1) as f32;
    let point = |(i, value): (usize, &f32)| {
        let fraction = (value / ceiling).clamp(0.0, 1.0);
        Point::new(i as f32 * step, inset + usable * (1.0 - fraction))
    };
    let mut points = values.iter().enumerate();
    let mut path = PathData::new().move_to(point(points.next()?));
    for reading in points {
        path = path.line_to(point(reading));
    }
    Some(path)
}

/// The same line taken down to the bottom of the box and closed, which is what gives the chart its fill.
fn close_to_baseline(line: &PathData, rect: Rect) -> PathData {
    line.clone()
        .line_to(Point::new(rect.width, rect.height))
        .line_to(Point::new(0.0, rect.height))
        .close()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsx::PathVerb;

    fn rect() -> Rect {
        Rect {
            x: 0.0,
            y: 0.0,
            width: 100.0,
            height: 20.0,
        }
    }

    #[test]
    fn a_series_spans_the_box_and_reads_bottom_up() {
        let path =
            series_path(&[0.0, 50.0, 100.0], 100.0, rect()).expect("three readings is a line");
        let points: Vec<Point> = path
            .verbs()
            .iter()
            .map(|verb| match verb {
                PathVerb::MoveTo(p) | PathVerb::LineTo(p) => *p,
                other => panic!("a sparkline is only moves and lines, got {other:?}"),
            })
            .collect();
        assert_eq!(points.len(), 3);
        assert_eq!(
            points[0].x, 0.0,
            "the oldest reading starts at the left edge"
        );
        assert_eq!(points[2].x, 100.0, "the newest ends at the right");
        assert!(
            points[0].y > points[1].y && points[1].y > points[2].y,
            "a rising series climbs, so y falls: {points:?}"
        );
    }

    #[test]
    fn a_reading_over_the_ceiling_clamps_instead_of_leaving_the_box() {
        let path = series_path(&[0.0, 500.0], 100.0, rect()).expect("two readings is a line");
        for verb in path.verbs() {
            let (PathVerb::MoveTo(p) | PathVerb::LineTo(p)) = verb else {
                continue;
            };
            assert!(
                p.y >= 0.0 && p.y <= rect().height,
                "every point stays inside the box: {p:?}"
            );
        }
    }

    #[test]
    fn too_short_a_series_draws_nothing() {
        assert!(series_path(&[], 100.0, rect()).is_none());
        assert!(
            series_path(&[42.0], 100.0, rect()).is_none(),
            "one reading is not a trend"
        );
        // A collapsed box would divide by a zero width.
        assert!(
            series_path(
                &[1.0, 2.0],
                100.0,
                Rect {
                    x: 0.0,
                    y: 0.0,
                    width: 0.0,
                    height: 20.0
                }
            )
            .is_none()
        );
    }

    #[test]
    fn a_zero_ceiling_does_not_blow_the_scale_up() {
        // An idle network has peaked at nothing, and dividing by that would put every point at infinity.
        let path = series_path(&[0.0, 0.0, 0.0], 0.0, rect()).expect("still a line");
        for verb in path.verbs() {
            let (PathVerb::MoveTo(p) | PathVerb::LineTo(p)) = verb else {
                continue;
            };
            assert!(
                p.y.is_finite() && p.y <= rect().height,
                "finite and in the box: {p:?}"
            );
        }
    }

    #[test]
    fn the_area_returns_to_the_baseline_and_closes() {
        let line = series_path(&[10.0, 90.0], 100.0, rect()).expect("two readings is a line");
        let area = close_to_baseline(&line, rect());
        assert!(
            matches!(area.verbs().last(), Some(PathVerb::Close)),
            "an unclosed area fills to wherever the rasteriser decides"
        );
        let corners: Vec<&PathVerb> = area.verbs().iter().rev().skip(1).take(2).collect();
        assert!(
            corners
                .iter()
                .all(|verb| matches!(verb, PathVerb::LineTo(p) if p.y == rect().height)),
            "both closing corners sit on the bottom edge: {corners:?}"
        );
    }
}
