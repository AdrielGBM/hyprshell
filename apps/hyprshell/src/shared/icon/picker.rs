use std::cell::Cell;
use std::rc::Rc;
use std::time::Duration;

use platform_layershell::timeout;
use rsx::{
    AlignItems, Component, Container, Effect, Event, EventResult, Input, JustifyContent, LayoutError,
    LayoutItem, LayoutScrollArea, LayoutStyle, NodeId, Overlay, ReactiveList, ReadSignal, Rect,
    RectStyle, RenderNode, RwSignal, ScrollViewport, SizeDimension, StyledContainer, Text,
    TextStyle, anchor_rect, box_item, effect, signal, use_theme,
};

use super::{CollectionState, icon_collection, icon_view};
use crate::shared::module::surface_env;
use crate::shared::theme::{FontRole, NordTheme};

const CELL: f32 = 40.0;
const ICON: f32 = 24.0;
const GRID_GAP: f32 = 4.0;
/// The visible window is grown by this many px on every side when deciding what to load, so an icon starts
/// fetching just before it scrolls into view instead of only once it is already on screen.
const PREFETCH: f32 = 96.0;
const PANEL_WIDTH: f32 = 288.0;
const GRID_HEIGHT: f32 = 240.0;
const FILTER_DEBOUNCE: Duration = Duration::from_millis(200);
/// Upper bound on results kept for the grid. Generous because the grid is virtualized (only on-screen rows
/// are built), so a whole icon set shows and scrolls cheaply; it just caps the reconciled row list.
const MAX_RESULTS: usize = 2000;

/// A floating icon picker anchored under `anchor_node`, like a dropdown menu: a click-away backdrop over the
/// whole surface, and a panel with a filter box over a scrolling grid of the default icon set. Selecting an
/// icon calls `on_select` with its `set:name` id; a click outside (or a pick) calls `on_close`.
pub fn icon_picker_overlay(
    anchor_node: NodeId,
    fallback_rect: RwSignal<Rect>,
    on_select: impl Fn(String) + 'static,
    on_close: impl Fn() + 'static,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let theme = use_theme::<NordTheme>();
    let on_close: Rc<dyn Fn()> = Rc::new(on_close);
    // Picking an icon both reports it and closes the popover.
    let pick: Rc<dyn Fn(String)> = {
        let on_close = Rc::clone(&on_close);
        Rc::new(move |id: String| {
            on_select(id);
            (on_close)();
        })
    };

    // The panel sits just under the anchor, in the surface-absolute space the overlay host portals into.
    let anchor = anchor_rect(anchor_node, &fallback_rect);
    let panel = StyledContainer::new(
        LayoutStyle::new()
            .flex_column()
            .width(PANEL_WIDTH)
            .padding_all(8.0)
            .margin_left(anchor.x)
            .margin_top(anchor.y + anchor.height + 4.0),
        move |_| RectStyle::filled(theme.surface, 10.0),
        vec![picker_body(theme, pick)?],
    )?
    // Swallow presses on the panel so they don't fall through to the backdrop and dismiss it.
    .on_press(|| {});

    let backdrop_close = Rc::clone(&on_close);
    let backdrop = StyledContainer::new(
        LayoutStyle::new().flex_column().flex_grow(1.0),
        |_| RectStyle::default(),
        vec![box_item(panel)],
    )?
    .on_press(move || (backdrop_close)());
    let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(backdrop)])?;
    Ok(box_item(overlay))
}

/// The picker's panel contents: a filter box over the scrolling grid, plus the debounced-filter effect that
/// feeds the grid. The effect is parked in the returned item so it lives exactly as long as the panel.
fn picker_body(
    theme: NordTheme,
    pick: Rc<dyn Fn(String)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let collection = icon_collection(&default_set());
    let query = signal(String::new());
    let filtered = signal(Vec::<String>::new());

    let filter_effect = debounced_filter(query.read_only(), collection.clone(), filtered.clone());

    let search = search_box(query, theme)?;
    let scroll = LayoutScrollArea::new_with(
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(GRID_HEIGHT),
        {
            let filtered = filtered.read_only();
            move |vp| results_view(vp, filtered, collection, theme, pick)
        },
    )?;
    let column = Container::new(
        LayoutStyle::new()
            .flex_column()
            .gap(8.0)
            .width(SizeDimension::Percent(1.0)),
        vec![search, Box::new(scroll)],
    )?;
    Ok(Box::new(WithEffect {
        inner: Box::new(column),
        _effect: filter_effect,
    }))
}

