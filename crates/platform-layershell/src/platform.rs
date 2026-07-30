use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use telar::{
    App, Color, Component, Event, EventHandler, Key, KeyboardMode, ModifiersState,
    MultiSurfacePlatform, NamedKey, PlatformError, PointerButton, PointerSource, ScrollDelta,
    SurfaceAnchor, SurfaceContent, SurfaceControl, SurfaceHost, SurfaceId, SurfacePlacement,
    SurfaceRole, SurfaceRoot, SurfaceScaffold, SurfaceSize, SurfaceToken, WindowConfig,
    begin_batch, build_surface_handler, end_batch, reset_layout_runtime, set_surface_host,
};
use smithay_client_toolkit::compositor::{CompositorHandler, CompositorState, Region};
use smithay_client_toolkit::output::{OutputHandler, OutputState};
use smithay_client_toolkit::reexports::calloop::channel::{
    Event as ChannelEvent, Sender as ChannelSender, channel,
};
use smithay_client_toolkit::reexports::calloop::ping::make_ping;
use smithay_client_toolkit::reexports::calloop::timer::{TimeoutAction, Timer};
use smithay_client_toolkit::reexports::calloop::{EventLoop, LoopHandle, RegistrationToken};
use smithay_client_toolkit::reexports::calloop_wayland_source::WaylandSource;
use smithay_client_toolkit::reexports::protocols::ext::idle_notify::v1::client::ext_idle_notifier_v1::ExtIdleNotifierV1;
use smithay_client_toolkit::reexports::protocols::ext::session_lock::v1::client::ext_session_lock_manager_v1::ExtSessionLockManagerV1;
use smithay_client_toolkit::reexports::protocols::ext::session_lock::v1::client::ext_session_lock_surface_v1::ExtSessionLockSurfaceV1;
use smithay_client_toolkit::registry::{ProvidesRegistryState, RegistryState};
use smithay_client_toolkit::seat::keyboard::{
    KeyEvent, KeyboardHandler, Keysym, Modifiers, RawModifiers,
};
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
use wayland_client::backend::ObjectId;
use wayland_client::globals::registry_queue_init;
use wayland_client::protocol::{wl_keyboard, wl_output, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Connection, Proxy, QueueHandle};

use crate::config::{Anchor, KeyboardInteractivity, Layer, LayerConfig, OutputDescriptor};
use crate::lock::LockSession;
use crate::window::LayerWindow;

/// The type driven every surface handler is boxed to, so one loop holds statically-declared bars and
/// runtime-opened drawers/OSDs in one `Vec` (the blanket `EventHandler for Box<dyn EventHandler>` makes the
/// box callable). All surfaces share this UI thread; isolation is the handler's own `ui_core::Surface`.
pub(crate) type BoxedHandler = Box<dyn EventHandler<LayerWindow>>;

/// The calloop sources (timers, channels) a surface registered while its handler ran, removed together when the
/// surface is torn down. Shared by `Rc` so `with_current` can hand the sink to `interval`/`watch` without
/// borrowing the driver's `SurfaceEntry`.
type SourceSink = Rc<RefCell<Vec<RegistrationToken>>>;

thread_local! {
    static LOOP_HANDLE: RefCell<Option<LoopHandle<'static, Driver>>> = const { RefCell::new(None) };
    // The close flag of the surface whose handler is currently running, so `request_close` targets it (a bar
    // has none; a dynamic drawer/OSD does). Set by the driver around each handler call.
    static CURRENT_CLOSE: RefCell<Option<Arc<AtomicBool>>> = const { RefCell::new(None) };
    // Where `interval`/`watch` file their registration tokens while a surface's handler runs, so the driver can
    // drop them with that surface. `None` outside a surface (app-level setup), where sources are process-lived.
    static CURRENT_SOURCES: RefCell<Option<SourceSink>> = const { RefCell::new(None) };
    // Dynamic surfaces requested via `open_surface` on the UI thread; the driver drains and mounts them.
    static DYN_QUEUE: RefCell<Vec<PendingSurface>> = const { RefCell::new(Vec::new()) };
    // App-level setup to run once on the driver thread after the loop is up (see `run_on_start`).
    static STARTUP: RefCell<Vec<Box<dyn FnOnce()>>> = const { RefCell::new(Vec::new()) };
    // The driver's live view of the compositor's outputs, so `outputs()` needs no second Wayland connection.
    static OUTPUTS: RefCell<Vec<OutputDescriptor>> = const { RefCell::new(Vec::new()) };
    // Notified when the output set changes once the shell is up, so the app can reconcile its surfaces (hotplug).
    static OUTPUTS_CHANGED: RefCell<Option<Box<dyn Fn()>>> = const { RefCell::new(None) };
}

/// Files `token` against the surface currently being driven, so its teardown removes the source. Outside a
/// surface the token is dropped: app-level sources (the config watcher) live as long as the process.
fn track_source(token: RegistrationToken) {
    CURRENT_SOURCES.with(|s| {
        if let Some(sink) = s.borrow().as_ref() {
            sink.borrow_mut().push(token);
        }
    });
}

/// Registers a closure to run once on the driver thread just after its loop is set up (its `LOOP_HANDLE` and
/// `SurfaceHost` installed), so app-level setup that needs `watch`/`open_surface` — e.g. the notification popup
/// host — runs on the right thread. Call it before `run_multi_with_platform` (same thread as the driver).
pub fn run_on_start(task: impl FnOnce() + 'static) {
    STARTUP.with(|s| s.borrow_mut().push(Box::new(task)));
}

struct PendingSurface {
    config: LayerConfig,
    // `None` for a reservation-only strip (no rsx handler, just its exclusive zone).
    handler: Option<BoxedHandler>,
    close: Arc<AtomicBool>,
}

/// Runs the handler closure with `close` installed as the current surface's close flag (so `request_close` and
/// any UI dismiss reach the right surface) and `sources` as the sink `interval`/`watch` file their registration
/// tokens into (so the surface's timers and channels die with it), then restores both.
fn with_current<R>(
    close: &Option<Arc<AtomicBool>>,
    sources: &SourceSink,
    f: impl FnOnce() -> R,
) -> R {
    CURRENT_CLOSE.with(|c| *c.borrow_mut() = close.clone());
    CURRENT_SOURCES.with(|s| *s.borrow_mut() = Some(Rc::clone(sources)));
    let result = f();
    CURRENT_CLOSE.with(|c| *c.borrow_mut() = None);
    CURRENT_SOURCES.with(|s| *s.borrow_mut() = None);
    result
}

