use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::time::Duration;

use platform_layershell::{EventSender, timeout, watch};
use rsx::{
    AssetSource, AssetState, Color, LayoutError, LayoutItem, LayoutStyle, ObjectFit, ReactiveList,
    ReadSignal, RwSignal, SpinnerProps, Svg, SvgData, signal, spinner,
};

use crate::shared::module::surface_env;

/// A transient download failure (the shell often starts before the network is up at login) keeps the icon on its spinner and re-tries a bounded number of times, so icons self-heal once connectivity arrives without hammering the endpoint over a genuine 404.
const MAX_ATTEMPTS: u32 = 8;
const RETRY_DELAY: Duration = Duration::from_secs(4);

/// A network icon addressed as `set/name` (Iconify layout). A bare name takes the configured default set; `set:name` overrides it inline, so many sets flow through one endpoint.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
struct IconId {
    set: String,
    name: String,
}

impl IconId {
    fn parse(raw: &str, default_set: &str) -> Self {
        match raw.split_once(':') {
            Some((set, name)) if !set.is_empty() && !name.is_empty() => Self {
                set: set.to_string(),
                name: name.to_string(),
            },
            _ => Self {
                set: default_set.to_string(),
                name: raw.to_string(),
            },
        }
    }

    fn cache_path(&self, root: &Path) -> PathBuf {
        root.join(&self.set).join(format!("{}.svg", self.name))
    }

    fn url(&self, provider: &str) -> String {
        format!(
            "{}/{}/{}.svg",
            provider.trim_end_matches('/'),
            self.set,
            self.name
        )
    }
}

/// What the download worker needs; owned on its own thread, so it holds only `Send` data (no signals).
#[derive(Clone)]
struct FetchConfig {
    provider: String,
    cache_dir: PathBuf,
}

type IconResult = (IconId, Option<Arc<SvgData>>);

/// The per-surface-thread reactive icon registry. Implements [`AssetSource`]: reading an icon returns a signal that starts `Loading` and advances to `Ready`/`Failed` as the download lands, re-rendering whoever read it. Transport lives in [`run_worker`]; this side only holds signals, tracks retries, and enqueues requests.
struct IconStore {
    signals: RefCell<HashMap<IconId, RwSignal<AssetState<Arc<SvgData>>>>>,
    attempts: RefCell<HashMap<IconId, u32>>,
    requests: Sender<IconId>,
    default_set: String,
}

impl AssetSource for IconStore {
    fn svg(&self, id: &str) -> ReadSignal<AssetState<Arc<SvgData>>> {
        let icon_id = IconId::parse(id, &self.default_set);
        let mut signals = self.signals.borrow_mut();
        let handle = signals.entry(icon_id.clone()).or_insert_with(|| {
            let _ = self.requests.send(icon_id.clone());
            signal(AssetState::Loading)
        });
        handle.read_only()
    }
}

thread_local! {
    static STORE: RefCell<Option<IconStore>> = const { RefCell::new(None) };
}

/// A reactive icon widget: shows the self-animating [`spinner`] while the glyph downloads (nothing is hardcoded — the spinner is rsx's own indeterminate ring), then swaps to the tinted SVG once it lands. `name` and `tint` are reactive closures, so the icon re-resolves when either changes (e.g. battery ↔ charging). Drop this into a `.rsx` view with `widget`.
pub fn icon_view(
    name: impl Fn() -> String + 'static,
    tint: impl Fn() -> Color + Clone + 'static,
    size: f32,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source = move || vec![icon_state(&name())];
    let key = |state: &AssetState<Arc<SvgData>>| state.as_ready().map(|svg| svg.id());
    let build = move |state: AssetState<Arc<SvgData>>| -> Result<Box<dyn LayoutItem>, LayoutError> {
        match state {
            AssetState::Ready(svg) => {
                let tint = tint.clone();
                let widget = Svg::new(
                    LayoutStyle::new().width(size).height(size),
                    move || svg.clone(),
                    move || Some(tint()),
                    || ObjectFit::Contain,
                )?;
                Ok(Box::new(widget))
            }
            _ => spinner(SpinnerProps {
                color: Box::new(tint.clone()),
                size,
            }),
        }
    };
    Ok(Box::new(ReactiveList::new(source, key, build)?))
}

/// The current load state of `name`, subscribing the caller so it re-renders as the icon resolves. `name` is a bare glyph (`bell`) or a `set:name` for another Iconify set (`mdi:home`).
fn icon_state(name: &str) -> AssetState<Arc<SvgData>> {
    ensure_store();
    STORE.with(|s| {
        s.borrow()
            .as_ref()
            .expect("ensure_store initializes the icon store")
            .svg(name)
            .get()
    })
}

