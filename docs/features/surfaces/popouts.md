---
id: popouts
kind: surface
title: Popouts
summary: The readout a chip shows while the pointer rests on it.
status: stable
compositor: any
config: [popouts]
commands: []
deps: [wlr-layer-shell]
see_also: [panels, statusicons]
---

# Popouts

Distinct from the drawer a click opens, and the shell's primary status interaction: a bar chip has room for one
glyph, and everything that glyph stands for — the level behind it, the sensor it came from, the whole window
title it truncated — lives here.

## Which chips have one

`volume` `mic` `brightness` `battery` `network` `bluetooth` `kblayout` `lockstatus` `activewindow` `media`
`cpu` `gpu` `memory` `temperature` `netspeed`

A chip with no card is never given a hover target, so nothing ever opens empty.

## Three things make it a popout rather than a flicker

- **Delays.** The pointer has to rest on a chip before anything opens, and the card survives long enough after
  you leave for the pointer to reach it.
- **One surface.** Moving from chip to chip replaces the card rather than stacking a second one.
- **A carved input region.** The surface is sized to the tallest card a popout may be, and everything the card
  does not cover is click-through — so an invisible rectangle never eats a click meant for the window behind.

## Live, not a snapshot

Every card subscribes to the service it reads, so it follows the value while it is up. Hovering the volume chip
and scrolling it is one gesture, and a card frozen at the level it opened with would be worse than no card.

Nothing polls: each subscription is bound to the popout surface and dies with it.

## Configuring

`[popouts]` — `enabled`, `open_delay`, `close_delay`, `width`, `max_height`.

## What it needs

`wlr-layer-shell`, plus whatever the reading behind the card needs.

## Related

- [Panels](panels.md) — what a click opens instead.