/// Asks the *current* surface to close — for a dynamic surface (drawer/OSD), flips its close flag so the driver
/// tears it down on the next loop turn. No-op on a bar surface, which has no close flag.
pub fn request_close() {
    CURRENT_CLOSE.with(|c| {
        if let Some(flag) = c.borrow().as_ref() {
            flag.store(true, Ordering::Relaxed);
        }
    });
}

/// Repeats `callback` every `period` on the shared loop. Bound to the surface that registered it: when that
/// surface is torn down (a drawer closing, a bar replaced on config reload) the timer is removed with it, so a
/// reopened panel never stacks a second ticker on the first.
pub fn interval(period: Duration, mut callback: impl FnMut() + 'static) {
    LOOP_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            let registered = handle.insert_source(
                Timer::from_duration(period),
                move |_instant, _meta, _state: &mut Driver| {
                    callback();
                    TimeoutAction::ToDuration(period)
                },
            );
            if let Ok(token) = registered {
                track_source(token);
            }
        }
    });
}

/// Runs `callback` once, `delay` from now, on the shared event loop, then drops the timer. Used for an OSD's
/// auto-dismiss. No-op when called outside a surface loop (e.g. a headless test).
pub fn timeout(delay: Duration, callback: impl FnOnce() + 'static) {
    LOOP_HANDLE.with(|h| {
        if let Some(handle) = h.borrow().as_ref() {
            let mut callback = Some(callback);
            let _ = handle.insert_source(
                Timer::from_duration(delay),
                move |_instant, _meta, _state: &mut Driver| {
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

/// Runs `producer` on its own thread and delivers what it sends to `on_event` on the loop thread. Bound to the
/// surface that registered it: tearing that surface down removes the channel source, which drops the receiver
/// so the producer's next `send` fails and it winds itself down (every producer here checks that result).
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
            let registered = handle.insert_source(rx, move |event, _meta, _state: &mut Driver| {
                if let ChannelEvent::Msg(item) = event {
                    on_event(item);
                }
            });
            if let Ok(token) = registered {
                track_source(token);
            }
        }
    });
}

/// Registers the app's reaction to the compositor's output set changing after startup — a monitor plugged in or
/// unplugged — so it can open bars on the new screen and drop the ones on the old. Fires on the driver thread.
pub fn on_outputs_changed(callback: impl Fn() + 'static) {
    OUTPUTS_CHANGED.with(|c| *c.borrow_mut() = Some(Box::new(callback)));
}

/// The compositor's outputs. On the driver thread this reads the live set the driver already tracks; anywhere
/// else (before the loop is up) it falls back to a throwaway connection via [`enumerate_outputs`].
pub fn outputs() -> Vec<OutputDescriptor> {
    let cached = OUTPUTS.with(|o| o.borrow().clone());
    if cached.is_empty() {
        return enumerate_outputs();
    }
    cached
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
        H: EventHandler<LayerWindow> + 'static,
        F: Fn(SurfaceId) -> H + 'static,
    {
        run_driver(self.configs, self.shutdown, surfaces, factory)
    }
}

/// The shell object a surface is mounted through. Two roles share every other part of the driver — one
/// connection, one seat, one loop, the same rsx handler and the same `LayerWindow` bridging it to wgpu — and
/// differ only in which protocol object carries the surface and how it is configured.
pub(crate) enum Shell {
    Layer(LayerSurface),
    /// A session-lock surface, which owns its `wl_surface` directly (there is no SCTK wrapper for it).
    Lock {
        surface: wl_surface::WlSurface,
        lock: ExtSessionLockSurfaceV1,
    },
}

impl Shell {
    pub(crate) fn wl_surface(&self) -> &wl_surface::WlSurface {
        match self {
            Shell::Layer(layer) => layer.wl_surface(),
            Shell::Lock { surface, .. } => surface,
        }
    }

    fn commit(&self) {
        self.wl_surface().commit();
    }
}

/// A single mounted surface: its shell object, wgpu-bridging window, and (unless it is a reservation-only
/// strip) the rsx handler that renders it. All entries live on one thread and share one Wayland connection.
pub(crate) struct SurfaceEntry {
    pub(crate) shell: Shell,
    wl_id: ObjectId,
    window: Option<LayerWindow>,
    handler: Option<BoxedHandler>,
    // `Some` for a dynamic surface (its `SurfaceHandle`/`request_close` flag); `None` for a static bar, which
    // only closes on the shared shutdown.
    close: Option<Arc<AtomicBool>>,
    /// Timers and channel sources this surface registered (via `interval`/`watch`), removed from the loop when
    /// it is torn down so a closed drawer stops ticking instead of outliving its own signals.
    sources: SourceSink,
    /// The layer-shell namespace, so a diagnostic can name which surface an event reached.
    namespace: String,
    reserve_only: bool,
    interactive_input_region: bool,
    scale: i32,
    logical_size: (u32, u32),
    configured: bool,
    resumed: bool,
    closed: bool,
    events: Vec<Event>,
    timeout: Option<Duration>,
    input_region: Vec<(i32, i32, i32, i32)>,
    reservation: Option<(SlotPool, Buffer, (u32, u32))>,
}

impl SurfaceEntry {
    /// A surface the driver mounts with no layer-shell configuration of its own — currently only a lock
    /// surface, whose size, anchoring and input are the compositor's to decide.
    pub(crate) fn new(
        shell: Shell,
        wl_id: ObjectId,
        handler: Option<BoxedHandler>,
        close: Option<Arc<AtomicBool>>,
        namespace: String,
        scale: i32,
        logical_size: (u32, u32),
    ) -> Self {
        Self {
            shell,
            wl_id,
            window: None,
            handler,
            close,
            sources: SourceSink::default(),
            namespace,
            reserve_only: false,
            interactive_input_region: false,
            scale,
            logical_size,
            configured: false,
            resumed: false,
            closed: false,
            events: Vec::new(),
            timeout: None,
            input_region: Vec::new(),
            reservation: None,
        }
    }

    /// Adopts a compositor-decided size: records it, resizes the window behind the renderer, and (once the
    /// first configure has been taken) tells the handler to re-lay-out.
    pub(crate) fn apply_configure(&mut self, width: u32, height: u32) {
        self.logical_size = (width, height);
        let scale = self.scale.max(1) as u32;
        if let Some(window) = &self.window {
            window.set_size(width * scale, height * scale);
        }
        self.shell.wl_surface().set_buffer_scale(self.scale.max(1));
        if self.configured {
            self.events.push(Event::WindowResized { width, height });
        }
        self.configured = true;
    }
}

/// The single-thread driver: one Wayland connection's shared globals (registry/output/seat/shm) plus every
/// live surface. The SCTK delegate handlers route each event to its surface by `wl_surface` id.
pub(crate) struct Driver {
    registry_state: RegistryState,
    pub(crate) output_state: OutputState,
    seat_state: SeatState,
    shm: Shm,
    keyboard: Option<wl_keyboard::WlKeyboard>,
    pointer: Option<wl_pointer::WlPointer>,
    modifiers: ModifiersState,
    // The surface currently holding keyboard focus, so key events route to the right handler.
    keyboard_focus: Option<ObjectId>,
    pub(crate) surfaces: Vec<SurfaceEntry>,
    /// `None` where the compositor does not implement `ext-session-lock-v1`, which is what makes the shell
    /// refuse to lock rather than draw an overlay it cannot enforce.
    pub(crate) lock_manager: Option<ExtSessionLockManagerV1>,
    pub(crate) lock: Option<LockSession>,
}

/// What the shell can ask about this compositor before it commits to a feature. Read from any thread that has
/// gone through the driver, so a UI handler can grey out "lock" rather than fail on the attempt.
#[derive(Clone, Copy, Default)]
pub(crate) struct DriverFacts {
    pub(crate) lock_supported: bool,
}

thread_local! {
    static FACTS: RefCell<DriverFacts> = const { RefCell::new(DriverFacts { lock_supported: false }) };
}

pub(crate) fn with_driver_facts<R>(read: impl FnOnce(&DriverFacts) -> R) -> R {
    FACTS.with(|facts| read(&facts.borrow()))
}

impl Driver {
    fn entry_mut(&mut self, wl_id: &ObjectId) -> Option<&mut SurfaceEntry> {
        self.surfaces.iter_mut().find(|e| &e.wl_id == wl_id)
    }

    /// The layer-shell namespace of the surface an event landed on, for diagnostics. `None` means the event
    /// named a surface this driver does not own.
    fn surface_namespace(&self, wl_id: &ObjectId) -> Option<&str> {
        self.surfaces
            .iter()
            .find(|e| &e.wl_id == wl_id)
            .map(|e| e.namespace.as_str())
    }

    fn descriptors(&mut self) -> Vec<OutputDescriptor> {
        let outputs: Vec<_> = self.output_state.outputs().collect();
        outputs
            .into_iter()
            .filter_map(|o| self.output_state.info(&o))
            .map(|info| OutputDescriptor {
                name: info.name,
                logical_size: info.logical_size,
                position: info.logical_position.unwrap_or(info.location),
                scale: info.scale_factor,
            })
            .collect()
    }

    /// Refreshes the cached output set and, once the shell is up, notifies the app when it actually changed so
    /// it can open bars on a newly connected monitor and drop the ones on a disconnected one.
    fn refresh_outputs(&mut self) {
        let next = self.descriptors();
        let changed = OUTPUTS.with(|o| {
            let mut cache = o.borrow_mut();
            let changed = names(&cache) != names(&next);
            *cache = next;
            changed
        });
        if changed {
            OUTPUTS_CHANGED.with(|c| {
                if let Some(callback) = c.borrow().as_ref() {
                    callback();
                }
            });
        }
    }
}

/// Outputs compared by name: the identity a `LayerConfig` pins a surface to, so a scale or resolution change
/// (which the surface handles through `configure`) doesn't trigger a full surface reconciliation.
fn names(outputs: &[OutputDescriptor]) -> Vec<Option<String>> {
    outputs.iter().map(|o| o.name.clone()).collect()
}

#[allow(clippy::too_many_arguments)]
fn create_surface_entry(
    driver: &mut Driver,
    compositor: &CompositorState,
    layer_shell: &LayerShell,
    qh: &QueueHandle<Driver>,
    config: &LayerConfig,
    handler: Option<BoxedHandler>,
    close: Option<Arc<AtomicBool>>,
) {
    let output = config.output.as_deref().and_then(|name| {
        driver
            .output_state
            .outputs()
            .find(|o| driver.output_state.info(o).and_then(|i| i.name).as_deref() == Some(name))
    });
    let scale = output
        .as_ref()
        .and_then(|o| driver.output_state.info(o))
        .map(|i| i.scale_factor)
        .unwrap_or(1)
        .max(1);

    let surface = compositor.create_surface(qh);
    let layer = layer_shell.create_layer_surface(
        qh,
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
    layer.wl_surface().set_buffer_scale(scale);
    // A fully click-through surface, and an interactive-region one before its first frame computes its rects,
    // both start with an empty input region so they never steal clicks from windows beneath.
    if (config.input_transparent || config.interactive_input_region)
        && let Ok(region) = Region::new(compositor)
    {
        layer
            .wl_surface()
            .set_input_region(Some(region.wl_region()));
    }
    layer.commit();

    let wl_id = layer.wl_surface().id();
    let mut entry = SurfaceEntry::new(
        Shell::Layer(layer),
        wl_id,
        handler,
        close,
        config.namespace.clone(),
        scale,
        (config.size.0.max(1), config.size.1.max(1)),
    );
    entry.reserve_only = config.reserve_only;
    entry.interactive_input_region = config.interactive_input_region;
    driver.surfaces.push(entry);
}

fn run_driver<H, F>(
    configs: HashMap<SurfaceId, LayerConfig>,
    shutdown: Option<Arc<AtomicBool>>,
    surfaces: Vec<(SurfaceId, WindowConfig)>,
    factory: F,
) -> Result<(), PlatformError>
where
    H: EventHandler<LayerWindow> + 'static,
    F: Fn(SurfaceId) -> H + 'static,
{
    let conn = Connection::connect_to_env()
        .map_err(|e| PlatformError(format!("wayland connect failed: {e}")))?;
    let (globals, event_queue) = registry_queue_init::<Driver>(&conn)
        .map_err(|e| PlatformError(format!("registry init failed: {e}")))?;
    let qh = event_queue.handle();

    let compositor = CompositorState::bind(&globals, &qh)
        .map_err(|e| PlatformError(format!("wl_compositor unavailable: {e}")))?;
    let layer_shell = LayerShell::bind(&globals, &qh)
        .map_err(|e| PlatformError(format!("zwlr_layer_shell_v1 unavailable: {e}")))?;
    let shm =
        Shm::bind(&globals, &qh).map_err(|e| PlatformError(format!("wl_shm unavailable: {e}")))?;
    // Optional by design: a compositor without either protocol still runs every bar and panel. The features
    // that need them ask first (`lock_supported`, `idle_supported`) rather than failing at the point of use.
    let lock_manager = globals
        .bind::<ExtSessionLockManagerV1, Driver, ()>(&qh, 1..=1, ())
        .inspect_err(|e| tracing::info!("ext-session-lock-v1 unavailable: {e}"))
        .ok();
    let idle_notifier = globals
        .bind::<ExtIdleNotifierV1, Driver, ()>(&qh, 1..=2, ())
        .inspect_err(|e| tracing::info!("ext-idle-notify-v1 unavailable: {e}"))
        .ok();
    FACTS.with(|facts| facts.borrow_mut().lock_supported = lock_manager.is_some());

    let mut driver = Driver {
        registry_state: RegistryState::new(&globals),
        output_state: OutputState::new(&globals, &qh),
        seat_state: SeatState::new(&globals, &qh),
        shm,
        keyboard: None,
        pointer: None,
        modifiers: ModifiersState::default(),
        keyboard_focus: None,
        surfaces: Vec::new(),
        lock_manager,
        lock: None,
    };

    let mut event_loop: EventLoop<Driver> =
        EventLoop::try_new().map_err(|e| PlatformError(format!("calloop init failed: {e}")))?;
    let loop_handle = event_loop.handle();
    WaylandSource::new(conn.clone(), event_queue)
        .insert(loop_handle.clone())
        .map_err(|e| PlatformError(format!("wayland source insert failed: {e}")))?;

    // One ping wakes the shared loop; each frame re-drives every live surface (idle ones no-op internally).
    let (ping, ping_source) =
        make_ping().map_err(|e| PlatformError(format!("calloop ping failed: {e}")))?;
    loop_handle
        .insert_source(ping_source, |_, _, _: &mut Driver| {})
        .map_err(|e| PlatformError(format!("ping source insert failed: {e}")))?;

    LOOP_HANDLE.with(|h| *h.borrow_mut() = Some(loop_handle.clone()));
    set_surface_host(Box::new(LayerShellSurfaceHost));

    // Prime the registry so outputs are known before matching `config.output` on surface creation.
    for _ in 0..3 {
        if event_loop
            .dispatch(Duration::from_millis(40), &mut driver)
            .is_err()
        {
            return Ok(());
        }
    }

    // After priming, since an idle notification is taken out against a seat and the seat only exists once the
    // registry has been round-tripped. Absent either half, idle timers report themselves as unsupported.
    if let (Some(notifier), Some(seat)) = (idle_notifier, driver.seat_state.seats().next()) {
        crate::idle::install(notifier, seat, qh.clone());
    }

    for (id, _window_config) in surfaces {
        let config = configs.get(&id).cloned().unwrap_or_default();
        let handler: Option<BoxedHandler> = if config.reserve_only {
            None
        } else {
            Some(Box::new(factory(id)))
        };
        create_surface_entry(
            &mut driver,
            &compositor,
            &layer_shell,
            &qh,
            &config,
            handler,
            None,
        );
    }

    // App-level setup that needs the driver thread (LOOP_HANDLE + SurfaceHost now installed): the popup host.
    for task in STARTUP.with(|s| std::mem::take(&mut *s.borrow_mut())) {
        task();
    }

    let mut next_timeout: Option<Duration> = Some(Duration::ZERO);
    loop {
        // Before the surface pass, so a lock taken during the last dispatch has its surfaces mounted — and an
        // unlock has them torn down — in this same turn rather than one frame late.
        crate::lock::poll(&mut driver, &compositor, &qh, &conn, &loop_handle);

        // Mount any dynamic surfaces requested since the last turn (drawers/OSDs opened via `open_surface`).
        let pending: Vec<PendingSurface> = DYN_QUEUE.with(|q| std::mem::take(&mut *q.borrow_mut()));
        for p in pending {
            create_surface_entry(
                &mut driver,
                &compositor,
                &layer_shell,
                &qh,
                &p.config,
                p.handler,
                Some(p.close),
            );
        }

        // Bracket the dispatch in a reactive batch so signal writes from Wayland/calloop callbacks (an icon
        // download landing, a service update) are deferred and flushed once here, not synchronously mid-callback
        // — which under M3's shared runtime would re-enter a callback still holding a RefCell borrow. This
        // mirrors the winit runner bracketing each dispatch with the handler's new_events/about_to_wait.
        begin_batch();
        let dispatched = event_loop.dispatch(next_timeout, &mut driver);
        end_batch();
        if dispatched.is_err() {
            break;
        }
        if shutdown.as_ref().is_some_and(|f| f.load(Ordering::Relaxed)) {
            break;
        }

        let mut min_timeout: Option<Duration> = None;
        let mut remove: Vec<usize> = Vec::new();
        let display_ptr = NonNull::new(conn.backend().display_ptr() as *mut c_void);
        let Driver {
            surfaces,
            shm: shm_state,
            ..
        } = &mut driver;
        for (index, entry) in surfaces.iter_mut().enumerate() {
            if entry.closed
                || entry
                    .close
                    .as_ref()
                    .is_some_and(|f| f.load(Ordering::Relaxed))
            {
                remove.push(index);
                continue;
            }
            if !entry.configured {
                continue;
            }
            if entry.reserve_only {
                commit_reservation(shm_state, entry);
                continue;
            }
            if entry.window.is_none() {
                let Some(display_ptr) = display_ptr else {
                    tracing::error!("null wayland display pointer (system backend missing?)");
                    remove.push(index);
                    continue;
                };
                let Some(surface_ptr) =
                    NonNull::new(entry.shell.wl_surface().id().as_ptr() as *mut c_void)
                else {
                    remove.push(index);
                    continue;
                };
                let scale = entry.scale.max(1) as u32;
                let ping = ping.clone();
                entry.window = Some(LayerWindow::new(
                    surface_ptr,
                    display_ptr,
                    entry.logical_size.0 * scale,
                    entry.logical_size.1 * scale,
                    entry.scale as f64,
                    move || ping.ping(),
                ));
            }
            let window = entry.window.clone().expect("window built above");

            if !entry.resumed {
                let close = entry.close.clone();
                let sources = Rc::clone(&entry.sources);
                let ok = with_current(&close, &sources, || {
                    let handler = entry
                        .handler
                        .as_mut()
                        .expect("rendering surface has a handler");
                    handler.new_events();
                    let resumed = handler.on_resume(&window);
                    handler.about_to_wait();
                    resumed
                });
                if !ok {
                    tracing::error!("layer surface on_resume failed (renderer init)");
                    remove.push(index);
                    continue;
                }
                entry.resumed = true;
            }

            let close = entry.close.clone();
            let sources = Rc::clone(&entry.sources);
            let events: Vec<Event> = entry.events.drain(..).collect();
            entry.timeout = with_current(&close, &sources, || {
                let handler = entry
                    .handler
                    .as_mut()
                    .expect("rendering surface has a handler");
                handler.new_events();
                for event in events {
                    handler.on_event(event, &window);
                }
                handler.on_redraw(&window);
                handler.about_to_wait()
            });
            if entry.interactive_input_region {
                let rects = entry
                    .handler
                    .as_ref()
                    .map(|handler| handler.interactive_rects())
                    .unwrap_or_default();
                update_input_region(
                    &compositor,
                    entry.shell.wl_surface(),
                    &entry.namespace,
                    rects,
                    &mut entry.input_region,
                );
            }
            min_timeout = merge_timeout(min_timeout, entry.timeout);
        }

        for index in remove.into_iter().rev() {
            let entry = driver.surfaces.remove(index);
            tear_down(entry, &loop_handle);
        }

        next_timeout = min_timeout;
    }

    for entry in driver.surfaces.drain(..) {
        tear_down(entry, &loop_handle);
    }
    Ok(())
}

/// Suspends a surface's handler and then removes every loop source it registered, so its timers and watch
/// channels stop with it. Dropping the channel receivers also ends the producer threads feeding them.
pub(crate) fn tear_down(mut entry: SurfaceEntry, loop_handle: &LoopHandle<'static, Driver>) {
    if let Some(mut handler) = entry.handler.take() {
        let sources = Rc::clone(&entry.sources);
        with_current(&entry.close, &sources, || handler.on_suspend());
    }
    for token in entry.sources.borrow_mut().drain(..) {
        loop_handle.remove(token);
    }
    // A layer surface is destroyed by dropping SCTK's wrapper; a lock surface has no wrapper, so its two
    // protocol objects are released here — the role object first, as the protocol's ordering requires.
    if let Shell::Lock { surface, lock } = &entry.shell {
        lock.destroy();
        surface.destroy();
    }
}

fn merge_timeout(a: Option<Duration>, b: Option<Duration>) -> Option<Duration> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, b) => a.or(b),
    }
}

