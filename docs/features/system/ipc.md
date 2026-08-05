---
id: ipc
kind: system
title: IPC
summary: Every action the shell has is a command on a socket.
status: stable
compositor: any
config: []
commands: [shell]
deps: []
see_also: [scripting, keybinds, global-shortcuts]
---

# IPC

## What it is

Started with no arguments, `hyprshell` runs the shell. With arguments it is a **client**: the request goes to
the running shell over a Unix socket and the reply is printed.

That is why a keybind is just a shell command, and why anything the UI does is scriptable.

```sh
hyprshell ping
hyprshell --list        # every target, command and argument
```

## The socket

`$XDG_RUNTIME_DIR/hyprshell/<instance>.sock`, one per compositor instance. `HYPRLAND_INSTANCE_SIGNATURE` names
it, so two compositors on one login session get one socket each; outside Hyprland the name is still stable and
the client still finds the shell.

## The reply format

Every reply starts with `ok` or `err`, and the payload follows on the same line when there is one. A caller can
branch on the outcome **without parsing prose**, and the exit status mirrors the reply.

```sh
hyprshell lock status        # ok ...
hyprshell wifi connect x     # err ...
```

## Three commands are answered locally

`config schema`, `deps` and `man` are answered by the binary rather than sent to the shell: each is a function
of this build and this machine rather than of a running shell. `deps` is the case that matters — a dependency
report is for the machine where something is missing, and "nothing starts" is exactly when there is no shell to
ask.

## One table

The same table backs `--list`, `hyprshell(1)` and what actually dispatches, so they cannot drift from one
another. Looking a command up and running it are separate steps, which is what lets the shell check whether a
request line is valid — for a global shortcut, or for an `[idle]` stage — **without performing it**.

## What it needs

Nothing.

## Related

- [Scripting](../../guides/scripting.md) — patterns for using this from a script.
- [Global shortcuts](global-shortcuts.md) — the same lines, delivered by the portal.
