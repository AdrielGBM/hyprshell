---
id: osd
kind: surface
title: On-screen display
summary: The overlay that shows a level while you are changing it.
status: partial
compositor: any
config: [osd]
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

`[osd]` — `edge`, `align`, `timeout_ms`.

## Showing one without changing anything

Clicking the [brightness](../modules/brightness.md) chip shows the OSD without moving the level, which is the
gesture for "what is it at". There is **no IPC command** that does the same thing — `volume osd` / `mic osd` /
`brightness osd` do not exist yet, so a keybind that reports a level without changing it is not available.

## What it needs

`wlr-layer-shell`. The surface exists only while an OSD is up.

## Related

- [Toasts](toasts.md) — the text equivalent, for everything that is not a level.