/// (Re)commits a fully-transparent shm buffer sized to the reservation strip so its exclusive_zone takes hold;
/// only rebuilds when the pixel size changed.
fn commit_reservation(shm: &Shm, entry: &mut SurfaceEntry) {
    let scale = entry.scale.max(1) as u32;
    let w = (entry.logical_size.0 * scale).max(1);
    let h = (entry.logical_size.1 * scale).max(1);
    if entry
        .reservation
        .as_ref()
        .is_some_and(|(_, _, size)| *size == (w, h))
    {
        return;
    }
    let stride = w as i32 * 4;
    let len = (h as usize) * (stride as usize);
    let mut pool = match SlotPool::new(len.max(1), shm) {
        Ok(p) => p,
        Err(e) => {
            tracing::error!("reservation surface: shm pool failed: {e}");
            return;
        }
    };
    let buffer = match pool.create_buffer(w as i32, h as i32, stride, wl_shm::Format::Argb8888) {
        Ok((buffer, _canvas)) => buffer,
        Err(e) => {
            tracing::error!("reservation surface: shm buffer failed: {e}");
            return;
        }
    };
    let surface = entry.shell.wl_surface();
    surface.set_buffer_scale(scale as i32);
    if buffer.attach_to(surface).is_ok() {
        surface.damage_buffer(0, 0, w as i32, h as i32);
        entry.shell.commit();
        entry.reservation = Some((pool, buffer, (w, h)));
    }
}

