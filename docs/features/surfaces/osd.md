---
id: osd
kind: surface
title: On-screen display
summary: The overlay that shows a level while you are changing it.
status: partial
compositor: any
config: [stack]
commands: [volume, mic, brightness]
deps: [wlr-layer-shell]
see_also: [toasts, volume, brightness]
---

# On-screen display

## What it is

The overlay that appears when you change volume, microphone level or brightness — from a keybind, from a chip
click, or from a scroll on a chip.

Three kinds, and only three: `volume`, `mic` and `brightness`. Other state changes raise a
[toast](toasts.md) instead, which is a defensible different answer rather than an omission — a toast carries
text, an OSD carries a bar.

## Configuring

An OSD is a card in the shell's one column, so where it appears and how long it stays are the column's:
`[stack]` — `edge`, `align`, `width`, `max_visible`, `timeout_ms`. See [Toasts](toasts.md) for the column itself.

## Showing one without changing anything

Clicking the [brightness](../modules/brightness.md) chip shows the OSD without moving the level, which is the
gesture for "what is it at". There is **no IPC command** that does the same thing — `volume osd` / `mic osd` /
`brightness osd` do not exist yet, so a keybind that reports a level without changing it is not available.

## What it needs

`wlr-layer-shell`. The surface exists only while an OSD is up.

## Related

- [Toasts](toasts.md) — the text equivalent, for everything that is not a level.
