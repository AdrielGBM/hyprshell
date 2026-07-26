//! Which workspaces the bar shows, and what each pill says.

use crate::core::config::WorkspacesConfig;
use crate::shared::services::hyprland::{Snapshot, Workspace};

/// A pill the bar draws: either a workspace that exists, or a placeholder holding its slot.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pill {
    pub id: i32,
    pub label: String,
    pub occupied: bool,
    pub active: bool,
    pub special: bool,
    /// The window classes to draw icons for, already capped.
    pub clients: Vec<String>,
    /// An Iconify glyph that replaces the label — configured per special workspace.
    pub icon: Option<String>,
}

/// The pills to draw for a snapshot.
///
/// With `shown = 0` this is exactly the workspaces that exist. With `shown = N` the bar always draws N pills,
/// filling gaps with placeholders — which is the point: a bar whose width changes every time a workspace is
/// created or destroyed makes every module to its right jump around.
pub fn pills(snapshot: &Snapshot, config: &WorkspacesConfig, output: Option<&str>) -> Vec<Pill> {
    let mut visible: Vec<&Workspace> = snapshot
        .workspaces
        .iter()
        .filter(|w| !w.is_special() && belongs_here(w, config, output, snapshot))
        .collect();
    visible.sort_by_key(|w| w.id);

    let mut pills: Vec<Pill> = if config.shown == 0 {
        visible
            .iter()
            .enumerate()
            .map(|(index, w)| pill_for(w, config, index, snapshot.active))
            .collect()
    } else {
        fixed_window(&visible, config, snapshot.active)
    };

    if config.show_special {
        let specials = snapshot
            .workspaces
            .iter()
            .filter(|w| w.is_special() && w.is_occupied())
            .filter(|w| belongs_here(w, config, output, snapshot));
        for (index, w) in specials.enumerate() {
            pills.push(pill_for(w, config, index, snapshot.active));
        }
    }
    pills
}

/// Whether a workspace belongs on this bar. Without `per_monitor` every bar shows every workspace; with it, a
/// bar shows only its own monitor's — which is what a multi-head setup usually means by "my workspaces".
fn belongs_here(
    workspace: &Workspace,
    config: &WorkspacesConfig,
    output: Option<&str>,
    snapshot: &Snapshot,
) -> bool {
    if !config.per_monitor {
        return true;
    }
    // A bar with no output name (a single-monitor setup, or a surface the compositor didn't name) falls back to
    // the focused monitor rather than hiding everything.
    let mine = output.unwrap_or(&snapshot.focused_monitor);
    mine.is_empty() || workspace.monitor.is_empty() || workspace.monitor == mine
}

/// A fixed-width run of `shown` ids, anchored so the active workspace is always inside it.
///
/// Anchoring on the active one is what lets `shown = 5` work on a setup that uses workspaces 1–20: the window
/// slides to follow you instead of stranding you off the end of a fixed 1–5.
fn fixed_window(visible: &[&Workspace], config: &WorkspacesConfig, active: i32) -> Vec<Pill> {
    let count = config.shown as i32;
    let lowest = visible.first().map(|w| w.id).unwrap_or(1).max(1);
    let start = if active >= lowest && active < lowest + count {
        lowest
    } else if active >= lowest + count {
        (active - count + 1).max(1)
    } else {
        active.max(1)
    };

    (0..count)
        .map(|offset| {
            let id = start + offset;
            match visible.iter().find(|w| w.id == id) {
                Some(w) => pill_for(w, config, offset as usize, active),
                // A slot with no workspace behind it is still clickable: pressing it is how you get there.
                None => Pill {
                    id,
                    label: config.render_label(
                        id,
                        &id.to_string(),
                        offset as usize,
                        false,
                        id == active,
                    ),
                    occupied: false,
                    active: id == active,
                    special: false,
                    clients: Vec::new(),
                    icon: None,
                },
            }
        })
        .collect()
}

fn pill_for(w: &Workspace, config: &WorkspacesConfig, index: usize, active: i32) -> Pill {
    let icon = if w.is_special() {
        config.special_icons.get(w.special_name()).cloned()
    } else {
        None
    };
    let is_active = w.id == active;
    let label = if w.is_special() {
        config.capitalize.apply(w.special_name())
    } else {
        config.render_label(w.id, &w.name, index, w.is_occupied(), is_active)
    };
    Pill {
        id: w.id,
        label,
        occupied: w.is_occupied(),
        active: is_active,
        special: w.is_special(),
        clients: if config.window_icons {
            w.clients
                .iter()
                .take(config.max_window_icons as usize)
                .cloned()
                .collect()
        } else {
            Vec::new()
        },
        icon,
    }
}

