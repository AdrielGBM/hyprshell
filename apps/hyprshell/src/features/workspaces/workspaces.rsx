[logic]
use crate::shared::hyprland::{self, Snapshot};
use crate::core::theme::NordTheme;

enum ChipState {
    Active,
    Occupied,
    Empty,
}

fn chip_state(snap: &Snapshot, id: i32) -> ChipState {
    if snap.active == id {
        ChipState::Active
    } else if snap.workspaces.iter().any(|w| w.id == id && w.windows > 0) {
        ChipState::Occupied
    } else {
        ChipState::Empty
    }
}

fn chip_fill(snap: Snapshot, id: i32) -> Color {
    let t = use_theme::<NordTheme>();
    match chip_state(&snap, id) {
        ChipState::Active => t.accent,
        ChipState::Occupied => t.surface,
        ChipState::Empty => t.base,
    }
}

fn chip_text(snap: Snapshot, id: i32) -> Color {
    let t = use_theme::<NordTheme>();
    match chip_state(&snap, id) {
        ChipState::Active => t.base,
        ChipState::Occupied => t.text,
        ChipState::Empty => t.muted,
    }
}

// Resolving the socket dir per click keeps the on_press closure capture-free (it takes only the loop id).
fn focus(id: i32) {
    if let Some(dir) = hyprland::socket_dir() {
        hyprland::focus_workspace(&dir, id);
    }
}

let snapshot = signal(Snapshot::default());
let ids_src = snapshot.read_only();
let fill_src = snapshot.read_only();
let text_src = snapshot.read_only();
// The producer blocks on the socket; the consumer writes the signal on this surface's thread.
if let Some(watch_dir) = hyprland::socket_dir() {
    platform_layershell::watch(
        move |tx| hyprland::stream_workspaces(watch_dir, tx),
        move |snap| snapshot.set(snap),
    );
}
let workspace_ids =
    memo(move || ids_src.with(|s| s.workspaces.iter().map(|w| w.id).collect::<Vec<i32>>()));
// A vertical bar (left/right edge) stacks the chips as a column of squares; a horizontal one keeps them in a row of pills.
let vertical = crate::bar_is_vertical();

[view]
if vertical
    col align:center
        for id in $workspace_ids key *id gap:8
            box fill:chip_fill($fill_src, id) radius:6 width:24 height:24 align:center justify:center on_press(|| focus(id))
                text "{id}" size:13 color:chip_text($text_src, id)
else
    row align:center
        for id in $workspace_ids key *id gap:8
            box fill:chip_fill($fill_src, id) radius:6 height:20 pad_x:8 align:center justify:center on_press(|| focus(id))
                text "{id}" size:13 color:chip_text($text_src, id)
