[logic]
use crate::modules::workspaces::{Pill, pills};
use crate::shared::services::hyprland::{self, Snapshot};
use crate::shared::theme::{FontRole, NordTheme};

// The view iterates ids and reads each pill back out of the signal, rather than iterating the pills themselves:
// the transpiler builds one closure per property, and `Pill` owns a `String`, so a loop over pills would move
// it into the first closure and leave the rest without one.
fn pill_at(list: &[Pill], id: i32) -> Option<&Pill> {
    list.iter().find(|p| p.id == id)
}

// Three states, three fills: the active one takes the accent, an occupied one the surface token so it reads as
// "something lives here", and an empty one the bar's own background so it recedes.
fn pill_fill(list: Vec<Pill>, id: i32, occupied_background: bool) -> Color {
    let t = use_theme::<NordTheme>();
    let Some(pill) = pill_at(&list, id) else {
        return t.base;
    };
    if pill.active {
        t.accent
    } else if pill.occupied && occupied_background {
        t.surface
    } else {
        t.base
    }
}

fn pill_text(list: Vec<Pill>, id: i32) -> Color {
    let t = use_theme::<NordTheme>();
    let Some(pill) = pill_at(&list, id) else {
        return t.muted;
    };
    if pill.active {
        t.base
    } else if pill.occupied {
        t.text
    } else {
        t.muted
    }
}

fn pill_label(list: Vec<Pill>, id: i32) -> String {
    pill_at(&list, id).map(|p| p.label.clone()).unwrap_or_default()
}

// Resolving the socket dir per click keeps the on_press closure capture-free (it takes only the loop id).
fn focus(id: i32) {
    if let Some(dir) = hyprland::socket_dir() {
        hyprland::focus_workspace(&dir, id);
    }
}

let env = crate::surface_env();
let config = env
    .as_ref()
    .map(|e| e.config.workspaces.clone())
    .unwrap_or_default();
let output = env.as_ref().and_then(|e| e.output.clone());
let occupied_bg = config.occupied_background;

let list = signal(Vec::<Pill>::new());
let ids_src = list.read_only();
let fill_src = list.read_only();
let text_src = list.read_only();
let label_src = list.read_only();
// Subscribe to the single shared workspaces source; the consumer writes the signal on this surface's thread.
platform_layershell::watch(hyprland::subscribe, move |snap: Snapshot| {
    list.set(pills(&snap, &config, output.as_deref()));
});

let pill_ids = memo(move || ids_src.with(|l| l.iter().map(|p| p.id).collect::<Vec<i32>>()));

let vertical = crate::bar_is_vertical();
// A stretched horizontal chip can't derive its width from its height, so size both sides to make a square.
let side = crate::bar_thickness();
// Pills round like the sibling chips instead of a fixed radius, so they follow the theme/`[shape]` radius.
let rad = crate::chip_radius();
let caption = use_theme::<NordTheme>().font(FontRole::Caption);

[view]
if vertical
    col align:center
        for id in $pill_ids key *id gap:8
            box fill:pill_fill($fill_src, id, occupied_bg) radius:rad width:side height:side align:center justify:center on_press(|| focus(id))
                text "{pill_label($label_src, id)}" size:caption align:center color:pill_text($text_src, id)
else
    row align:center
        for id in $pill_ids key *id gap:8
            box fill:pill_fill($fill_src, id, occupied_bg) radius:rad width:side height:side align:center justify:center on_press(|| focus(id))
                text "{pill_label($label_src, id)}" size:caption align:center color:pill_text($text_src, id)
