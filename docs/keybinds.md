# Keybinds

Every action the shell has is an IPC command, so a keybind is a bind to a shell command. `hyprshell --list` is
the complete, authoritative menu — this page is a starting set, not a second copy of it.

## The commands worth binding

| Command | What it does |
| --- | --- |
| `hyprshell launcher toggle` | the application launcher |
| `hyprshell dashboard toggle` | the dashboard |
| `hyprshell panel toggle notifications` | notification history |
| `hyprshell notifs center toggle` | the full-height notification centre |
| `hyprshell panel toggle session` | the session menu |
| `hyprshell panel toggle settings` | the settings application |
| `hyprshell panel toggle utilities` | the utilities panel |
| `hyprshell lock on` | lock the session |
| `hyprshell notifs dnd toggle` | do-not-disturb |
| `hyprshell screenshot region` | pick a region and capture it |
| `hyprshell screenshot screen` | every monitor, composed into one image |
| `hyprshell record toggle` | start or stop a screen recording |
| `hyprshell volume up` / `down` / `mute` | |
| `hyprshell mic mute` | |
| `hyprshell brightness up` / `down` | |
| `hyprshell media play-pause` / `next` / `previous` | |
| `hyprshell wallpaper random` | |

Two things that are not obvious from the names:

**Where a screenshot goes is config, not a flag.** `[screenshot] copy` and `save` decide whether a capture
reaches the clipboard, a file, or both — so one bind behaves the way you set it up rather than needing a
different bind per destination.

**`brightness up` with no display named means the primary panel, not every screen.** It is the one mutation
where an unnamed target is not "all of them", because it is overwhelmingly a laptop's function key. Name a
connector (`brightness up DP-2`) or spell out `all` for the rest. Every other mutation — `wallpaper set`,
`wallpaper clear` — does mean all of them when nothing is named.

## Binding them

Hyprland's `.conf` format is deprecated in favour of Lua, but the *command* half is identical either way, and
that half is the part this page can promise. Check the binder's own syntax against your Hyprland version — the
option table (key repeat, release, locked) has moved between releases.

```lua
-- ~/.config/hypr/hyprland.lua
local function sh(cmd)
  return function() hl.dsp.exec_cmd("hyprshell " .. cmd) end
end

hl.bind({ "SUPER" }, "space",  sh("launcher toggle"))
hl.bind({ "SUPER" }, "d",      sh("dashboard toggle"))
hl.bind({ "SUPER" }, "n",      sh("panel toggle notifications"))
hl.bind({ "SUPER" }, "comma",  sh("panel toggle settings"))
hl.bind({ "SUPER" }, "escape", sh("panel toggle session"))
hl.bind({ "SUPER" }, "l",      sh("lock on"))

hl.bind({ "SUPER", "SHIFT" }, "s", sh("screenshot region"))
hl.bind({ "SUPER", "SHIFT" }, "r", sh("record toggle"))
hl.bind({ "SUPER", "SHIFT" }, "w", sh("wallpaper random"))

hl.bind({}, "XF86AudioRaiseVolume",  sh("volume up"))
hl.bind({}, "XF86AudioLowerVolume",  sh("volume down"))
hl.bind({}, "XF86AudioMute",         sh("volume mute"))
hl.bind({}, "XF86AudioMicMute",      sh("mic mute"))
hl.bind({}, "XF86MonBrightnessUp",   sh("brightness up"))
hl.bind({}, "XF86MonBrightnessDown", sh("brightness down"))
hl.bind({}, "XF86AudioPlay",         sh("media play-pause"))
hl.bind({}, "XF86AudioNext",         sh("media next"))
hl.bind({}, "XF86AudioPrev",         sh("media previous"))
```

The deprecated equivalent, for reference:

```ini
bind = SUPER, space, exec, hyprshell launcher toggle
binde = , XF86AudioRaiseVolume, exec, hyprshell volume up
```

## The portal route

The shell also registers its most-bound actions with `xdg-desktop-portal`. The portal registers **actions**,
never keys — it has no way to ask for a particular one — so you still write the bind. What it buys you is that
the compositor's own settings UI can list them by description.

```sh
hyprctl globalshortcuts   # what is registered, and under what name
```

The name is `<appid>:<id>`, and on a non-sandboxed install the app id is empty — so it is `:launcher`, not
`hyprshell:launcher`.

Registered ids: `launcher` `dashboard` `notifications` `session` `dnd` `volume-up` `volume-down` `volume-mute`
`mic-mute` `brightness-up` `brightness-down`.

That list is deliberately shorter than the IPC table. `hyprshell audio set 40` is a scripting command, not a
shortcut, and registering every command would bury the ten anyone binds.

Binding the commands directly covers everything this route does and more, so use the portal only if you want
the actions to appear in the compositor's own UI.
