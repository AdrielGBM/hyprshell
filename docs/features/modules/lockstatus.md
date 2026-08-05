---
id: lockstatus
kind: module
title: Lock keys
summary: Caps Lock and Num Lock indicators.
status: stable
compositor: any
config: [lock_status, toasts]
commands: []
deps: [leds]
popout: true
see_also: [kblayout, toasts]
---

# Lock keys

## What it shows

Caps Lock and Num Lock. With `hide_inactive` the row shows only what is *on*, which means it is usually empty
— so the module draws its own row rather than sitting in a chip shell, and a bar does not carry a padded gap
where nothing is shown.

## Interacting

Nothing. It is a readout.

## Configuring

`[lock_status]` — `caps`, `num`, `hide_inactive`.

`[toasts.events] lock_keys` raises a [toast](../surfaces/toasts.md) when a lock key changes, which is what
makes the feature useful without a bar in view.

## What it needs

`/sys/class/leds`. This is **the one service in the shell with no event source to subscribe to**, so it polls
— two small sysfs reads on a thread that only starts if something is showing the module.

The reason is a real Wayland limit rather than an oversight: `wl_keyboard.modifiers` reaches only the surface
holding keyboard focus, and a bar deliberately does not take focus. Hyprland's event stream carries no lock
state either. The protocol that would answer it properly, `org_kde_kwin_keystate`, is KWin-only.

## Related

- [statusicons](statusicons.md) — `caps` and `num` are also available as glyphs in the shared cluster.
