use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use rsx::{
    App, AppConfig, AppPathsProvider, Event, EventHandler, Key, ModifiersState,
    MultiSurfacePlatform, Platform, PlatformError, PointerButton, PointerSource, ScrollDelta,
    SurfaceId, WindowConfig, run_with_platform,
};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{
    Event as ChannelEvent, Sender as ChannelSender, channel,
};
use smithay_client_toolkit::reexports::calloop::ping::make_ping;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{KeyEvent, KeyboardHandler, Keysym, Modifiers};
use smithay_client_toolkit::seat::pointer::{PointerEvent, PointerEventKind, PointerHandler};
use smithay_client_toolkit::seat::{Capability, SeatHandler, SeatState};
use smithay_client_toolkit::shell::WaylandSurface;
use smithay_client_toolkit::shell::wlr_layer::{
    LayerShell, LayerShellHandler, LayerSurface, LayerSurfaceConfigure,
};
use smithay_client_toolkit::shm::slot::{Buffer, SlotPool};
use smithay_client_toolkit::shm::{Shm, ShmHandler};
use smithay_client_toolkit::{
    delegate_compositor, delegate_keyboard, delegate_layer, delegate_output, delegate_pointer,
    delegate_registry, delegate_seat, delegate_shm, registry_handlers,
};
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::config::{LayerConfig, OutputDescriptor};
use crate::window::LayerWindow;

thread_local! {
    static LOOP_HANDLE: RefCell<Option<LoopHandle<'static, SurfaceState>>> = const { RefCell::new(None) };
    static SURFACE_CLOSE: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
}

/// Asks the *current* surface to close — for a dynamic surface (drawer/OSD), flips its close flag so its event loop tears it down within ~50 ms. No-op on a bar surface, which has no close flag.
pub fn request_close() {
    SURFACE_CLOSE.with(|c| {
        if let Some(flag) = c.borrow().as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    });
}

pub fn interval(period: Duration, mut callback: impl FnMut() + 'static) {
    LOOP_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            let _ = handle.insert_source(
                Timer::from_duration(period),
                move |_instant, _meta, _state: &mut SurfaceState| {
                    callback();
                    TimeoutAction::ToDuration(period)
                },
            );
        }
    });
}

/// Runs `callback` once, `delay` from now, on this surface's event loop, then drops the timer. Used for
/// an OSD's auto-dismiss. No-op when called outside a surface loop (e.g. a headless test).
pub fn timeout(delay: Duration, callback: impl FnOnce() + 'static) {
    LOOP_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            let mut callback = Some(callback);
            let _ = handle.insert_source(
                Timer::from_duration(delay),
                move |_instant, _meta, _state: &mut SurfaceState| {
                    if let Some(cb) = callback.take() {
                        cb();
                    }
                    TimeoutAction::Drop
                },
            );
        }
    });
}

pub struct EventSender<T>(ChannelSender<T>);

impl<T> EventSender<T> {
    pub fn send(&self, event: T) -> bool {
        self.0.send(event).is_ok()
    }
}

pub fn watch<T, P, F>(producer: P, mut on_event: F)
where
    T: Send + 'static,
    P: FnOnce(EventSender<T>) + Send + 'static,
    F: FnMut(T) + 'static,
{
    LOOP_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            let (tx, rx) = channel::<T>();
            let _ = std::thread::Builder::new()
                .name("hyprshell-watch".to_string())
                .spawn(move || producer(EventSender(tx)));
            let _ = handle.insert_source(rx, move |event, _meta, _state: &mut SurfaceState| {
                if let ChannelEvent::Msg(item) = event {
                    on_event(item);
                }
            });
        }
    });
}

#[derive(Default)]
pub struct LayerShellPlatform {
    configs: HashMap<SurfaceId, LayerConfig>,
    shutdown: Option<Arc<AtomicBool>>,
}

impl LayerShellPlatform {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_surface(mut self, id: SurfaceId, config: LayerConfig) -> Self {
        self.configs.insert(id, config);
        self
    }

