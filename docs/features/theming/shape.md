---
id: shape
kind: theming
title: Shape and motion
summary: Bar shape, screen corners, and how much the shell animates.
status: stable
compositor: any
config: [shape, corners, animation, keynav, panels]
commands: []
deps: []
see_also: [bars, panels, palettes]
---

# Shape and motion

## Shape modes

`[shape] mode` — `bar`, `sections` or `chips`. One solid strip, the three zones as separate plates, or a plate
per module. Every module and every surface works in all three, on all four edges; that is a standing rule of
the project rather than a coincidence.

`[shape]` — `mode`, `gap`, `frame`, `inactive_size`, plus `spacing` and `radius`, which are unset by default so
they fall back to the theme's values.

`gap` defaults to 0 — an edge-to-edge bar. Floating is opt-in.

**Per bar**, `[bars.<edge>.shape]` overrides `mode`, `gap`, `spacing` and `radius` for one edge only. All four
are unset by default, so a bar follows `[shape]` until it says otherwise.

`frame` draws a ring around the screen, which is what makes a floating bar look intentional rather than
detached.

## Screen corners

`[corners]` — `top_left`, `top_right`, `bottom_left`, `bottom_right`. Each names a corner treatment
independently, so a bar on one edge can round into the screen without the opposite corners following.

## Motion

`[animation]` — `enabled`, `curve`, `easing`, `duration_scale`, `panel_duration_ms`.

`duration_scale` is the one to reach for: it scales every animation at once, and `0` switches motion off
without changing anything else.

## Keyboard navigation

`[keynav] vim` adds `hjkl` navigation to the surfaces that take keyboard focus.

**The keys themselves are fixed** — there is no keybind table for in-surface navigation, only this switch.

## What it needs

Nothing.

## Related

- [Bars](../surfaces/bars.md) — where the shape modes are visible.
- [Panels](../surfaces/panels.md) — a panel takes its gap from the bar's, and its opacity from `[theme]`.