/// Re-filters `filtered` from `collection` a short beat after `query` (or the collection) last changed, so the
/// grid only rebuilds once typing settles rather than on every keystroke.
fn debounced_filter(
    query: ReadSignal<String>,
    collection: ReadSignal<CollectionState>,
    filtered: RwSignal<Vec<String>>,
) -> Effect {
    let generation = Rc::new(Cell::new(0u64));
    effect(move || {
        let needle = query.get();
        collection.with(|_| {}); // subscribe (without cloning the value) so a just-loaded set re-filters
        let seq = generation.get().wrapping_add(1);
        generation.set(seq);
        let guard = Rc::clone(&generation);
        let collection = collection.clone();
        let filtered = filtered.clone();
        timeout(FILTER_DEBOUNCE, move || {
            if guard.get() != seq {
                return;
            }
            let ids = collection.with(|state| match state {
                CollectionState::Ready(all) => filter_ids(all, &needle),
                _ => Vec::new(),
            });
            filtered.set(ids);
        });
    })
}

/// Icons whose name (the part after `set:`) contains `query`, case-insensitively; an empty query keeps them
/// all. Capped at [`MAX_RESULTS`].
fn filter_ids(all: &[String], query: &str) -> Vec<String> {
    let needle = query.trim().to_lowercase();
    all.iter()
        .filter(|id| needle.is_empty() || name_part(id).to_lowercase().contains(&needle))
        .take(MAX_RESULTS)
        .cloned()
        .collect()
}

fn name_part(id: &str) -> &str {
    id.split_once(':').map(|(_, name)| name).unwrap_or(id)
}

fn results_view(
    vp: ScrollViewport,
    filtered: ReadSignal<Vec<String>>,
    collection: ReadSignal<CollectionState>,
    theme: NordTheme,
    pick: Rc<dyn Fn(String)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let source_collection = collection;
    let source_filtered = filtered.clone();
    let build_filtered = filtered;
    let list = ReactiveList::new(
        move || vec![view_kind(&source_collection, &source_filtered)],
        |k: &u8| *k,
        move |kind| match kind {
            0 => message("Loading icons…", theme),
            1 => message("Couldn't load icons for this provider", theme),
            2 => message("No icons match", theme),
            _ => grid(vp.clone(), build_filtered.clone(), theme, pick.clone()),
        },
    )?;
    Ok(Box::new(list))
}

/// A coarse kind for the results area, so the outer list only swaps between message and grid when the mode
/// changes — a grid persists (and reconciles) as the filtered set changes within `Ready`.
fn view_kind(collection: &ReadSignal<CollectionState>, filtered: &ReadSignal<Vec<String>>) -> u8 {
    let empty = filtered.with(|f| f.is_empty());
    collection.with(|state| match state {
        CollectionState::Loading => 0,
        CollectionState::Unavailable => 1,
        CollectionState::Ready(_) if empty => 2,
        CollectionState::Ready(_) => 3,
    })
}

/// One row of the icon grid: `index` fixes its position; `ids` are the row's icons when it is on-screen, or
/// empty when it is a spacer (so an off-screen row costs one empty box, not a strip of icon widgets).
#[derive(Clone)]
struct Row {
    index: usize,
    ids: Vec<String>,
}

