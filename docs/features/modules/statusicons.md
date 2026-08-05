---
id: statusicons
kind: module
title: Status icons
summary: Several service icons sharing one chip.
status: stable
compositor: any
config: [status_icons]
commands: []
deps: [pw-dump, networkmanager, bluez, upower, leds]
see_also: [volume, network, bluetooth, battery, lockstatus]
---

# Status icons

A chip per reading is fine for two or three and wasteful for eight — each one carrying its own padding,
background and hover target. This draws the same glyphs, from the same source the standalone chips use, inside
a single chip.

## What it shows

Whatever you list, in the order you list it. The available ids:

| Id | Reading |
| --- | --- |
| `volume` | output level and mute |
| `mic` | input level and mute |
| `network` | online, and over what |
| `wifi` | the wireless link specifically |
| `bluetooth` | adapter and connections |
| `battery` | charge and charging state |
| `caps` | Caps Lock |
| `num` | Num Lock |

A cluster icon and its own chip are **the same thing under the same name**, so moving a reading between the two
never means renaming it. `wifi` is the one cluster-only id: the `network` chip already covers being online over
any link.

## Interacting

Nothing. The cluster has no click, because a press would have to pick one of several readings to act on. Each
reading keeps its standalone module for that.

## Configuring

`[status_icons]` — `icons` (the list, in your order) and `spacing`.

Order is config rather than a fixed list because the order icons sit in is the whole point of a cluster: a
reader wants their own priority, not the shell's.

## What it needs

Whatever the readings you listed need — `pw-dump` for audio, NetworkManager for Wi-Fi detail, BlueZ for
Bluetooth, UPower or sysfs for battery, `/sys/class/leds` for the lock keys.

An icon whose source is missing behaves exactly as its standalone chip does: hidden, or reporting unknown.

## Related

The standalone versions: [volume](volume.md), [mic](mic.md), [network](network.md), [bluetooth](bluetooth.md),
[battery](battery.md), [lockstatus](lockstatus.md).