    /// Shared shutdown flag: flipping tears down all surfaces for config reload.
    pub fn with_shutdown(mut self, flag: Arc<AtomicBool>) -> Self {
        self.shutdown = Some(flag);
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
        let shutdown = self.shutdown;
        let mut joins = Vec::with_capacity(surfaces.len());
        for (id, _window_config) in surfaces {
            let layer_config = configs.get(&id).cloned().unwrap_or_default();
            let factory = Arc::clone(&factory);
            let close = shutdown.clone();
            let join = std::thread::Builder::new()
                .name(format!("hyprshell-surface-{}", id.0))
                .spawn(move || {
                    run_surface(id, layer_config, |sid| factory(sid), close);
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

struct SurfaceState {
    registry_state: RegistryState,
    output_state: OutputState,
    seat_state: SeatState,
    shm: Shm,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    window: Option<LayerWindow>,
    events: Vec<Event>,
    modifiers: ModifiersState,
    scale: i32,
    logical_size: (u32, u32),
    configured: bool,
    exit: bool,
    needs_redraw: bool,
}

fn run_surface<H: EventHandler<LayerWindow>>(
    surface_id: SurfaceId,
    config: LayerConfig,
    build_handler: impl FnOnce(SurfaceId) -> H,
    close: Option<Arc<AtomicBool>>,
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
    let shm = match Shm::bind(&globals, &qh) {
        Ok(s) => s,
        Err(e) => {
            tracing::error!("wl_shm unavailable: {e}");
            return;
        }
    };

    let mut state = SurfaceState {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        shm,
        keyboard: None,
        pointer: None,
        window: None,
        events: Vec::new(),
        modifiers: ModifiersState::default(),
        scale: 1,
        logical_size: (config.size.0.max(1), config.size.1.max(1)),
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

    LOOP_HANDLE.with(|h| *h.borrow_mut() = Some(loop_handle.clone()));

    if let Some(close) = close {
        SURFACE_CLOSE.with(|c| *c.borrow_mut() = Some(Arc::clone(&close)));
        let poll = Duration::from_millis(50);
        let _ = loop_handle.insert_source(
            Timer::from_duration(poll),
            move |_instant, _meta, state: &mut SurfaceState| {
                if close.load(Ordering::Relaxed) {
                    state.exit = true;
                }
                TimeoutAction::ToDuration(poll)
            },
        );
    }

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
    state.scale = output
        .as_ref()
        .and_then(|o| state.output_state.info(o))
        .map(|i| i.scale_factor)
        .unwrap_or(1)
        .max(1);

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
    layer.wl_surface().set_buffer_scale(state.scale);
    if config.input_transparent
        && let Ok(region) = Region::new(&compositor)
    {
        layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
    }
    layer.commit();

    if config.reserve_only {
        run_reservation_loop(&mut event_loop, &mut state, &layer);
        return;
    }

    let surface_ptr = NonNull::new(layer.wl_surface().id().as_ptr() as *mut c_void);
    let display_ptr = NonNull::new(conn.backend().display_ptr() as *mut c_void);
    let (Some(surface_ptr), Some(display_ptr)) = (surface_ptr, display_ptr) else {
        tracing::error!(
            "layer surface {surface_id:?}: null wayland pointers (system backend missing?)"
        );
        return;
    };
    let scale = state.scale.max(1) as u32;
    let window = LayerWindow::new(
        surface_ptr,
        display_ptr,
        state.logical_size.0 * scale,
        state.logical_size.1 * scale,
        state.scale as f64,
        move || {
            ping.ping();
        },
    );
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

/// Commits a fully-transparent shm buffer sized to the configured surface to hold its exclusive_zone.
fn run_reservation_loop(
    event_loop: &mut EventLoop<SurfaceState>,
    state: &mut SurfaceState,
    layer: &LayerSurface,
) {
    let mut retained: Option<(SlotPool, Buffer)> = None;
    let mut committed: (u32, u32) = (0, 0);
    loop {
        if event_loop.dispatch(None, state).is_err() || state.exit {
            drop(retained);
            return;
        }
        if !state.configured {
            continue;
        }
        let scale = state.scale.max(1) as u32;
        let w = (state.logical_size.0 * scale).max(1);
        let h = (state.logical_size.1 * scale).max(1);
        if (w, h) == committed {
            continue;
        }
        let stride = w as i32 * 4;
        let len = (h as usize) * (stride as usize);
        let mut pool = match SlotPool::new(len.max(1), &state.shm) {
            Ok(p) => p,
            Err(e) => {
                tracing::error!("reservation surface: shm pool failed: {e}");
                return;
            }
        };
        let buffer = match pool.create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888)
        {
            Ok((buffer, _canvas)) => buffer,
            Err(e) => {
                tracing::error!("reservation surface: shm buffer failed: {e}");
                return;
            }
        };
        let surface = layer.wl_surface();
        surface.set_buffer_scale(scale as i32);
        if buffer.attach_to(surface).is_ok() {
            surface.damage_buffer(0, 0, w as i32, h as i32);
            layer.commit();
            committed = (w, h);
        }
        retained = Some((pool, buffer));
    }
}

struct NoPaths;
impl AppPathsProvider for NoPaths {
    fn config_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn data_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
    fn cache_dir(&self) -> Option<std::path::PathBuf> {
        None
    }
}

struct SingleLayerPlatform {
    config: LayerConfig,
    close: Arc<AtomicBool>,
}

impl Platform for SingleLayerPlatform {
    type Window = LayerWindow;

    fn run<H: EventHandler<LayerWindow>>(
        self,
        _config: WindowConfig,
        handler: H,
    ) -> Result<(), PlatformError> {
        run_surface(
            SurfaceId(0),
            self.config,
            move |_| handler,
            Some(self.close),
        );
        Ok(())
    }
}

/// A live dynamically-opened surface. Dropping it — or calling [`close`](Self::close) — asks the surface to tear down.
pub struct SurfaceHandle {
    close: Arc<AtomicBool>,
}

impl SurfaceHandle {
    /// Asks the surface to close. Returns immediately; the surface's thread notices the flag within ~50 ms and tears itself down. Deliberately non-blocking so a UI event handler can close a drawer without stalling.
    pub fn close(&self) {
        self.close.store(true, Ordering::Relaxed);
    }

    /// Whether this surface has been asked to close (by `close`, drop, or the surface closing itself via `request_close`). Lets the owner reconcile its own toggle state after a self-close.
    pub fn is_closing(&self) -> bool {
        self.close.load(Ordering::Relaxed)
    }
}

impl Drop for SurfaceHandle {
    fn drop(&mut self) {
        self.close.store(true, Ordering::Relaxed);
    }
}

/// Opens a new layer-shell surface at runtime on its own thread (fully isolated reactive/theme/overlay world).
pub fn open_surface<A: App + Send + 'static>(spec: LayerConfig, app: A) -> SurfaceHandle {
    let close = Arc::new(AtomicBool::new(false));
    let close_for_thread = Arc::clone(&close);
    let spawned = std::thread::Builder::new()
        .name("hyprshell-surface-dyn".to_string())
        .spawn(move || {
            let platform = SingleLayerPlatform {
                config: spec,
                close: close_for_thread,
            };
            if let Err(e) = run_with_platform::<_, _, ()>(
                platform,
                AppConfig::default(),
                Box::new(NoPaths),
                app,
                "hyprshell",
            ) {
                tracing::error!("dynamic surface exited with error: {e}");
            }
        });
    if let Err(e) = spawned {
        tracing::error!("failed to spawn dynamic surface thread: {e}");
    }
    SurfaceHandle { close }
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
    let ch = event.utf8.as_deref()?.chars().next()?;
    if ch.is_control() {
        return None;
    }
    Some(Key::Char(ch))
}

impl CompositorHandler for SurfaceState {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let scale = new_factor.max(1);
        if scale == self.scale {
            return;
        }
        self.scale = scale;
        surface.set_buffer_scale(scale);
        let (lw, lh) = self.logical_size;
        if let Some(window) = &self.window {
            window.set_size(lw * scale as u32, lh * scale as u32);
            window.set_scale_factor(scale as f64);
        }
        self.events.push(Event::ScaleFactorChanged {
            scale_factor: scale as f64,
        });
        self.events.push(Event::WindowResized {
            width: lw,
            height: lh,
        });
        self.needs_redraw = true;
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
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        // configure sizes are LOGICAL. `0` on an axis means the compositor left it to us — keep the last value.
        let (mut lw, mut lh) = configure.new_size;
        if lw == 0 {
            lw = self.logical_size.0.max(1);
        }
        if lh == 0 {
            lh = self.logical_size.1.max(1);
        }
        self.logical_size = (lw, lh);
        let scale = self.scale.max(1) as u32;
        if let Some(window) = &self.window {
            window.set_size(lw * scale, lh * scale);
        }
        layer.wl_surface().set_buffer_scale(self.scale.max(1));
        if self.configured {
            self.events.push(Event::WindowResized {
                width: lw,
                height: lh,
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

impl ShmHandler for SurfaceState {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(SurfaceState);
delegate_output!(SurfaceState);
delegate_layer!(SurfaceState);
delegate_seat!(SurfaceState);
delegate_keyboard!(SurfaceState);
delegate_pointer!(SurfaceState);
delegate_shm!(SurfaceState);
delegate_registry!(SurfaceState);

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