/// The workspace one wheel notch away from `active`, clamped to the ones that exist. Returns `None` when there
/// is nowhere to go, so the handler can leave the compositor alone rather than dispatching a no-op.
pub fn scroll_target(snapshot: &Snapshot, up: bool) -> Option<i32> {
    let mut ids: Vec<i32> = snapshot
        .workspaces
        .iter()
        .filter(|w| !w.is_special())
        .map(|w| w.id)
        .collect();
    ids.sort_unstable();
    let at = ids.iter().position(|id| *id == snapshot.active)?;
    let next = if up {
        at.checked_sub(1)?
    } else {
        at.checked_add(1).filter(|n| *n < ids.len())?
    };
    ids.get(next).copied()
}

/// The wheel over the pills switches workspace, when `[workspaces] scroll` allows it.
///
/// Reads the shared snapshot rather than the compositor: a wheel notch must not spend a socket round-trip
/// deciding where to go, and the service already holds the answer.
pub fn scroll(_dx: f32, dy: f32) {
    let enabled = crate::core::shell::config()
        .map(|c| c.workspaces.scroll)
        .unwrap_or(true);
    if !enabled {
        return;
    }
    let Some(snapshot) = crate::shared::services::hyprland::current_workspaces() else {
        return;
    };
    // `dy > 0` is a scroll up (the platform already flips Wayland's axis), which moves to the lower id.
    let Some(target) = scroll_target(&snapshot, dy > 0.0) else {
        return;
    };
    if let Some(dir) = crate::shared::services::hyprland::socket_dir() {
        crate::shared::services::hyprland::focus_workspace(&dir, target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ws(id: i32, windows: u32, monitor: &str) -> Workspace {
        Workspace {
            id,
            name: id.to_string(),
            windows,
            monitor: monitor.to_string(),
            clients: Vec::new(),
        }
    }

    fn snapshot(workspaces: Vec<Workspace>, active: i32) -> Snapshot {
        Snapshot {
            workspaces,
            active,
            focused_monitor: "eDP-1".to_string(),
        }
    }

    #[test]
    fn shown_zero_draws_exactly_what_exists() {
        let snap = snapshot(vec![ws(1, 1, "eDP-1"), ws(4, 0, "eDP-1")], 1);
        let pills = pills(&snap, &WorkspacesConfig::default(), None);
        assert_eq!(pills.len(), 2);
        assert_eq!(pills[0].id, 1);
        assert_eq!(pills[1].id, 4, "sparse ids are shown as they are");
    }

    #[test]
    fn a_fixed_window_keeps_the_bar_width_stable() {
        let config = WorkspacesConfig {
            shown: 5,
            ..WorkspacesConfig::default()
        };
        let one = pills(&snapshot(vec![ws(1, 1, "eDP-1")], 1), &config, None);
        let many = pills(
            &snapshot(vec![ws(1, 1, "eDP-1"), ws(2, 1, "eDP-1"), ws(3, 0, "eDP-1")], 1),
            &config,
            None,
        );
        assert_eq!(one.len(), 5, "empty slots hold the width");
        assert_eq!(many.len(), 5, "and it does not grow as workspaces appear");
        assert!(!one[3].occupied, "a placeholder is not occupied");
    }

    #[test]
    fn the_window_slides_to_keep_the_active_workspace_visible() {
        let config = WorkspacesConfig {
            shown: 5,
            ..WorkspacesConfig::default()
        };
        let workspaces: Vec<Workspace> = (1..=20).map(|id| ws(id, 1, "eDP-1")).collect();

        let early = pills(&snapshot(workspaces.clone(), 2), &config, None);
        assert_eq!(early.first().unwrap().id, 1);
        assert!(early.iter().any(|p| p.active && p.id == 2));

        // On workspace 12 a fixed 1–5 run would strand the user off the end.
        let late = pills(&snapshot(workspaces, 12), &config, None);
        assert!(
            late.iter().any(|p| p.active && p.id == 12),
            "the active workspace is always inside the window: {:?}",
            late.iter().map(|p| p.id).collect::<Vec<_>>()
        );
        assert_eq!(late.len(), 5);
    }

    #[test]
    fn per_monitor_filters_to_this_bars_output() {
        let config = WorkspacesConfig {
            per_monitor: true,
            ..WorkspacesConfig::default()
        };
        let snap = snapshot(
            vec![ws(1, 1, "eDP-1"), ws(2, 1, "DP-2"), ws(3, 1, "eDP-1")],
            1,
        );
        let mine = pills(&snap, &config, Some("eDP-1"));
        assert_eq!(
            mine.iter().map(|p| p.id).collect::<Vec<_>>(),
            vec![1, 3],
            "the other monitor's workspaces stay on the other monitor"
        );
        let all = pills(&snap, &WorkspacesConfig::default(), Some("eDP-1"));
        assert_eq!(all.len(), 3, "without per_monitor every bar shows every one");
    }

    #[test]
    fn special_workspaces_appear_only_when_occupied_and_can_carry_an_icon() {
        let mut special = ws(-99, 1, "eDP-1");
        special.name = "special:magic".to_string();
        let mut empty_special = ws(-98, 0, "eDP-1");
        empty_special.name = "special:unused".to_string();

        let mut config = WorkspacesConfig::default();
        config
            .special_icons
            .insert("magic".to_string(), "sparkles".to_string());

        let snap = snapshot(vec![ws(1, 1, "eDP-1"), special, empty_special], 1);
        let drawn = pills(&snap, &config, None);
        assert_eq!(drawn.len(), 2, "the empty scratchpad is not drawn");
        let magic = drawn.last().unwrap();
        assert!(magic.special);
        assert_eq!(magic.label, "magic", "the `special:` prefix is stripped");
        assert_eq!(magic.icon.as_deref(), Some("sparkles"));

        config.show_special = false;
        assert_eq!(pills(&snap, &config, None).len(), 1);
    }

    #[test]
    fn labels_render_from_the_template() {
        let config = WorkspacesConfig {
            label: "[{index}]".to_string(),
            ..WorkspacesConfig::default()
        };
        let snap = snapshot(vec![ws(7, 1, "eDP-1"), ws(9, 1, "eDP-1")], 7);
        let drawn = pills(&snap, &config, None);
        assert_eq!(drawn[0].label, "[1]", "index is the position, not the id");
        assert_eq!(drawn[1].label, "[2]");

        let by_id = WorkspacesConfig::default();
        assert_eq!(pills(&snap, &by_id, None)[0].label, "7");
    }

    #[test]
    fn window_icons_are_capped_and_off_by_default() {
        let mut w = ws(1, 6, "eDP-1");
        w.clients = vec!["a", "b", "c", "d", "e", "f"]
            .into_iter()
            .map(String::from)
            .collect();
        let snap = snapshot(vec![w], 1);

        assert!(
            pills(&snap, &WorkspacesConfig::default(), None)[0]
                .clients
                .is_empty(),
            "icons cost an icon lookup per window, so they are opt-in"
        );

        let config = WorkspacesConfig {
            window_icons: true,
            max_window_icons: 3,
            ..WorkspacesConfig::default()
        };
        assert_eq!(pills(&snap, &config, None)[0].clients.len(), 3);
    }

    #[test]
    fn scroll_moves_one_workspace_and_stops_at_the_ends() {
        let snap = snapshot(
            vec![ws(1, 1, "eDP-1"), ws(2, 1, "eDP-1"), ws(3, 1, "eDP-1")],
            2,
        );
        assert_eq!(scroll_target(&snap, true), Some(1));
        assert_eq!(scroll_target(&snap, false), Some(3));

        let first = snapshot(vec![ws(1, 1, "eDP-1"), ws(2, 1, "eDP-1")], 1);
        assert_eq!(
            scroll_target(&first, true),
            None,
            "nowhere to go means no dispatch at all"
        );
        let last = snapshot(vec![ws(1, 1, "eDP-1"), ws(2, 1, "eDP-1")], 2);
        assert_eq!(scroll_target(&last, false), None);
    }

    #[test]
    fn scroll_skips_special_workspaces() {
        let mut special = ws(-99, 1, "eDP-1");
        special.name = "special:magic".to_string();
        let snap = snapshot(vec![ws(1, 1, "eDP-1"), special, ws(2, 1, "eDP-1")], 1);
        assert_eq!(
            scroll_target(&snap, false),
            Some(2),
            "a scratchpad is not somewhere the wheel should land"
        );
    }
}