/// Rebuilds the surface's input region from its handler's pointer targets — the laid-out interactive widgets,
/// in logical surface coordinates — committing only when the set changed (`last` is the previously applied set,
/// sorted so a reordered read isn't mistaken for a change). An empty set yields an empty region, i.e. fully
/// click-through, so an overlay with no interactive content never blocks the windows beneath.
///
/// The rects come from the handler rather than from the global `telar::interactive_rects`, and that is the whole
/// correctness of this function: the registry is one of the handler's *per-surface* worlds, live only inside
/// its own calls. Read from out here — after the handler has returned — the ambient world answers, and it is
/// always empty, so every surface using this was click-through everywhere.
fn update_input_region(
    compositor: &CompositorState,
    surface: &wl_surface::WlSurface,
    namespace: &str,
    rects: Vec<telar::Rect>,
    last: &mut Vec<(i32, i32, i32, i32)>,
) {
    let mut rects: Vec<(i32, i32, i32, i32)> = rects
        .into_iter()
        .map(|r| {
            let x = r.x.floor() as i32;
            let y = r.y.floor() as i32;
            let right = (r.x + r.width).ceil() as i32;
            let bottom = (r.y + r.height).ceil() as i32;
            (x, y, right - x, bottom - y)
        })
        .collect();
    rects.sort_unstable();
    if rects == *last {
        return;
    }
    // What distinguishes "the compositor is not delivering to us" from "we told it not to": zero rects means no pointer input at all.
    tracing::debug!(
        "input region for {namespace}: {} rect(s) {rects:?}",
        rects.len()
    );
    let Ok(region) = Region::new(compositor) else {
        return;
    };
    for (x, y, w, h) in &rects {
        region.add(*x, *y, *w, *h);
    }
    surface.set_input_region(Some(region.wl_region()));
    surface.commit();
    *last = rects;
}

