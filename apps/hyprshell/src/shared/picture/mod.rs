//! Reading an image file off disk into something a surface can draw.
//!
//! Three surfaces want the same three lines — the wallpaper, the dashboard's avatar and its cover art — and
//! each one that wrote them itself would also have to decide for itself what a missing file means. It means
//! `None`: the caller falls back to a colour or a glyph rather than reserving space for a picture that is not
//! coming.

use std::path::Path;
use std::sync::Arc;

use telar::{
    Image, ImageData, ImageFilter, LayoutError, LayoutItem, LayoutStyle, ObjectFit, SizeDimension,
};

/// Decodes an image file into RGBA, or `None` when the path is missing or the format is unsupported.
pub fn decode(path: &Path) -> Option<ImageData> {
    let rgba = ::image::open(path).ok()?.to_rgba8();
    let (width, height) = rgba.dimensions();
    Some(ImageData::new(rgba.into_raw(), width, height))
}

/// A fixed-size, cover-cropped picture — an avatar, a cover. Decoding happens once, at build time: the file is
/// on local disk and a surface that re-read it every frame would be doing a JPEG decode per repaint.
pub fn square(path: &Path, size: f32) -> Option<Box<dyn LayoutItem>> {
    fitted(
        path,
        LayoutStyle::new().width(size).height(size).flex_shrink(0.0),
    )
}

/// A picture that fills its container's width at a fixed height.
pub fn banner(path: &Path, height: f32) -> Option<Box<dyn LayoutItem>> {
    fitted(
        path,
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(height),
    )
}

fn fitted(path: &Path, style: LayoutStyle) -> Option<Box<dyn LayoutItem>> {
    let data = Arc::new(decode(path)?);
    let image: Result<Image, LayoutError> = Image::new(
        style,
        move || data.clone(),
        || ImageFilter::Linear,
        || ObjectFit::Cover,
    );
    match image {
        Ok(image) => Some(Box::new(image)),
        Err(e) => {
            tracing::warn!("cannot lay out {}: {e}", path.display());
            None
        }
    }
}
