---
id: notifications
kind: module
title: Notification bell
summary: The unread count, and the history drawer behind it.
status: stable
compositor: any
config: [notifications]
commands: [notifs]
deps: []
panel: true
see_also: [notifications-daemon, notification-centre]
---

# Notification bell

The chip and its drawer. The *daemon* — what receives notifications, groups them, pops them and stores them —
is on its own page: **[Notification daemon](../system/notifications-daemon.md)**.

## What it shows

A bell with the unread count, and do-not-disturb state when it is on.

## Interacting

| Gesture | What happens |
| --- | --- |
| Click | opens the history drawer |

```sh
hyprshell panel toggle notifications   # the drawer
hyprshell notifs center toggle         # the full-height centre instead
hyprshell notifs dnd toggle
hyprshell notifs clear [app]
```

The drawer is a **glance**: it hangs off its chip, it is as tall as its content, and it closes when you look
away. The [notification centre](../surfaces/notification-centre.md) is the other thing — it takes the whole
edge, it scrolls, and it is where you work through a morning's notifications.

## Configuring

`[notifications]` covers both the popups and this drawer — see
[Notification daemon](../system/notifications-daemon.md) for what each key means.

## What it needs

Nothing. hyprshell **is** the notification daemon; it owns `org.freedesktop.Notifications` itself.

## Related

- [Notification daemon](../system/notifications-daemon.md) — grouping, DND, per-app mute, actions, sound.
- [Notification centre](../surfaces/notification-centre.md) — the full-height surface.