pub(crate) struct NoPaths;
impl telar::AppPathsProvider for NoPaths {
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

/// A live dynamically-opened surface. Dropping it — or calling [`close`](Self::close) — asks the driver to tear it down.
pub struct SurfaceHandle {
    close: Arc<AtomicBool>,
}

impl SurfaceHandle {
    /// Asks the surface to close. Returns immediately; the driver tears it down on its next loop turn.
    /// Deliberately non-blocking so a UI event handler can close a drawer without stalling.
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

impl SurfaceControl for SurfaceHandle {
    fn close(&self) {
        SurfaceHandle::close(self);
    }
    fn is_closing(&self) -> bool {
        SurfaceHandle::is_closing(self)
    }
}

/// Opens a new layer-shell surface at runtime. Builds the handler on the UI thread and enqueues it for the
/// driver to mount on its next loop turn — no new thread, so it shares the one reactive runtime (M3).
pub fn open_surface<A: App + 'static>(spec: LayerConfig, app: A) -> SurfaceHandle {
    let close = Arc::new(AtomicBool::new(false));
    let handler = build_surface_handler::<LayerWindow, A>(app, Box::new(NoPaths), "hyprshell");
    DYN_QUEUE.with(|q| {
        q.borrow_mut().push(PendingSurface {
            config: spec,
            handler: Some(handler),
            close: Arc::clone(&close),
        })
    });
    SurfaceHandle { close }
}

/// Opens a reservation-only strip (no rsx content — just its exclusive zone, an invisible transparent buffer),
/// closeable like any dynamic surface. Used to reserve bar space so the strip and the visible bar are
/// independent surfaces (see the bar/reservation split), reconcilable on config reload without a full teardown.
pub fn open_reservation(spec: LayerConfig) -> SurfaceHandle {
    let close = Arc::new(AtomicBool::new(false));
    DYN_QUEUE.with(|q| {
        q.borrow_mut().push(PendingSurface {
            config: spec,
            handler: None,
            close: Arc::clone(&close),
        })
    });
    SurfaceHandle { close }
}

/// Maps rsx's backend-agnostic [`SurfaceAnchor`] to layer-shell edge flags. `Center` anchors to no edge, so
/// the compositor centres the surface.
fn anchor_flags(anchor: SurfaceAnchor) -> Anchor {
    match anchor {
        SurfaceAnchor::Top => Anchor::TOP,
        SurfaceAnchor::Bottom => Anchor::BOTTOM,
        SurfaceAnchor::Left => Anchor::LEFT,
        SurfaceAnchor::Right => Anchor::RIGHT,
        SurfaceAnchor::Center => Anchor::empty(),
    }
}

/// Derives the layer-shell surface config from a [`SurfacePlacement`]. A placement needing a scaffold
/// (scrim or outside-dismiss) becomes a full-screen surface the `SurfaceScaffold` positions its panel
/// within; a directly-anchored one is sized and anchored by the compositor.
fn layer_config_for(placement: &SurfacePlacement) -> LayerConfig {
    let namespace = match placement.role {
        SurfaceRole::Drawer => "hyprshell-drawer",
        SurfaceRole::Popup => "hyprshell-popup",
        SurfaceRole::Osd => "hyprshell-osd",
        SurfaceRole::Float => "hyprshell-float",
        SurfaceRole::Overlay => "hyprshell-overlay",
    }
    .to_string();
    // `on-demand` grants focus on interaction (a click into a text field) without seizing it; `exclusive` holds
    // the keyboard from the moment the surface maps, which is what a launcher needs — it opens on a keybind and
    // the next keystroke is already its first search character.
    let keyboard_interactivity = match placement.keyboard {
        KeyboardMode::None => KeyboardInteractivity::None,
        KeyboardMode::OnDemand => KeyboardInteractivity::OnDemand,
        KeyboardMode::Exclusive => KeyboardInteractivity::Exclusive,
    };
    if placement.needs_scaffold() {
        LayerConfig {
            output: placement.output.clone(),
            layer: Layer::Overlay,
            anchor: Anchor::TOP
                .union(Anchor::BOTTOM)
                .union(Anchor::LEFT)
                .union(Anchor::RIGHT),
            exclusive_zone: 0,
            size: (0, 0),
            margin: (0, 0, 0, 0),
            keyboard_interactivity,
            namespace,
            reserve_only: false,
            input_transparent: false,
            interactive_input_region: false,
        }
    } else {
        let size = match placement.size {
            SurfaceSize::Fixed(w, h) => (w, h),
            SurfaceSize::Auto => (0, 0),
        };
        LayerConfig {
            output: placement.output.clone(),
            layer: Layer::Overlay,
            anchor: anchor_flags(placement.anchor),
            exclusive_zone: 0,
            size,
            margin: placement.margin,
            keyboard_interactivity,
            namespace,
            reserve_only: false,
            input_transparent: placement.input_transparent,
            interactive_input_region: false,
        }
    }
}

/// The internal rsx app for a hosted secondary surface: builds the content, wraps it in the placement's
/// scaffold (scrim + outside-dismiss) or a plain full-surface root, and arms an auto-dismiss timer when the
/// placement asks for one. The auto-dismiss captures this surface's own close flag directly, so it fires
/// regardless of which surface is current when the timer elapses.
struct HostedSurfaceApp {
    placement: SurfacePlacement,
    content: RefCell<Option<SurfaceContent>>,
    close: Arc<AtomicBool>,
}

impl App for HostedSurfaceApp {
    fn root(&self) -> Box<dyn Component> {
        reset_layout_runtime();
        let content = self
            .content
            .borrow_mut()
            .take()
            .expect("hosted surface content factory taken twice")();
        if let Some(delay) = self.placement.timeout {
            let close = Arc::clone(&self.close);
            timeout(delay, move || close.store(true, Ordering::Relaxed));
        }
        if self.placement.needs_scaffold() {
            let dismiss: Option<Rc<dyn Fn()>> = self.placement.dismiss_on_outside.then(|| {
                let close = Arc::clone(&self.close);
                Rc::new(move || close.store(true, Ordering::Relaxed)) as Rc<dyn Fn()>
            });
            Box::new(
                SurfaceScaffold::new(&self.placement, content, dismiss)
                    .expect("surface scaffold build failed")
                    .animate_in(),
            )
        } else {
            Box::new(
                SurfaceRoot::new(content)
                    .expect("surface root build failed")
                    .animate_in(),
            )
        }
    }

