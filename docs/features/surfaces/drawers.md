---
id: drawers
kind: surface
title: Drawers
summary: A panel anchored to the chip that opened it.
status: stable
compositor: any
config: [panels]
commands: [panel]
deps: [wlr-layer-shell]
see_also: [panels, floats, bars]
---

# Drawers

## What it is

The default presentation for a panel: a surface that hangs off the chip you clicked, on the same edge as its
bar, sized to its content up to a limit.

A drawer derives its cross-axis alignment from the **zone** its chip sits in — a chip in `start` opens a drawer
aligned to the start, one in `end` aligns to the end — so a drawer never opens off the side of the screen.

## Configuring

`[panels.drawer]` — `width`, `max_height`.
`[panels]` — `gap` (the distance from the bar), `opacity`.

`max_height` is a maximum, not a height: a drawer with two rows in it is two rows tall.

## When to use a float instead

A drawer closes when you look away and cannot be moved. If you want a panel to stay put while you work in the
window behind it — the mixer while you balance two applications, window info while you compare two windows —
set `[modules.<id>] open = "float"`. See [Floats](floats.md).

## What it needs

`wlr-layer-shell`.

## Related

- [Panels](panels.md) — the shared behaviour, and the list of what has one.