/// A virtualized grid: a flex column of fixed-height rows where only the rows in (or near) the viewport carry
/// their icons — the rest are empty spacers of the same height, so the total scroll height and every row's
/// position stay correct while only on-screen icons are ever built (and thus downloaded). Reacts to the scroll
/// offset and viewport height, so scrolling swaps spacers for real rows on the fly. One reactive list drives
/// it all, which keeps it clear of the per-cell visibility-signal pitfalls.
fn grid(
    vp: ScrollViewport,
    filtered: ReadSignal<Vec<String>>,
    theme: NordTheme,
    pick: Rc<dyn Fn(String)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let (_off_x, off_y) = vp.offset();
    let vp_rect = vp.rect();
    let source = move || rows(&filtered.get(), &vp_rect.get(), off_y.get());
    let list = ReactiveList::with_style(
        LayoutStyle::new()
            .flex_column()
            .gap(GRID_GAP)
            .width(SizeDimension::Percent(1.0)),
        source,
        // Key by position + contents, so a row rebuilds when it scrolls in/out or its icons change.
        |row: &Row| (row.index, row.ids.clone()),
        move |row: Row| build_row(row, theme, pick.clone()),
    )?;
    Ok(Box::new(list))
}

/// The rows for the current filter and scroll position: every row up to the total exists (to size the scroll),
/// but only those within a [`PREFETCH`] band of the viewport get their icon ids.
fn rows(all: &[String], vp: &Rect, scroll_y: f32) -> Vec<Row> {
    let cols = columns(vp.width);
    let row_pitch = CELL + GRID_GAP;
    let total = all.len().div_ceil(cols);
    (0..total)
        .map(|index| {
            let top = index as f32 * row_pitch;
            let on_screen =
                top + CELL > scroll_y - PREFETCH && top < scroll_y + vp.height + PREFETCH;
            let ids = if on_screen {
                all[index * cols..((index + 1) * cols).min(all.len())].to_vec()
            } else {
                Vec::new()
            };
            Row { index, ids }
        })
        .collect()
}

/// Columns that fit across `width`, from the fixed cell size and gap (at least one).
fn columns(width: f32) -> usize {
    (((width + GRID_GAP) / (CELL + GRID_GAP)).floor() as usize).max(1)
}

fn build_row(
    row: Row,
    theme: NordTheme,
    pick: Rc<dyn Fn(String)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    if row.ids.is_empty() {
        return Ok(Box::new(Container::new(
            LayoutStyle::new()
                .width(SizeDimension::Percent(1.0))
                .height(CELL),
            vec![],
        )?));
    }
    let cells = row
        .ids
        .into_iter()
        .map(|id| cell(id, theme, pick.clone()))
        .collect::<Result<Vec<_>, _>>()?;
    let row = Container::new(
        LayoutStyle::new()
            .flex_row()
            .gap(GRID_GAP)
            .width(SizeDimension::Percent(1.0)),
        cells,
    )?;
    Ok(Box::new(row))
}

/// A single icon button. Built only for on-screen rows, so constructing it (which reads — and thus downloads —
/// the glyph via [`icon_view`]) is what makes loading lazy.
fn cell(
    id: String,
    theme: NordTheme,
    pick: Rc<dyn Fn(String)>,
) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let id_icon = id.clone();
    let button = StyledContainer::new(
        LayoutStyle::new()
            .width(CELL)
            .height(CELL)
            .flex_shrink(0.0)
            .align_items(AlignItems::CENTER)
            .justify_content(JustifyContent::CENTER),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![icon_view(move || id_icon.clone(), move || theme.text, ICON)?],
    )?
    .on_hover_style(move |_| RectStyle::filled(theme.overlay, 8.0))
    .on_press(move || pick(id.clone()));
    Ok(Box::new(button))
}

fn search_box(query: RwSignal<String>, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let input = Input::new(
        query,
        LayoutStyle::new()
            .width(SizeDimension::Percent(1.0))
            .height(theme.font(FontRole::Body) * 1.4),
        move || TextStyle::new(theme.font(FontRole::Body), theme.text),
    )?
    .placeholder("Filter icons…");
    let boxed = StyledContainer::new(
        LayoutStyle::new()
            .flex_row()
            .width(SizeDimension::Percent(1.0))
            .padding_horizontal(10.0)
            .padding_vertical(8.0),
        move |_| RectStyle::filled(theme.base, 8.0),
        vec![box_item(input)],
    )?;
    Ok(Box::new(boxed))
}