    fn window_config(&self) -> Option<WindowConfig> {
        // The scaffold paints its own scrim; the surface itself stays transparent so the compositor blends it.
        Some(WindowConfig {
            is_transparent: true,
            ..WindowConfig::default()
        })
    }

    fn clear_color(&self) -> Option<Color> {
        None
    }
}

/// Installed once so the shell's rsx world can open drawers/OSDs/popups via `telar::open_surface`.
struct LayerShellSurfaceHost;

impl SurfaceHost for LayerShellSurfaceHost {
    fn open(&self, placement: SurfacePlacement, content: SurfaceContent) -> SurfaceToken {
        let config = layer_config_for(&placement);
        let close = Arc::new(AtomicBool::new(false));
        let app = HostedSurfaceApp {
            placement,
            content: RefCell::new(Some(content)),
            close: Arc::clone(&close),
        };
        let handler = build_surface_handler::<LayerWindow, _>(app, Box::new(NoPaths), "hyprshell");
        DYN_QUEUE.with(|q| {
            q.borrow_mut().push(PendingSurface {
                config,
                handler: Some(handler),
                close: Arc::clone(&close),
            })
        });
        SurfaceToken::new(Box::new(SurfaceHandle { close }))
    }
}

fn map_button(code: u32) -> Option<PointerButton> {
    // Codes from linux/input-event-codes.h — not immediately obvious why these specific hex values.
    match code {
        0x110 => Some(PointerButton::Primary),
        0x111 => Some(PointerButton::Secondary),
        0x112 => Some(PointerButton::Auxiliary),
        _ => None,
    }
}

fn map_key(event: &KeyEvent) -> Option<Key> {
    // Editing keys carry a control-char `utf8` (or none), so they must be resolved from the keysym — the
    // printable `utf8` path below drops them.
    if let Some(named) = named_from_keysym(event.keysym) {
        return Some(Key::Named(named));
    }
    // Printable characters: take the resolved UTF-8, dropping any remaining control char.
    let ch = event.utf8.as_deref()?.chars().next()?;
    if ch.is_control() {
        return None;
    }
    Some(Key::Char(ch))
}

/// Maps an xkb keysym to the editing/navigation [`NamedKey`] it represents, or `None` for keys that carry
/// their own printable character. Mirrors platform-winit's named-key mapping, over xkb keysyms.
fn named_from_keysym(keysym: Keysym) -> Option<NamedKey> {
    match keysym {
        Keysym::Return | Keysym::KP_Enter => Some(NamedKey::Enter),
        Keysym::BackSpace => Some(NamedKey::Backspace),
        Keysym::Escape => Some(NamedKey::Escape),
        Keysym::Tab | Keysym::ISO_Left_Tab => Some(NamedKey::Tab),
        Keysym::Delete => Some(NamedKey::Delete),
        Keysym::Left => Some(NamedKey::ArrowLeft),
        Keysym::Right => Some(NamedKey::ArrowRight),
        Keysym::Up => Some(NamedKey::ArrowUp),
        Keysym::Down => Some(NamedKey::ArrowDown),
        Keysym::Home => Some(NamedKey::Home),
        Keysym::End => Some(NamedKey::End),
        Keysym::Page_Up => Some(NamedKey::PageUp),
        Keysym::Page_Down => Some(NamedKey::PageDown),
        Keysym::Insert => Some(NamedKey::Insert),
        _ => None,
    }
}

impl CompositorHandler for Driver {
    fn scale_factor_changed(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        surface: &wl_surface::WlSurface,
        new_factor: i32,
    ) {
        let scale = new_factor.max(1);
        let id = surface.id();
        let Some(entry) = self.entry_mut(&id) else {
            return;
        };
        if scale == entry.scale {
            return;
        }
        entry.scale = scale;
        surface.set_buffer_scale(scale);
        let (lw, lh) = entry.logical_size;
        if let Some(window) = &entry.window {
            window.set_size(lw * scale as u32, lh * scale as u32);
            window.set_scale_factor(scale as f64);
        }
        entry.events.push(Event::ScaleFactorChanged {
            scale_factor: scale as f64,
        });
        entry.events.push(Event::WindowResized {
            width: lw,
            height: lh,
        });
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

impl OutputHandler for Driver {
    fn output_state(&mut self) -> &mut OutputState {
        &mut self.output_state
    }
    fn new_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.refresh_outputs();
    }
    fn update_output(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_output::WlOutput) {
        self.refresh_outputs();
    }
    fn output_destroyed(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        output: wl_output::WlOutput,
    ) {
        // Before the refresh, so a monitor unplugged and plugged back in gets a fresh lock surface instead of
        // being skipped as one this session already covered.
        crate::lock::forget_output(self, &output);
        self.refresh_outputs();
    }
}

impl LayerShellHandler for Driver {
    fn closed(&mut self, _conn: &Connection, _qh: &QueueHandle<Self>, layer: &LayerSurface) {
        let id = layer.wl_surface().id();
        if let Some(entry) = self.entry_mut(&id) {
            entry.closed = true;
        }
    }

