//! Reading pixels back off the compositor, through `wlr-screencopy`.
//!
//! A capture is not a surface: nothing is mapped, nothing is drawn, and the answer is wanted synchronously by
//! whoever asked. So this takes its own short-lived connection — the same shape [`enumerate_outputs`] uses —
//! rather than borrowing the driver's loop, which would mean pumping a screenshot's round trips through the
//! thread every bar is painted on.
//!
//! [`enumerate_outputs`]: crate::enumerate_outputs

use std::time::{Duration, Instant};

use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_frame_v1::{
    self, ZwlrScreencopyFrameV1,
};
use smithay_client_toolkit::reexports::protocols_wlr::screencopy::v1::client::zwlr_screencopy_manager_v1::ZwlrScreencopyManagerV1;
use smithay_client_toolkit::shm::slot::SlotPool;
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{delegate_output, delegate_registry, delegate_shm, registry_handlers};
use wayland_client::globals::{GlobalList, registry_queue_init};
use wayland_client::protocol::{wl_output, wl_shm};
use wayland_client::{Connection, Dispatch, Proxy, QueueHandle, WEnum};

/// How long a capture waits for the compositor to hand a frame back. A copy is a blit the compositor does on its
/// own schedule, so it needs a bound rather than a parked thread: a shell that hangs on a screenshot is worse
/// than one that says the screenshot failed.
const DEADLINE: Duration = Duration::from_secs(3);

/// One captured image: tightly-packed RGBA8, top row first, in the output's own physical pixels.
pub struct Capture {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
    /// The output's scale factor, so a caller composing several captures knows how the pixels relate to the
    /// logical coordinates it laid the region out in.
    pub scale: i32,
}

/// Which part of the screen to read. Region coordinates are relative to the output, in its logical pixels —
/// which is what the protocol takes, so the translation from a screen-wide selection belongs to the caller that
/// knows where each output sits.
#[derive(Clone, Copy, Debug)]
pub enum CaptureArea {
    Output,
    Region {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
}

/// Whether this compositor implements `wlr-screencopy` at all. Asked before offering a capture rather than
/// after: a shell without it has a `grim` fallback, and finding out from a failed keypress is worse than
/// greying the button out.
pub fn screencopy_supported() -> bool {
    let Ok(conn) = Connection::connect_to_env() else {
        return false;
    };
    let Ok((globals, _queue)) = registry_queue_init::<CaptureState>(&conn) else {
        return false;
    };
    has_screencopy(&globals)
}

fn has_screencopy(globals: &GlobalList) -> bool {
    let wanted = ZwlrScreencopyManagerV1::interface().name;
    globals
        .contents()
        .with_list(|list| list.iter().any(|global| global.interface == wanted))
}

/// Captures `area` of `output` (the first output when unnamed), including the cursor when asked.
pub fn capture(
    output: Option<&str>,
    area: CaptureArea,
    cursor: bool,
) -> Result<Capture, CaptureError> {
    let conn = Connection::connect_to_env().map_err(|e| CaptureError(e.to_string()))?;
    let (globals, mut queue) =
        registry_queue_init::<CaptureState>(&conn).map_err(|e| CaptureError(e.to_string()))?;
    if !has_screencopy(&globals) {
        return Err(CaptureError(
            "this compositor does not implement wlr-screencopy".to_string(),
        ));
    }
    let qh = queue.handle();
    let mut state = CaptureState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        shm: Shm::bind(&globals, &qh).map_err(|e| CaptureError(e.to_string()))?,
        frame: Frame::default(),
    };
    // Two round trips: the first announces the outputs, the second delivers each one's name and mode.
    for _ in 0..2 {
        queue
            .roundtrip(&mut state)
            .map_err(|e| CaptureError(e.to_string()))?;
    }
    let target = state.output(output).ok_or_else(|| match output {
        Some(name) => CaptureError(format!("no output named '{name}'")),
        None => CaptureError("no outputs to capture".to_string()),
    })?;
    let scale = state
        .output_state
        .info(&target)
        .map(|info| info.scale_factor)
        .unwrap_or(1);

    let manager: ZwlrScreencopyManagerV1 = globals
        .bind(&qh, 1..=3, ())
        .map_err(|e| CaptureError(e.to_string()))?;
    let overlay = i32::from(cursor);
    let frame = match area {
        CaptureArea::Output => manager.capture_output(overlay, &target, &qh, ()),
        CaptureArea::Region {
            x,
            y,
            width,
            height,
        } => manager.capture_output_region(overlay, &target, x, y, width, height, &qh, ()),
    };

