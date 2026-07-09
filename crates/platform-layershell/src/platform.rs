use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::time::Duration;

use rsx::{
    Event, EventHandler, Key, ModifiersState, MultiSurfacePlatform, PlatformError, PointerButton,
    PointerSource, ScrollDelta, SurfaceId, Window, WindowConfig,
};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::EventLoop;
use smithay_client_toolkit::reexports::calloop::ping::make_ping;
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::config::{LayerConfig, OutputDescriptor};
use crate::window::LayerWindow;

/// A wlr-layer-shell [`MultiSurfacePlatform`]: renders one rsx tree per configured surface (a bar/panel/OSD),
/// each on its own thread with its own wayland connection — so each surface gets a fully isolated reactive/
/// theme/overlay/focus world.
#[derive(Default)]
pub struct LayerShellPlatform {
    configs: HashMap<SurfaceId, LayerConfig>,
}

impl LayerShellPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register the layer config for a surface id (matched against the `SurfaceId`s passed to
    /// `run_multi_with_platform`).
    pub fn with_surface(mut self, id: SurfaceId, config: LayerConfig) -> Self {
        self.configs.insert(id, config);
        self
    }
}

impl MultiSurfacePlatform for LayerShellPlatform {
    type Window = LayerWindow;

    fn run_surfaces<H, F>(
        self,
        surfaces: Vec<(SurfaceId, WindowConfig)>,
        factory: F,
    ) -> Result<(), PlatformError>
    where
        H: EventHandler<LayerWindow>,
        F: Fn(SurfaceId) -> H + Send + Sync + 'static,
    {
        let factory = Arc::new(factory);
        let configs = self.configs;
        let mut joins = Vec::with_capacity(surfaces.len());
        for (id, _window_config) in surfaces {
            let layer_config = configs.get(&id).cloned().unwrap_or_default();
            let factory = Arc::clone(&factory);
            let join = std::thread::Builder::new()
                .name(format!("hyprshell-surface-{}", id.0))
                .spawn(move || {
                    // The handler is built here, on this surface's thread, so its whole reactive/theme/overlay
                    // world lives in this thread's thread-locals — isolated from every other surface.
                    run_surface(id, layer_config, |sid| factory(sid));
                })
                .map_err(|e| PlatformError(format!("failed to spawn surface thread: {e}")))?;
            joins.push(join);
        }
        for join in joins {
            join.join()
                .map_err(|_| PlatformError("a surface thread panicked".to_string()))?;
        }
        Ok(())
    }
}

// Per-surface wayland state. Holds the SCTK sub-states + input objects, and accumulates translated rsx events
// (drained by the driving loop) plus flags. The rsx handler itself lives in the loop, not here, so SCTK's
// handler callbacks (which take `&mut Self`) and the rsx handler don't fight over borrows.
struct SurfaceState {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    window: Option<LayerWindow>,
    events: Vec<Event>,
    modifiers: ModifiersState,
    configured: bool,
    exit: bool,
    needs_redraw: bool,
}

