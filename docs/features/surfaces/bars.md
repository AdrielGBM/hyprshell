---
id: bars
kind: surface
title: Bars
summary: One per screen edge, all four at once if you like, on every monitor.
status: stable
compositor: any
config: [bars, shape, corners, general]
commands: [shell]
deps: [wlr-layer-shell]
see_also: [panels, per-monitor, shape]
---

# Bars

## What it is

A layer surface anchored to a screen edge, carrying modules in three zones. There is one per edge — top,
bottom, left, right — and you can have all four at once, on every monitor.

An empty bar collapses to nothing, which is why the default config is all-empty: you get only the bars you
describe.

## Zones

```toml
[bars.top]
start  = ["workspaces", "spacer", "activewindow"]
center = ["clock"]
end    = ["statusicons", "tray", "battery"]
```

Three anchor points. [spacer](../modules/spacer.md) is what buys every arrangement in between.

A module entry is a bare id in the common case, and a table when one instance needs settings of its own:

```toml
center = [{ id = "clock", accent = "red" }]
```

The table form is what lets the same module appear on a bar twice looking different — a `[modules.<id>]`
override is keyed by id, so it applies to every copy at once.

## Shapes

`[shape] mode` decides what the bar looks like, and every module works in all three:

| Mode | What it is |
| --- | --- |
| `bar` | one solid strip, edge to edge |
| `sections` | the three zones as separate plates |
| `chips` | a plate per module |

`[shape] gap` floats the bar off the edge; `frame` draws a ring around the screen; `inactive_size` shrinks
what is not focused. `[corners]` rounds the screen's own corners against it.

Each bar can override the global shape for itself — `[bars.<edge>.shape]` takes `mode`, `gap`, `spacing` and
`radius`, all unset by default, so a top bar can be one solid strip while a left bar is chips.

## Auto-hide

`[bars.<edge>] persistent = false` gives a bar that is only on screen when it is wanted.

It is not hidden by drawing it somewhere else — it is **moved**. The layer surface sits at a negative margin
on its own anchored edge, far enough off that only `peek` logical pixels remain, and reveals itself by
animating that margin back. Two things follow: the bar takes no input over the strip it is not occupying,
because it is genuinely not there, and the peek strip is the bar's own edge rather than a second surface to
keep in step.

`show_on_hover` is what triggers the reveal.

**Known limit:** it hides on pointer-leave unconditionally, including while a panel it opened is still up.
Hiding only when a window would actually cover it needs `cosmic_overlap_notify_v1`, which is COSMIC-only today.

## Per monitor

`[bars] excluded_screens` names outputs that get no bars at all, matched as `*` patterns against the connector
name — so `HDMI-*` covers a port whose index moves between reboots.

*Which* modules a screen shows is a per-monitor override rather than a key here. See
[Per-monitor setup](../../guides/per-monitor.md).

## What it needs

`wlr-layer-shell` — the one hard requirement of the whole shell.

## Known limit

The toolkit binds layer-shell at versions 1–4, so v5's `set_exclusive_edge` is out of reach: a bar anchored to
more than one edge cannot say which edge its exclusive zone belongs to, and the compositor decides.

## Related

- [Panels](panels.md) — what a chip opens.
- [Modules](../modules/) — what goes in the zones.