    // The compositor answers a capture request with the buffer it wants filled; only then is there a size to
    // allocate, which is why this cannot be one round trip.
    pump(&conn, &mut queue, &mut state, |state| {
        state.frame.buffer.is_some() || state.frame.failed
    })?;
    let Some(spec) = state.frame.buffer else {
        return Err(CaptureError(
            "the compositor refused the capture".to_string(),
        ));
    };
    let format = match spec.format {
        WEnum::Value(format) => format,
        WEnum::Unknown(raw) => return Err(CaptureError(format!("unknown pixel format {raw}"))),
    };
    if !supported_format(format) {
        return Err(CaptureError(format!("unsupported pixel format {format:?}")));
    }

    let len = spec.height as usize * spec.stride as usize;
    let mut pool =
        SlotPool::new(len.max(1), &state.shm).map_err(|e| CaptureError(e.to_string()))?;
    let (buffer, _) = pool
        .create_buffer(
            spec.width as i32,
            spec.height as i32,
            spec.stride as i32,
            format,
        )
        .map_err(|e| CaptureError(e.to_string()))?;
    frame.copy(buffer.wl_buffer());
    pump(&conn, &mut queue, &mut state, |state| {
        state.frame.ready || state.frame.failed
    })?;
    if !state.frame.ready {
        return Err(CaptureError("the capture failed".to_string()));
    }

    let canvas = buffer
        .canvas(&mut pool)
        .ok_or_else(|| CaptureError("the capture buffer was released".to_string()))?;
    let pixels = to_rgba(
        canvas,
        spec.width,
        spec.height,
        spec.stride,
        format,
        state.frame.y_invert,
    );
    frame.destroy();
    Ok(Capture {
        width: spec.width,
        height: spec.height,
        pixels,
        scale,
    })
}

/// Dispatches until `done` or the deadline. Blocking on the queue rather than spinning: a copy takes as long as
/// the compositor's next composition, and a busy loop would spend that whole frame burning a core.
fn pump(
    conn: &Connection,
    queue: &mut wayland_client::EventQueue<CaptureState>,
    state: &mut CaptureState,
    done: impl Fn(&CaptureState) -> bool,
) -> Result<(), CaptureError> {
    let start = Instant::now();
    while !done(state) {
        if start.elapsed() > DEADLINE {
            return Err(CaptureError("the compositor did not answer".to_string()));
        }
        conn.flush().map_err(|e| CaptureError(e.to_string()))?;
        queue
            .blocking_dispatch(state)
            .map_err(|e| CaptureError(e.to_string()))?;
    }
    Ok(())
}

fn supported_format(format: wl_shm::Format) -> bool {
    matches!(
        format,
        wl_shm::Format::Argb8888
            | wl_shm::Format::Xrgb8888
            | wl_shm::Format::Abgr8888
            | wl_shm::Format::Xbgr8888
    )
}

/// The compositor's buffer as tightly-packed, top-row-first RGBA8.
///
/// Three separate corrections, and every one of them is invisible in a still image until it is wrong: the
/// 32-bit formats are little-endian, so an `argb8888` buffer carries blue first; an `x` format's fourth byte is
/// undefined rather than opaque, and taking it as alpha yields a picture that is transparent in a viewer; and a
/// frame flagged `y_invert` is stored bottom-up.
fn to_rgba(
    canvas: &[u8],
    width: u32,
    height: u32,
    stride: u32,
    format: wl_shm::Format,
    y_invert: bool,
) -> Vec<u8> {
    let swap_red_blue = matches!(format, wl_shm::Format::Argb8888 | wl_shm::Format::Xrgb8888);
    let opaque = matches!(format, wl_shm::Format::Xrgb8888 | wl_shm::Format::Xbgr8888);
    let width = width as usize;
    let height = height as usize;
    let stride = stride as usize;
    let mut out = Vec::with_capacity(width * height * 4);
    for row in 0..height {
        let source = if y_invert { height - 1 - row } else { row };
        let start = source * stride;
        for column in 0..width {
            let pixel = &canvas[start + column * 4..start + column * 4 + 4];
            let (r, b) = if swap_red_blue {
                (pixel[2], pixel[0])
            } else {
                (pixel[0], pixel[2])
            };
            out.extend_from_slice(&[r, pixel[1], b, if opaque { 255 } else { pixel[3] }]);
        }
    }
    out
}

#[derive(Debug)]
pub struct CaptureError(pub String);

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for CaptureError {}

