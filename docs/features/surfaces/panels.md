---
id: panels
kind: surface
title: Panels
summary: What a chip opens — as a drawer hanging off it, or as a movable float.
status: stable
compositor: any
config: [panels, modules]
commands: [panel]
deps: [wlr-layer-shell]
see_also: [drawers, floats, popouts]
---

# Panels

## What it is

The surface behind a module. Thirteen modules have one: clock, dashboard, battery, bluetooth, network, mixer,
notifications, notes, settings, utilities, windowinfo, session and logo.

A panel is reached three ways — a chip click, `hyprshell panel toggle <module>`, or a keybind — and all three
reach the **same** surface rather than stacking three copies of it.

```sh
hyprshell panel toggle settings
hyprshell panel open network
hyprshell panel list            # what is open right now
```

## Two presentations

`[modules.<id>] open` picks one per module:

| Value | What you get |
| --- | --- |
| `drawer` | anchored to the chip that opened it, sized to its content — see [Drawers](drawers.md) |
| `float` | a free-standing window you can move and resize — see [Floats](floats.md) |

The choice lives under `[modules.<id>]` rather than on the bar entry because a panel is toggled by module id
from three places and only one of them has a bar entry in hand. An entry-scoped answer would make the same
panel open differently depending on how you asked for it.

## Configuring

`[panels]` — `opacity`, `gap`, `drag_threshold`.
`[panels.drawer]` — `width`, `max_height`.
`[panels.float]` — `width`, `height`.

Per module, `[modules.<id>]` overrides `width` and `height` for that module's float, plus `variant` and
`accent` for how it is drawn.

## Keyboard

Most panels are display-only and take no keyboard focus, so the window behind them keeps it. Three take
focus because they have fields: **notes**, **settings** and **session**.

## What it needs

`wlr-layer-shell`.

## Lifecycle

A panel that has never been opened does not exist. What the *config* describes — bars, the wallpaper, the frame
— is reconciled on every reload; what the *user* opened is tracked separately, which is what keeps a reload
from closing what you had open.

## Related

- [Popouts](popouts.md) — the hover card, which is a different surface with different rules.
