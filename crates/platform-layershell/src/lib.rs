mod capture;
mod clipboard;
mod config;
mod globals;
mod idle;
mod link;
mod lock;
mod platform;
mod window;

pub use capture::{Capture, CaptureArea, CaptureError, capture, screencopy_supported};
pub use clipboard::{clipboard_supported, set_selection};
pub use config::{Anchor, KeyboardInteractivity, Layer, LayerConfig, OutputDescriptor};
pub use globals::advertises;
pub use idle::{IdleHandle, idle_notification, idle_supported};
pub use link::SurfaceUpdate;
pub use lock::{LockHandle, lock_session, lock_supported};
pub use platform::{
    EventSender, LayerShellPlatform, SurfaceHandle, enumerate_outputs, interval, on_close,
    on_outputs_changed, open_reservation, open_surface, outputs, request_close, request_margin,
    request_size, run_on_start, timeout, watch,
};
pub use window::LayerWindow;