#[derive(Clone, Copy)]
struct BufferSpec {
    format: WEnum<wl_shm::Format>,
    width: u32,
    height: u32,
    stride: u32,
}

#[derive(Default)]
struct Frame {
    buffer: Option<BufferSpec>,
    ready: bool,
    failed: bool,
    y_invert: bool,
}

struct CaptureState {
    registry_state: RegistryState,
    output_state: OutputState,
    shm: Shm,
    frame: Frame,
}

impl CaptureState {
    /// The output to capture: the one named, else the first the compositor announced.
    fn output(&self, name: Option<&str>) -> Option<wl_output::WlOutput> {
        let Some(name) = name else {
            return self.output_state.outputs().next();
        };
        self.output_state.outputs().find(|output| {
            self.output_state
                .info(output)
                .and_then(|info| info.name)
                .is_some_and(|announced| announced == name)
        })
    }
}

impl OutputHandler for CaptureState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ShmHandler for CaptureState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

impl ProvidesRegistryState for CaptureState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

impl Dispatch<ZwlrScreencopyManagerV1, ()> for CaptureState {
    fn event(
        _: &mut Self,
        _: &ZwlrScreencopyManagerV1,
        _: <ZwlrScreencopyManagerV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<ZwlrScreencopyFrameV1, ()> for CaptureState {
    fn event(
        state: &mut Self,
        _: &ZwlrScreencopyFrameV1,
        event: <ZwlrScreencopyFrameV1 as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_screencopy_frame_v1::Event::Buffer {
                format,
                width,
                height,
                stride,
            } => {
                state.frame.buffer = Some(BufferSpec {
                    format,
                    width,
                    height,
                    stride,
                });
            }
            zwlr_screencopy_frame_v1::Event::Flags {
                flags: WEnum::Value(flags),
            } => {
                state.frame.y_invert = flags.contains(zwlr_screencopy_frame_v1::Flags::YInvert);
            }
            zwlr_screencopy_frame_v1::Event::Ready { .. } => state.frame.ready = true,
            zwlr_screencopy_frame_v1::Event::Failed => state.frame.failed = true,
            _ => {}
        }
    }
}

delegate_output!(CaptureState);
delegate_shm!(CaptureState);
delegate_registry!(CaptureState);

#[cfg(test)]
mod tests {
    use super::*;

    /// Four bytes per pixel, one pixel per row, two rows — enough to catch every correction at once.
    fn buffer() -> Vec<u8> {
        vec![10, 20, 30, 40, 50, 60, 70, 80]
    }

    #[test]
    fn a_little_endian_argb_buffer_comes_back_as_rgba() {
        // `argb8888` is a 32-bit word, so in memory it reads B, G, R, A. Taken as-is, every screenshot on this
        // machine would come out with its reds and blues swapped.
        let rgba = to_rgba(&buffer(), 1, 2, 4, wl_shm::Format::Argb8888, false);
        assert_eq!(rgba[0..4], [30, 20, 10, 40]);
        assert_eq!(rgba[4..8], [70, 60, 50, 80]);

        // An `xbgr` buffer is already in byte order and its fourth byte means nothing, so it must be forced
        // opaque — kept as alpha, a full-screen capture opens as a transparent image.
        let rgba = to_rgba(&buffer(), 1, 2, 4, wl_shm::Format::Xbgr8888, false);
        assert_eq!(rgba[0..4], [10, 20, 30, 255]);
    }

    #[test]
    fn a_y_inverted_frame_is_read_bottom_up() {
        let upright = to_rgba(&buffer(), 1, 2, 4, wl_shm::Format::Xbgr8888, false);
        let flipped = to_rgba(&buffer(), 1, 2, 4, wl_shm::Format::Xbgr8888, true);
        assert_eq!(flipped[0..4], upright[4..8], "the last row is drawn first");
        assert_eq!(flipped[4..8], upright[0..4]);
    }

    #[test]
    fn padding_between_rows_is_dropped_rather_than_read_as_pixels() {
        // A compositor is free to pad each row; a reader that ignores the stride shears the image.
        let padded = vec![1, 2, 3, 4, 0, 0, 0, 0, 5, 6, 7, 8, 0, 0, 0, 0];
        let rgba = to_rgba(&padded, 1, 2, 8, wl_shm::Format::Xbgr8888, false);
        assert_eq!(rgba.len(), 8, "two pixels, not four");
        assert_eq!(rgba[4..8], [5, 6, 7, 255]);
    }
}