fn message(text: &'static str, theme: NordTheme) -> Result<Box<dyn LayoutItem>, LayoutError> {
    let label = Text::auto(
        move || text.to_string(),
        LayoutStyle::new(),
        move || TextStyle::new(theme.font(FontRole::Caption), theme.muted),
    )?;
    let wrap = Container::new(
        LayoutStyle::new().padding_all(12.0),
        vec![Box::new(label) as Box<dyn LayoutItem>],
    )?;
    Ok(Box::new(wrap))
}

fn default_set() -> String {
    surface_env()
        .map(|e| e.config.icons.default_set.clone())
        .unwrap_or_else(|| "lucide".to_string())
}

/// Wraps a layout item so it also owns an effect for its lifetime — the two drop together when the subtree is
/// disposed. Lets a plain component tree keep a background effect (here, the debounced filter) alive.
struct WithEffect {
    inner: Box<dyn LayoutItem>,
    _effect: Effect,
}

impl LayoutItem for WithEffect {
    fn layout_node(&self) -> NodeId {
        self.inner.layout_node()
    }
}

impl Component for WithEffect {
    fn view(&self) -> RenderNode {
        self.inner.view()
    }

    fn on_event(&mut self, event: &Event) -> EventResult {
        self.inner.on_event(event)
    }

    fn debug_name(&self) -> &'static str {
        "IconPicker"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rsx::{
        App, Color, Component, SurfaceRoot, WindowConfig, reset_layout_runtime, set_theme,
        track_layout,
    };