// Drives one layer surface end to end on its own thread: connect, create the surface, run the event loop, and
// pump the rsx handler (built by `build_handler`) through it.
fn run_surface<H: EventHandler<LayerWindow>>(
    surface_id: SurfaceId,
    config: LayerConfig,
    build_handler: impl FnOnce(SurfaceId) -> H,
) {
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("layer surface {surface_id:?}: wayland connect failed: {e}");
            return;
        }
    };
    let (globals, event_queue) = match registry_queue_init::<SurfaceState>(&conn) {
        Ok(v) => v,
        Err(e) => {
            tracing::error!("layer surface {surface_id:?}: registry init failed: {e}");
            return;
        }
    };
    let qh = event_queue.handle();

    let compositor = match CompositorState::bind(&globals, &qh) {
        Ok(c) => c,
        Err(e) => {
            tracing::error!("wl_compositor unavailable: {e}");
            return;
        }
    };
    let layer_shell = match LayerShell::bind(&globals, &qh) {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("zwlr_layer_shell_v1 unavailable: {e}");
            return;
        }
    };

    let mut state = SurfaceState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        keyboard: None,
        pointer: None,
        window: None,
        events: Vec::new(),
        modifiers: ModifiersState::default(),
        configured: false,
        exit: false,
        needs_redraw: false,
    };

    let mut event_loop: EventLoop<SurfaceState> = match EventLoop::try_new() {
        Ok(l) => l,
        Err(e) => {
            tracing::error!("calloop init failed: {e}");
            return;
        }
    };
    let loop_handle = event_loop.handle();
    if let Err(e) = WaylandSource::new(conn.clone(), event_queue).insert(loop_handle.clone()) {
        tracing::error!("wayland source insert failed: {e}");
        return;
    }

    // request_redraw() wakes this loop from another thread (rsx flushes / the hw render thread).
    let (ping, ping_source) = match make_ping() {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("calloop ping failed: {e}");
            return;
        }
    };
    if loop_handle
        .insert_source(ping_source, |_, _, state: &mut SurfaceState| {
            state.needs_redraw = true;
        })
        .is_err()
    {
        tracing::error!("ping source insert failed");
        return;
    }

    // Populate outputs so we can pin to the requested one (a few dispatch cycles let the wl_output/xdg-output
    // events arrive).
    for _ in 0..3 {
        if event_loop
            .dispatch(Duration::from_millis(40), &mut state)
            .is_err()
        {
            return;
        }
    }
    let output = config.output.as_deref().and_then(|name| {
        state
            .output_state
            .outputs()
            .find(|o| state.output_state.info(o).and_then(|i| i.name).as_deref() == Some(name))
    });

    // Create the surface + layer surface, apply the placement, and do the initial (bufferless) commit.
    let surface = compositor.create_surface(&qh);
    let layer = layer_shell.create_layer_surface(
        &qh,
        surface,
        config.layer,
        Some(config.namespace.clone()),
        output.as_ref(),
    );
    layer.set_anchor(config.anchor);
    layer.set_size(config.size.0, config.size.1);
    layer.set_exclusive_zone(config.exclusive_zone);
    let (mt, mr, mb, ml) = config.margin;
    layer.set_margin(mt, mr, mb, ml);
    layer.set_keyboard_interactivity(config.keyboard_interactivity);
    layer.commit();

    // Raw libwayland handles for rsx's wgpu renderer.
    let surface_ptr = NonNull::new(layer.wl_surface().id().as_ptr() as *mut c_void);
    let display_ptr = NonNull::new(conn.backend().display_ptr() as *mut c_void);
    let (Some(surface_ptr), Some(display_ptr)) = (surface_ptr, display_ptr) else {
        tracing::error!(
            "layer surface {surface_id:?}: null wayland pointers (system backend missing?)"
        );
        return;
    };
    let init_w = config.size.0.max(1);
    let init_h = config.size.1.max(1);
    let window = LayerWindow::new(surface_ptr, display_ptr, init_w, init_h, 1.0, move || {
        ping.ping();
    });
    state.window = Some(window.clone());

    // Wait for the first configure so the surface has a real size before rsx builds its renderer.
    while !state.configured {
        if event_loop.dispatch(None, &mut state).is_err() {
            return;
        }
        if state.exit {
            return;
        }
    }

    let mut handler = build_handler(surface_id);
    handler.new_events();
    let resumed = handler.on_resume(&window);
    handler.about_to_wait();
    if !resumed {
        tracing::error!("layer surface {surface_id:?}: on_resume failed (renderer init)");
        return;
    }

    // Main loop: mirror the winit worker's shape (new_events → dispatch → drain events → on_redraw →
    // about_to_wait), with the dispatch timeout coming from rsx's frame pacing.
    let mut timeout: Option<Duration> = None;
    loop {
        handler.new_events();
        if event_loop.dispatch(timeout, &mut state).is_err() {
            break;
        }
        for event in state.events.drain(..) {
            handler.on_event(event, &window);
        }
        // Gated internally by tree-dirty / keepalive; `needs_redraw` is advisory (a ping/input woke us).
        state.needs_redraw = false;
        handler.on_redraw(&window);
        timeout = handler.about_to_wait();
        if state.exit {
            break;
        }
    }
    handler.on_suspend();
}

fn map_button(code: u32) -> Option<PointerButton> {
    // linux/input-event-codes.h
    match code {
        0x110 => Some(PointerButton::Primary),   // BTN_LEFT
        0x111 => Some(PointerButton::Secondary), // BTN_RIGHT
        0x112 => Some(PointerButton::Auxiliary), // BTN_MIDDLE
        _ => None,
    }
}

fn map_key(event: &KeyEvent) -> Option<Key> {
    // Minimal: printable text → Char. Named-key (arrows/Enter/…) mapping is a follow-up.
    let ch = event.utf8.as_deref()?.chars().next()?;
    if ch.is_control() {
        return None;
    }
    Some(Key::Char(ch))
}

// ---- SCTK handler impls -------------------------------------------------------------------------------

impl CompositorHandler for SurfaceState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_factor: i32,
    ) {
        // Scale handling is a follow-up; the surface renders at scale 1 for now.
    }
    fn transform_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _new_transform: wl_output::Transform,
    ) {
    }
    fn frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _time: u32,
    ) {
    }
    fn surface_enter(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
    fn surface_leave(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _surface: &wl_surface::WlSurface,
        _output: &wl_output::WlOutput,
    ) {
    }
}

