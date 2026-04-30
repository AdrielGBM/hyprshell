# Chip & Content Plugins

The framework auto-discovers `Chip.qml` files inside plugin folders and wires them into the bar. A `Content.qml` in the same folder becomes the drawer panel that opens when the chip is clicked. This document describes the contract both files must fulfill.

## How it works

1. `Framework.qml` scans each plugin folder for `Chip.qml` files using `FolderScanner`.
2. Every discovered component is registered in `ModuleRegistry` under the plugin's folder key.
3. `Bar.qml` renders the chips placed in `settings.json` slots (`left`, `center`, `right`, `top`, `bottom`).
4. After loading each chip, `Bar.qml` calls `ChipWiring.wire()`, which injects context properties into the item.
5. If the chip declares a `panelUrl` pointing to a `Content.qml`, clicking it opens a drawer panel via `DrawerState`.

---

## Chip.qml

A chip is a bar widget. The simplest implementation extends `ChipContainer`, which handles all layout, styling, interaction, and drawer wiring automatically.

### Minimal example

```qml
import QtQuick
import "../../../shared/components"

ChipContainer {
    id: myChip

    // Point to the companion drawer panel (optional)
    panelUrl: Qt.resolvedUrl("./Content.qml").toString()

    // Extra props forwarded to Content.qml (optional)
    // panelProps: ({ myProp: someValue })

    LucideIcon {
        name: "star"
        accent: myChip.iconAccent
        size: 24
    }
}
```

### ChipContainer interface

`ChipContainer` is the base type for chips. It provides automatic styling, interaction, and drawer wiring.

**Properties injected by the bar (do not set manually):**

| Property | Type | Description |
| --- | --- | --- |
| `drawerState` | `var` | Controls the drawer. Used internally to open the panel on click. |
| `overlayState` | `var` | Reference to the overlay system. Available for chips that need to push popups manually. |
| `moduleRegistry` | `var` | Access to all registered module components and states. |
| `barPosition` | `string` | `"top"`, `"bottom"`, `"left"`, or `"right"`. |
| `barIndex` | `int` | Positional index within the bar slot (used to anchor the drawer). |
| `barScreen` | `var` | The `Screen` object for the monitor this chip is on. |
| `chipRadius` | `int` | Radius inherited from the bar. Overrides `Theme.radius` for the chip background. |

**Properties you configure in the chip:**

| Property | Type | Default | Description |
| --- | --- | --- | --- |
| `panelUrl` | `string` | `""` | URL to `Content.qml`. When non-empty, the chip becomes clickable and opens a drawer. |
| `panelProps` | `object` | `{}` | Extra properties forwarded to `Content.qml` when the drawer opens. |
| `accent` | `string` | `""` | Theme color token (`"accent1"`, `"error"`, `"success"`, etc.). Falls back to `Theme.defaultAccent` then `"text"`. |
| `variant` | `string` | `"default"` | `"default"` (transparent background) or `"filled"` (solid accent background). |

**Computed properties available inside the chip:**

| Property | Type | Description |
| --- | --- | --- |
| `isVertical` | `bool` | `true` when `barPosition` is `"left"` or `"right"`. |
| `fgColor` | `color` | Foreground color: accent color for `"default"` variant, contrast color for `"filled"`. |
| `iconAccent` | `string` | Pass this to `LucideIcon`'s `accent` prop so the icon color matches the chip variant. |
| `accentColor` | `color` | Resolved accent color. |
| `hovered` | `bool` | `true` while the cursor is over the chip. |
| `pressed` | `bool` | `true` while the chip is being clicked. |
| `interactive` | `bool` | `true` when `panelUrl` is set. Controls cursor shape and hover highlight. |
| `r` | `int` | Effective corner radius (`chipRadius` if set, else `Theme.radius`). |
| `pad` | `int` | `Theme.spacing`. |

### Custom chips (without ChipContainer)

Some chips manage their own layout (e.g. `workspaces`). They still receive the same injected properties via `ChipWiring.wire()`, so declare any of the following that you need:

```qml
Item {
    id: myChip

    // Any subset of these will be filled in by the bar:
    property string barPosition: ""
    property var barScreen: null
    property var drawerState: null
    property var overlayState: null
    property var moduleRegistry: null
    property int barIndex: 0
    property int chipRadius: -1

    // Styling tokens, also set from settings.json slot config:
    property string accent: ""
    property string variant: "default"
}
```

### Accessing state from other modules

When a chip needs data from another plugin's `State.qml` (e.g. notifications count), inject `moduleRegistry` and look it up:

```qml
ChipContainer {
    property var moduleRegistry: null  // injected by bar

    readonly property var notifState: moduleRegistry?.states["notifications"] ?? null
    readonly property int unread: notifState?.unreadCount ?? 0
}
```

States are registered under their plugin folder key (e.g. `"notifications"` for `src/plugins/common/notifications/`).

---

## Content.qml

A `Content.qml` is a drawer panel. It is loaded on demand when the user clicks its companion chip.

### Interface

The framework passes these properties when opening the drawer:

| Property | Type | Description |
| --- | --- | --- |
| `drawerState` | `var` | The active `DrawerState`. Call `drawerState.closeDrawer()` to close programmatically. |
| *(extra)* | `var` | Any properties declared in the chip's `panelProps` object. |

The content fills the drawer panel area. Use `anchors.fill: parent` as the root anchor.

### Minimal example

```qml
import QtQuick
import qs.src.shared.theme

Item {
    id: myPanel

    property var drawerState: null  // injected by framework

    anchors.fill: parent

    Text {
        anchors.centerIn: parent
        text: "Hello from the drawer"
        color: Theme.text
    }
}
```

### Receiving extra props

Declare matching properties in `Content.qml` and populate them in the chip's `panelProps`:

**Chip.qml:**

```qml
ChipContainer {
    readonly property var myState: moduleRegistry?.states["myplugin"] ?? null

    panelUrl: Qt.resolvedUrl("./Content.qml").toString()
    panelProps: ({ myState: myChip.myState })
}
```

**Content.qml:**

```qml
Item {
    property var drawerState: null
    property var myState: null   // forwarded from panelProps
}
```

---

## Plugin layout

A standard plugin folder looks like this:

```
src/plugins/<category>/<plugin>/
    Chip.qml      ← bar widget (required for auto-discovery)
    Content.qml   ← drawer panel (optional)
    State.qml     ← shared reactive state (optional, see popup-plugins.md)
    Popup.qml     ← overlay popup (optional, see popup-plugins.md)
```

The folder key used for `ModuleRegistry` and `states` lookup is the plugin folder name relative to the nearest parent scanned by `FolderScanner`.

---

## Slot configuration

Chips are placed via `settings.json` under `framework.bars`:

```json
"bars": {
    "top": {
        "left":   ["workspaces"],
        "center": ["clock"],
        "right":  [
            "network",
            { "id": "volume", "accent": "accent2" },
            { "id": "battery", "variant": "filled", "accent": "success" },
            "notifications"
        ]
    }
}
```

Each slot entry is either:

- A plain string — the plugin folder key.
- An object `{ "id": "<key>", "accent": "<token>", "variant": "<variant>" }` — overrides styling.

The `accent` and `variant` values are injected into the chip by `chipWiring.js`.

---

## Built-in plugins

| Plugin | Chip | Content | State | Popup | Notes |
| --- | --- | --- | --- | --- | --- |
| `battery` | ✅ | ✅ | — | — | Uses UPower. Adaptive icon based on charge level and charging state. |
| `clock` | ✅ | ✅ | — | — | Chip shows `HH:MM DD/MM`. Content shows `HH:MM:SS` and full date. |
| `network` | ✅ | — | — | — | Polls `nmcli` every 5 s. Adaptive icon based on Wi-Fi signal strength. |
| `notifications` | ✅ | ✅ | ✅ | ✅ | Chip shows bell + unread badge. Content is a scrollable history with DND toggle and clear-all. |
| `volume` | ✅ | — | ✅ | ✅ | Uses Pipewire. Adaptive icon based on volume level and mute state. |
| `workspaces` | ✅ | — | — | — | Custom chip (not `ChipContainer`). 10-workspace grid for Hyprland. Per-monitor awareness. |