    #[test]
    fn filter_matches_the_name_part_case_insensitively_and_caps() {
        let all: Vec<String> = ["lucide:home", "lucide:home-plus", "mdi:house", "lucide:bell"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        // Empty query keeps everything (order preserved).
        assert_eq!(filter_ids(&all, "  ").len(), 4);
        // Matches the name after `set:`, case-insensitively; `house` matches on the name, not the set.
        assert_eq!(
            filter_ids(&all, "HOM"),
            vec!["lucide:home".to_string(), "lucide:home-plus".to_string()]
        );
        assert_eq!(filter_ids(&all, "bell"), vec!["lucide:bell".to_string()]);
        assert!(filter_ids(&all, "nope").is_empty());
    }

    struct GridPreviewApp {
        ids: Vec<String>,
        theme: NordTheme,
    }

    impl App for GridPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(self.theme);
            let filtered = signal(self.ids.clone()).read_only();
            let theme = self.theme;
            let pick: Rc<dyn Fn(String)> = Rc::new(|_| {});
            let scroll = LayoutScrollArea::new_with(
                LayoutStyle::new()
                    .width(SizeDimension::Percent(1.0))
                    .height(GRID_HEIGHT),
                move |vp| grid(vp, filtered, theme, pick),
            )
            .expect("grid");
            let panel = StyledContainer::new(
                LayoutStyle::new()
                    .flex_column()
                    .width(PANEL_WIDTH)
                    .padding_all(8.0),
                move |_| RectStyle::filled(theme.surface, 10.0),
                vec![Box::new(scroll)],
            )
            .expect("panel");
            Box::new(SurfaceRoot::new(Box::new(panel)).expect("root"))
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            Some(NordTheme::new().base)
        }
    }

    // The real picker's timing: the viewport is laid out (empty grid) BEFORE the icon set loads and the grid
    // fills. A cell built after layout must still become visible once it's laid out, and request its glyph —
    // otherwise it stays stuck on the placeholder dot (the "never loads" bug).
    #[test]
    fn cell_built_after_layout_becomes_visible_and_requests_its_icon() {
        use rsx::{AvailableSpace, compute_layout, relayout_if_dirty};

        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme);
        let filtered = signal(Vec::<String>::new());
        let filtered_read = filtered.read_only();
        let pick: Rc<dyn Fn(String)> = Rc::new(|_| {});
        let scroll = LayoutScrollArea::new_with(
            LayoutStyle::new().width(280.0).height(240.0),
            move |vp| grid(vp, filtered_read, theme, pick.clone()),
        )
        .expect("scroll");
        let scroll_node = scroll.layout_node();
        let _keep = Box::new(scroll);

        // Lay the (empty) grid out first, so the scroll viewport is already sized when cells appear.
        let lay = |node| {
            compute_layout(node, AvailableSpace::Definite(280.0), AvailableSpace::Definite(240.0)).ok();
            relayout_if_dirty();
        };
        lay(scroll_node);
        assert!(
            !crate::shared::icon::was_requested("lucide:home"),
            "no cell yet, so nothing requested"
        );

        // Now the set "loads": cells are built after the viewport already has a size.
        filtered.set(vec!["lucide:home".to_string()]);
        lay(scroll_node);
        lay(scroll_node);
        assert!(
            crate::shared::icon::was_requested("lucide:home"),
            "a cell built after the viewport was laid out must still become visible and request its glyph"
        );
    }

    struct OverlayPreviewApp {
        theme: NordTheme,
    }

    impl App for OverlayPreviewApp {
        fn root(&self) -> Box<dyn Component> {
            reset_layout_runtime();
            set_theme(self.theme);
            let theme = self.theme;
            let trigger = StyledContainer::new(
                LayoutStyle::new().width(36.0).height(36.0),
                move |_| RectStyle::filled(theme.base, 8.0),
                vec![],
            )
            .expect("trigger");
            let node = trigger.layout_node();
            let rect = track_layout(node).expect("trigger node");
            let overlay =
                icon_picker_overlay(node, rect, |_| {}, || {}).expect("overlay");
            let root = Container::new(
                LayoutStyle::new().flex_column(),
                vec![Box::new(trigger), overlay],
            )
            .expect("root");
            Box::new(SurfaceRoot::new(Box::new(root)).expect("surface root"))
        }
        fn window_config(&self) -> Option<WindowConfig> {
            Some(WindowConfig {
                is_transparent: true,
                ..WindowConfig::default()
            })
        }
        fn clear_color(&self) -> Option<Color> {
            Some(NordTheme::new().base)
        }
    }

    // Faithful repro of opening the picker in a notes card: a scroll (the drawer content) holds a card with
    // a trigger and a `picking`-gated overlay holder; mount, flip picking (reconcile builds the overlay in a
    // flush), render, then tear down. Catches the reactive re-entry / overlay-host panics seen in the shell.
    #[test]
    fn overlay_inside_scroll_reconcile_and_teardown_does_not_panic() {
        use rsx::ComponentList;

        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme);
        let picking = signal(false);

        let trigger = StyledContainer::new(
            LayoutStyle::new().width(36.0).height(36.0),
            move |_| RectStyle::filled(theme.base, 8.0),
            vec![],
        )
        .expect("trigger");
        let node = trigger.layout_node();
        let rect = track_layout(node).expect("trigger node");

        let picking_src = picking.clone();
        let holder = ReactiveList::new(
            move || vec![picking_src.get()],
            |open: &bool| *open,
            move |open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
                if open {
                    icon_picker_overlay(node, rect.clone(), |_| {}, || {})
                } else {
                    Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?))
                }
            },
        )
        .expect("holder");
        let card = Container::new(
            LayoutStyle::new().flex_column(),
            vec![Box::new(trigger), Box::new(holder)],
        )
        .expect("card");
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().width(300.0).height(280.0),
            Box::new(card),
        )
        .expect("scroll");
        let root = SurfaceRoot::new(Box::new(scroll)).expect("surface root");

        let tree = ComponentList::new(root);
        let _ = tree.commands();
        picking.set(true);
        let _ = tree.commands();
        picking.set(false);
        let _ = tree.commands();
        drop(tree);
    }

    // The full path that crashed in the shell: an overlay (with its own scrolling grid) opened inside the
    // drawer's scroll, the layout host bounced across roots via relayout, the grid filled with many cells,
    // then closed and torn down. Exercises overlay attach/detach against a moving host and a big reconcile.
    #[test]
    fn overlay_grid_fills_relayouts_and_closes_without_panic() {
        use rsx::{ComponentList, relayout_if_dirty};

        reset_layout_runtime();
        let theme = NordTheme::new();
        set_theme(theme);
        let open = signal(false);
        let filtered = signal(Vec::<String>::new());

        let open_src = open.clone();
        let filtered_read = filtered.read_only();
        let holder = ReactiveList::new(
            move || vec![open_src.get()],
            |o: &bool| *o,
            move |is_open: bool| -> Result<Box<dyn LayoutItem>, LayoutError> {
                if !is_open {
                    return Ok(Box::new(Container::new(LayoutStyle::new(), vec![])?));
                }
                let filtered = filtered_read.clone();
                let pick: Rc<dyn Fn(String)> = Rc::new(|_| {});
                let scroll = LayoutScrollArea::new_with(
                    LayoutStyle::new()
                        .width(SizeDimension::Percent(1.0))
                        .height(GRID_HEIGHT),
                    move |vp| grid(vp, filtered, theme, pick.clone()),
                )?;
                let panel = StyledContainer::new(
                    LayoutStyle::new().flex_column().width(PANEL_WIDTH).padding_all(8.0),
                    move |_| RectStyle::filled(theme.surface, 10.0),
                    vec![Box::new(scroll)],
                )?
                .on_press(|| {});
                let overlay = Overlay::new(LayoutStyle::new().flex_column(), vec![box_item(panel)])?;
                Ok(box_item(overlay))
            },
        )
        .expect("holder");
        let card = Container::new(LayoutStyle::new().flex_column(), vec![Box::new(holder)])
            .expect("card");
        let scroll = LayoutScrollArea::new(
            LayoutStyle::new().width(300.0).height(280.0),
            Box::new(card),
        )
        .expect("scroll");
        let root = SurfaceRoot::new(Box::new(scroll)).expect("surface root");

        let tree = ComponentList::new(root);
        let _ = tree.commands();
        open.set(true);
        let _ = tree.commands();
        relayout_if_dirty();
        let big: Vec<String> = (0..500).map(|i| format!("lucide:icon-{i}")).collect();
        filtered.set(big);
        let _ = tree.commands();
        relayout_if_dirty();
        open.set(false);
        let _ = tree.commands();
        drop(tree);
    }

    // Builds the picker overlay in a real surface tree and drives a frame + teardown, to catch a
    // build/teardown panic (like the reactive-runtime re-entry seen when opening the popover).
    #[test]
    fn overlay_builds_and_tears_down_without_panic() {
        let out = std::env::temp_dir().join("hyprshell-overlay-test.png");
        crate::test_support::render_png(
            OverlayPreviewApp {
                theme: NordTheme::new(),
            },
            320,
            320,
            out.to_str().unwrap(),
        );
    }

    /// Renders the picker grid (cells show spinners/placeholders — headless has no network) so cell centring
    /// and the wrapping layout can be eyeballed. `RSX_VISUAL_PICKER_OUT=/tmp/p.png cargo test -p hyprshell --lib visual_picker -- --nocapture`.
    #[test]
    fn visual_picker_png() {
        let Ok(out) = std::env::var("RSX_VISUAL_PICKER_OUT") else {
            eprintln!("set RSX_VISUAL_PICKER_OUT to render the picker; skipping");
            return;
        };
        let ids: Vec<String> = [
            "home", "bell", "plus", "x", "search", "star", "heart", "settings", "user", "folder",
            "file", "clock", "calendar", "mail", "phone", "camera", "image", "music",
        ]
        .iter()
        .map(|n| format!("lucide:{n}"))
        .collect();
        crate::test_support::render_png_frames(
            GridPreviewApp {
                ids,
                theme: NordTheme::new().with_accent("teal"),
            },
            PANEL_WIDTH as u32 + 16,
            280,
            &out,
            8,
        );
    }
}