impl OutputHandler for SurfaceState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl LayerShellHandler for SurfaceState {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, _layer: &LayerSurface) {
        self.exit = true;
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let (mut w, mut h) = configure.new_size;
        if let Some(window) = &self.window {
            // 0 means "the compositor left this axis to us"; keep whatever we last had there.
            if w == 0 {
                w = window.width().max(1);
            }
            if h == 0 {
                h = window.height().max(1);
            }
            window.set_size(w, h);
        }
        if self.configured {
            self.events.push(Event::WindowResized {
                width: w,
                height: h,
            });
        }
        self.configured = true;
        self.needs_redraw = true;
    }
}

impl SeatHandler for SurfaceState {
    fn seat_state(&mut self) -> &mut SeatState {
        &mut self.seat_state
    }
    fn new_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
    fn new_capability(
        &mut self,
        _conn: &Connection,
        qh: &QueueHandle<Self>,
        seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard && self.keyboard.is_none() {
            self.keyboard = self.seat_state.get_keyboard(qh, &seat, None).ok();
        }
        if capability == Capability::Pointer && self.pointer.is_none() {
            self.pointer = self.seat_state.get_pointer(qh, &seat).ok();
        }
    }
    fn remove_capability(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _seat: wl_seat::WlSeat,
        capability: Capability,
    ) {
        if capability == Capability::Keyboard {
            if let Some(kb) = self.keyboard.take() {
                kb.release();
            }
        }
        if capability == Capability::Pointer {
            if let Some(ptr) = self.pointer.take() {
                ptr.release();
            }
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for SurfaceState {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: &wl_surface::WlSurface,
        _: u32,
    ) {
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if let Some(key) = map_key(&event) {
            self.events.push(Event::KeyPressed {
                key,
                modifiers: self.modifiers,
            });
            self.needs_redraw = true;
        }
    }
    fn release_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        if let Some(key) = map_key(&event) {
            self.events.push(Event::KeyReleased {
                key,
                modifiers: self.modifiers,
            });
            self.needs_redraw = true;
        }
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _layout: u32,
    ) {
        self.modifiers = ModifiersState {
            is_shift: modifiers.shift,
            is_ctrl: modifiers.ctrl,
            is_alt: modifiers.alt,
            is_meta: modifiers.logo,
        };
    }
}

impl PointerHandler for SurfaceState {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let (x, y) = event.position;
            match event.kind {
                PointerEventKind::Enter { .. } => self.events.push(Event::CursorEntered),
                PointerEventKind::Leave { .. } => self.events.push(Event::CursorLeft),
                PointerEventKind::Motion { .. } => self.events.push(Event::PointerMoved {
                    x,
                    y,
                    source: PointerSource::Mouse,
                }),
                PointerEventKind::Press { button, .. } => {
                    if let Some(button) = map_button(button) {
                        self.events.push(Event::PointerPressed {
                            x,
                            y,
                            button,
                            source: PointerSource::Mouse,
                        });
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    if let Some(button) = map_button(button) {
                        self.events.push(Event::PointerReleased {
                            x,
                            y,
                            button,
                            source: PointerSource::Mouse,
                        });
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => self.events.push(Event::Scrolled {
                    delta: ScrollDelta::Pixels {
                        x: horizontal.absolute as f32,
                        y: vertical.absolute as f32,
                    },
                }),
            }
            self.needs_redraw = true;
        }
    }
}

impl ProvidesRegistryState for SurfaceState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

delegate_compositor!(SurfaceState);
delegate_output!(SurfaceState);
delegate_layer!(SurfaceState);
delegate_seat!(SurfaceState);
delegate_keyboard!(SurfaceState);
delegate_pointer!(SurfaceState);
delegate_registry!(SurfaceState);

// ---- Output enumeration -------------------------------------------------------------------------------

struct OutputEnumState {
    registry_state: RegistryState,
    output_state: OutputState,
}

impl OutputHandler for OutputEnumState {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
    fn output_destroyed(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {}
}

impl ProvidesRegistryState for OutputEnumState {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState];
}

delegate_output!(OutputEnumState);
delegate_registry!(OutputEnumState);

/// Enumerate the connected outputs (monitors) so a shell can build one surface per monitor. Connects, reads
/// the outputs, and disconnects. Returns an empty list if the compositor can't be reached.
pub fn enumerate_outputs() -> Vec<OutputDescriptor> {
    let Ok(conn) = Connection::connect_to_env() else {
        return Vec::new();
    };
    let Ok((globals, mut event_queue)) = registry_queue_init::<OutputEnumState>(&conn) else {
        return Vec::new();
    };
    let qh = event_queue.handle();
    let mut state = OutputEnumState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
    };
    // Two roundtrips: wl_output globals, then their xdg-output geometry.
    for _ in 0..2 {
        if event_queue.roundtrip(&mut state).is_err() {
            break;
        }
    }
    state
        .output_state
        .outputs()
        .filter_map(|o| state.output_state.info(&o))
        .map(|info| OutputDescriptor {
            name: info.name,
            logical_size: info.logical_size,
            position: info.logical_position.unwrap_or(info.location),
            scale: info.scale_factor,
        })
        .collect()
}
