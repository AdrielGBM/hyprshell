---
id: launcher
kind: surface
title: Launcher
summary: A full-screen modal that owns the keyboard: applications, actions, a calculator, schemes and wallpapers.
status: stable
compositor: any
config: [launcher, general]
commands: [launcher, apps]
deps: [wlr-layer-shell, qalc, setsid]
see_also: [apps, palettes, wallpaper]
---

# Launcher

## What it is

A modal surface that takes the keyboard exclusively while it is up. Type to filter; press Enter to run.

```sh
hyprshell launcher toggle
hyprshell launcher close
```

## Five modes, chosen by a prefix

| Prefix | Mode | What it lists |
| --- | --- | --- |
| *(none)* | Applications | installed `.desktop` entries |
| `>` | Actions | the actions you declared in `[launcher] actions` |
| `=` | Calculator | the result of an expression |
| `#` | Schemes | the palettes `scheme set` accepts |
| `@` | Wallpapers | the images in your library, as a grid |

The prefix is stripped and the rest is the query, so `>reboot` and `> reboot` are the same thing.

## Ranking

Applications are ranked by **how often you have launched them**, which is state the shell keeps in
`state.json`. `[launcher] favourites` pins entries above that; `hidden` removes them.

`fuzzy` switches matching between fuzzy and substring. It is one global switch — there is no per-mode setting.

```sh
hyprshell apps search firefox    # the same ranking, over IPC
hyprshell apps reload            # re-scan, though a watcher usually does it for you
```

Applications are scanned once and cached for the process — a few hundred entries, parsed in a few milliseconds
— and a watcher notices an install, so new software appears without a reload.

## The calculator

A built-in evaluator handles ordinary arithmetic with no dependency. **`qalc`** adds currencies, constants and
dates; without it those queries simply do not resolve. `[launcher] calculator` and `qalc` switch each half.

## Actions

`[launcher] actions` is a list of things to run — the extension surface of the shell, alongside
`[theme.export] hooks`. `enable_dangerous_actions` gates the ones that can end a session.

## Configuring

`[launcher]` — `width`, `height`, `radius`, `max_results`, `fuzzy`, `calculator`, `qalc`, `actions`,
`favourites`, `hidden`, `enable_dangerous_actions`, plus `[launcher.icons]`.

## What it needs

Nothing to open. **`setsid`** is what detaches a launched application so it outlives the shell — without it,
what you launch from the launcher dies with the shell. **`qalc`** is optional, as above.

## Known limits

No emoji picker, no clipboard history, no open-window switcher, no web-search mode and no file search. Each is
a mode over machinery that already exists, and none is built yet.

## Related

- [Palettes](../theming/palettes.md) — what `#` lists.
- [Wallpaper](wallpaper.md) — what `@` lists.
