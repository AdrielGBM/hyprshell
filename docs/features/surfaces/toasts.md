---
id: toasts
kind: surface
title: Toasts
summary: The small, self-dismissing messages the shell says about itself.
status: stable
compositor: any
config: [toasts]
commands: [toast]
deps: [wlr-layer-shell]
see_also: [osd, notifications-daemon]
---

# Toasts

## What it is

Feedback about something you just did: Caps Lock came on, a screenshot was taken, the config reloaded, a VPN
came up.

**Deliberately not freedesktop notifications.** A notification is a record — it belongs to an application, it
goes into history, it can be acted on, and under do-not-disturb it is *kept* rather than shown. "Caps Lock is
on" is none of those: it is feedback about a key you just pressed, it is worthless a second later, and filing
it in the notification history would be filing your own keystrokes.

So toasts have their own queue, their own surface and their own switches, and nothing here reaches the daemon.

## Which events raise one

`[toasts.events]` — one switch each:

`audio_input` `audio_output` `charging` `config_loaded` `dnd` `game_mode` `kb_layout` `lock_keys`
`now_playing` `recording` `screenshot` `vpn`

## From a script

```sh
hyprshell toast show "backup finished"
hyprshell toast clear
```

Which makes the shell's own feedback channel available to anything you write.

## Where it appears

On whichever monitor the compositor reports as focused **at the moment the toast is posted** — for feedback
about a keypress, that is the screen you are looking at.

The surface is opened on the first toast and closed with the last, so an idle session carries no overlay at
all. Expiry runs on a thread of its own, because toasts are posted from wherever the event happened and the one
thing they cannot rely on is a surface being up to time them out.

## Configuring

`[toasts]` — `enabled`, `edge`, `align`, `gap`, `width`, `max_toasts`, `timeout_ms`, plus `[toasts.events]`.

## Known limit

Turning a `[toasts.events]` switch off at runtime stops the toast, but leaves the watcher — and therefore the
service behind it — running. Restart the shell to actually quiet it.

## What it needs

`wlr-layer-shell`.

## Related

- [OSD](osd.md) — for levels rather than text.
- [Notification daemon](../system/notifications-daemon.md) — for messages that belong to an application.
