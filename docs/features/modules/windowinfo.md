---
id: windowinfo
kind: module
title: Window info
summary: What the compositor knows about the focused window, and four things to do to it.
status: stable
compositor: hyprland
config: [utilities]
commands: [shell]
deps: [ext-image-copy-capture, wlr-screencopy]
panel: true
see_also: [activewindow, screenshot]
---

# Window info

The details were already in the client list. What this panel adds is the preview and the actions.

## What it shows

The focused window's class, title, workspace, geometry and floating state — plus a **live preview** of the
window's own rectangle.

The preview is a real screen capture, taken on a producer thread on a period from `[utilities]
window_preview_ms`. A capture per refresh is not something to do on the frame, and a preview that re-read the
screen every frame would cost more than the panel drawing it.

## Interacting

| Gesture | What happens |
| --- | --- |
| Click on the chip | opens the panel |

Inside: close, fullscreen, float and pin.

## Configuring

`[utilities] window_preview_ms` — how often the preview refreshes. Higher is cheaper.

## What it needs

**Hyprland's IPC**, for both the details and the actions. The actions are Hyprland dispatchers.

**A capture protocol** for the preview: `ext-image-copy-capture` where the compositor has it,
`wlr-screencopy` where it does not. Without either the panel still lists the details and shows no picture.

## Why the actions are verified rather than trusted

`hl.dsp.window.<action>` will not say what arguments it takes — called outside a dispatch it refuses to build
at all — so the service tries a shape and checks whether the compositor's own client list moved.

**Closing is the exception: it gets one attempt.** Trying a second spelling of a close is how the wrong window
gets closed twice.

## Known limit

The preview captures the *output* and crops, because `ext-image-copy-capture` will only capture a toplevel
named by an `ext_foreign_toplevel_handle_v1`, and the protocol that hands those out is not bound yet.

## Related

- [activewindow](activewindow.md) — the title on a bar.
- [Screenshot](../system/screenshot.md) — the same capture pipeline.
