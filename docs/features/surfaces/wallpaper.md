---
id: wallpaper
kind: surface
title: Wallpaper layer
summary: The background image, its transition, and the clock and visualiser drawn on it.
status: stable
compositor: any
config: [background, wallpaper, paths, visualiser]
commands: [wallpaper]
deps: [wlr-layer-shell, pw-cat]
see_also: [dynamic-scheme, launcher, clock]
---

# Wallpaper layer

One surface per monitor, at the bottom of the background layer. It paints the image this screen should show,
cover-cropped over the theme's base colour — and, when you ask for them, a clock and an audio visualiser on
top.

## Choosing an image

```sh
hyprshell wallpaper set ~/pictures/x.jpg    # every screen
hyprshell wallpaper set ~/pictures/x.jpg DP-2
hyprshell wallpaper random [output]
hyprshell wallpaper clear [output]          # back to what your config says
hyprshell wallpaper list
```

The launcher's `@` mode is the same library as a grid.

## Which image a screen shows

Resolution order, most specific first:

1. the runtime per-output choice,
2. the runtime global one,
3. `[background.monitors]`,
4. `[background] image`.

An image you pinned in your config keeps showing until something sets one at runtime, and `wallpaper clear`
puts you back. The runtime choice lives in `state.json`, not in `config.toml`: a wallpaper picked at random is
state the shell owns, not a preference you hand-edited.

## The transition

A wallpaper change is an **event on the live surface**, not a rebuild — a fresh tree has nothing left of the
old image to fade *from*. `[background] transition` and `transition_ms` control the cross-fade.

## The library

`[paths] wallpapers` is scanned recursively, with a thumbnail cache so a grid of two hundred images does not
decode two hundred full-resolution photographs.

`[wallpaper]` — `enabled`, `recursive`, `extensions`, `max_entries`, `thumbnail_size`.

## Desktop clock

`[background.clock]` draws a clock face on the wallpaper: `position`, `scale`, `format`, `date_format`,
`show_date`, `invert`, `margin`, `shadow`, `background`, `background_opacity`, `background_blur`.

**`background_blur` feathers the plate's own edge — it does not sample what is behind it.** No client-side
renderer can; asking the compositor is the route, through `ext-background-effect-v1` or a Hyprland
`layer_rule = blur, <namespace>`, and neither is wired up yet.

## Visualiser

<a id="visualiser"></a>

`[background.visualiser]` draws the sound coming out of your speakers along an edge of the wallpaper:
`enabled`, `edge`, `reach`, `gap`, `margin`, `radius`, `opacity`, `accent`, `hide_when_silent`.

`[visualiser]` tunes the analysis itself — `bars`, `frame_rate`, `gain`, `smoothing`, `floor_db`,
`beat_sensitivity`.

It needs **`pw-cat`**, recording the default sink's *monitor* — what is being played, not what a microphone
hears. Without it the bars stay silent.

This is the only service in the shell that publishes at a frame rate, and two things keep it from undoing an
idle desktop: nothing starts until something subscribes, and a frame identical to the one before it is not
published — so silence costs one final all-zero frame and then nothing at all. `hide_when_silent` is a reading
of that, not a timer.

## What it needs

`wlr-layer-shell`. `pw-cat` only for the visualiser.

## Related

- [Dynamic scheme](../theming/dynamic-scheme.md) — deriving the palette from whatever is showing here.