fn ensure_store() {
    if STORE.with(|s| s.borrow().is_some()) {
        return;
    }

    let icons = surface_env()
        .map(|env| env.config.icons.clone())
        .unwrap_or_default();
    let (requests, incoming) = channel::<IconId>();
    STORE.with(|s| {
        *s.borrow_mut() = Some(IconStore {
            signals: RefCell::new(HashMap::new()),
            attempts: RefCell::new(HashMap::new()),
            requests,
            default_set: icons.default_set.clone(),
        });
    });

    let fetch = FetchConfig {
        provider: icons.provider,
        cache_dir: cache_dir(),
    };
    // When there is no layer-shell event loop (headless tests), `watch` is a no-op: no worker runs and every icon stays on its spinner, which is exactly what an offline render shows.
    watch(
        move |sender| run_worker(incoming, fetch, sender),
        |(id, data)| deliver(id, data),
    );
}

fn deliver(id: IconId, data: Option<Arc<SvgData>>) {
    STORE.with(|s| {
        let borrow = s.borrow();
        let Some(store) = borrow.as_ref() else {
            return;
        };
        match data {
            Some(svg) => {
                store.attempts.borrow_mut().remove(&id);
                if let Some(handle) = store.signals.borrow().get(&id) {
                    handle.set(AssetState::Ready(svg));
                }
            }
            None => {
                let attempts = {
                    let mut map = store.attempts.borrow_mut();
                    let count = map.entry(id.clone()).or_insert(0);
                    *count += 1;
                    *count
                };
                if attempts < MAX_ATTEMPTS {
                    let requests = store.requests.clone();
                    timeout(RETRY_DELAY, move || {
                        let _ = requests.send(id);
                    });
                } else if let Some(handle) = store.signals.borrow().get(&id) {
                    handle.set(AssetState::Failed);
                }
            }
        }
    });
}

/// Blocks on the request channel, resolving each icon from disk cache or the network and shipping the parsed `SvgData` back to the UI thread. Runs on a dedicated thread (via `watch`) and ends when the store — and thus the request sender — is dropped on surface teardown.
fn run_worker(incoming: Receiver<IconId>, fetch: FetchConfig, sender: EventSender<IconResult>) {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .build()
        .into();
    for id in incoming {
        let data = load_icon(&id, &fetch, &agent);
        if !sender.send((id, data)) {
            break;
        }
    }
}

fn load_icon(id: &IconId, fetch: &FetchConfig, agent: &ureq::Agent) -> Option<Arc<SvgData>> {
    let path = id.cache_path(&fetch.cache_dir);
    if let Ok(cached) = fs::read_to_string(&path)
        && let Ok(svg) = SvgData::from_str(&cached)
    {
        return Some(Arc::new(svg));
    }

    let body = agent
        .get(&id.url(&fetch.provider))
        .call()
        .ok()?
        .body_mut()
        .read_to_string()
        .ok()?;
    if !body.contains("<svg") {
        return None;
    }
    let svg = SvgData::from_str(&body).ok()?;
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(&path, &body);
    Some(Arc::new(svg))
}

fn cache_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .filter(|p| !p.as_os_str().is_empty())
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".cache")))
        .unwrap_or_else(|| PathBuf::from(".cache"));
    base.join("hyprshell").join("icons")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_parsing_splits_set_and_defaults() {
        let bare = IconId::parse("bell", "lucide");
        assert_eq!((bare.set.as_str(), bare.name.as_str()), ("lucide", "bell"));
        let qualified = IconId::parse("mdi:home", "lucide");
        assert_eq!(
            (qualified.set.as_str(), qualified.name.as_str()),
            ("mdi", "home")
        );
        let empty_set = IconId::parse(":oops", "lucide");
        assert_eq!(empty_set.set, "lucide", "a leading colon is not a set override");
    }

    #[test]
    fn url_and_cache_path_follow_iconify_layout() {
        let id = IconId::parse("mdi:home", "lucide");
        assert_eq!(
            id.url("https://api.iconify.design/"),
            "https://api.iconify.design/mdi/home.svg",
            "trailing slash on the provider does not double up"
        );
        let root = PathBuf::from("/cache");
        assert_eq!(id.cache_path(&root), PathBuf::from("/cache/mdi/home.svg"));
    }

    #[test]
    fn icon_state_is_loading_without_a_surface_or_network() {
        assert!(
            matches!(icon_state("bell"), AssetState::Loading),
            "with no event loop the icon has nothing to resolve from, so it stays on its spinner"
        );
    }
}