    fn configure(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        layer: &LayerSurface,
        configure: LayerSurfaceConfigure,
        _serial: u32,
    ) {
        let id = layer.wl_surface().id();
        let Some(entry) = self.entry_mut(&id) else {
            return;
        };
        // configure sizes are LOGICAL. `0` on an axis means the compositor left it to us — keep the last value.
        let (mut lw, mut lh) = configure.new_size;
        if lw == 0 {
            lw = entry.logical_size.0.max(1);
        }
        if lh == 0 {
            lh = entry.logical_size.1.max(1);
        }
        entry.apply_configure(lw, lh);
    }
}

impl SeatHandler for Driver {
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
        if capability == Capability::Keyboard
            && let Some(kb) = self.keyboard.take()
        {
            kb.release();
        }
        if capability == Capability::Pointer
            && let Some(ptr) = self.pointer.take()
        {
            ptr.release();
        }
    }
    fn remove_seat(&mut self, _: &Connection, _: &QueueHandle<Self>, _: wl_seat::WlSeat) {}
}

impl KeyboardHandler for Driver {
    fn enter(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
        _: &[u32],
        _: &[Keysym],
    ) {
        self.keyboard_focus = Some(surface.id());
    }
    fn leave(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        surface: &wl_surface::WlSurface,
        _: u32,
    ) {
        if self.keyboard_focus.as_ref() == Some(&surface.id()) {
            self.keyboard_focus = None;
        }
    }
    fn press_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let modifiers = self.modifiers;
        if let (Some(key), Some(id)) = (map_key(&event), self.keyboard_focus.clone())
            && let Some(entry) = self.entry_mut(&id)
        {
            entry.events.push(Event::KeyPressed { key, modifiers });
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
        let modifiers = self.modifiers;
        if let (Some(key), Some(id)) = (map_key(&event), self.keyboard_focus.clone())
            && let Some(entry) = self.entry_mut(&id)
        {
            entry.events.push(Event::KeyReleased { key, modifiers });
        }
    }
    fn repeat_key(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        event: KeyEvent,
    ) {
        let modifiers = self.modifiers;
        if let (Some(key), Some(id)) = (map_key(&event), self.keyboard_focus.clone())
            && let Some(entry) = self.entry_mut(&id)
        {
            entry.events.push(Event::KeyPressed { key, modifiers });
        }
    }
    fn update_modifiers(
        &mut self,
        _: &Connection,
        _: &QueueHandle<Self>,
        _: &wl_keyboard::WlKeyboard,
        _: u32,
        modifiers: Modifiers,
        _: RawModifiers,
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

impl PointerHandler for Driver {
    fn pointer_frame(
        &mut self,
        _conn: &Connection,
        _qh: &QueueHandle<Self>,
        _pointer: &wl_pointer::WlPointer,
        events: &[PointerEvent],
    ) {
        for event in events {
            let id = event.surface.id();
            let (x, y) = event.position;
            let telar_event = match event.kind {
                // An enter carries the pointer's position and a widget resolves its hover from a move, so delivering it as bare "the cursor is over this surface" leaves a pointer that arrives and stops hovering nothing. `CursorEntered` is still emitted first, for whatever tracks the surface rather than the widget.
                PointerEventKind::Enter { .. } | PointerEventKind::Motion { .. } => {
                    Event::PointerMoved {
                        x,
                        y,
                        source: PointerSource::Mouse,
                    }
                }
                PointerEventKind::Leave { .. } => Event::CursorLeft,
                PointerEventKind::Press { button, .. } => {
                    let Some(button) = map_button(button) else {
                        continue;
                    };
                    // Whether a press reached a surface at all is the one thing that distinguishes "our input
                    // region is wrong" from "the compositor acted on an event it also delivered to us". Logged
                    // at debug so `RUST_LOG=platform_layershell=debug` can answer it without a custom build.
                    tracing::debug!(
                        "pointer press {button:?} at ({x:.0},{y:.0}) delivered to surface {:?}",
                        self.surface_namespace(&id)
                    );
                    Event::PointerPressed {
                        x,
                        y,
                        button,
                        source: PointerSource::Mouse,
                    }
                }
                PointerEventKind::Release { button, .. } => {
                    let Some(button) = map_button(button) else {
                        continue;
                    };
                    Event::PointerReleased {
                        x,
                        y,
                        button,
                        source: PointerSource::Mouse,
                    }
                }
                PointerEventKind::Axis {
                    horizontal,
                    vertical,
                    ..
                } => Event::Scrolled {
                    // Wayland axis is positive down/right; negate to the winit convention the shared scroll area expects (`offset -= delta`) so the view tracks the gesture.
                    delta: ScrollDelta::Pixels {
                        x: -(horizontal.absolute as f32),
                        y: -(vertical.absolute as f32),
                    },
                },
            };
            if let Some(entry) = self.entry_mut(&id) {
                if matches!(event.kind, PointerEventKind::Enter { .. }) {
                    entry.events.push(Event::CursorEntered);
                }
                entry.events.push(telar_event);
            }
        }
    }
}

impl ProvidesRegistryState for Driver {
    fn registry(&mut self) -> &mut RegistryState {
        &mut self.registry_state
    }
    registry_handlers![OutputState, SeatState];
}

impl ShmHandler for Driver {
    fn shm_state(&mut self) -> &mut Shm {
        &mut self.shm
    }
}

delegate_compositor!(Driver);
delegate_output!(Driver);
delegate_layer!(Driver);
delegate_seat!(Driver);
delegate_keyboard!(Driver);
delegate_pointer!(Driver);
delegate_shm!(Driver);
delegate_registry!(Driver);

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn editing_keysyms_map_to_named_keys() {
        assert_eq!(named_from_keysym(Keysym::Return), Some(NamedKey::Enter));
        assert_eq!(named_from_keysym(Keysym::KP_Enter), Some(NamedKey::Enter));
        assert_eq!(
            named_from_keysym(Keysym::BackSpace),
            Some(NamedKey::Backspace)
        );
        assert_eq!(named_from_keysym(Keysym::Escape), Some(NamedKey::Escape));
        assert_eq!(named_from_keysym(Keysym::Tab), Some(NamedKey::Tab));
        assert_eq!(named_from_keysym(Keysym::Left), Some(NamedKey::ArrowLeft));
        assert_eq!(named_from_keysym(Keysym::Right), Some(NamedKey::ArrowRight));
    }
}
