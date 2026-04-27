# Popup Plugins

The framework auto-discovers `Popup.qml` files inside plugin folders and wires them into the overlay system. This document describes the contract a plugin must fulfill to show popups correctly.

## How it works

1. `Framework.qml` scans each plugin folder for `State.qml` and `Popup.qml`.
2. A `PopupWatcher` is created per plugin that has a `Popup.qml`.
3. The watcher observes `pluginState.activeList` (a `ListModel`). Each row represents one popup to show.
4. For every new row, the watcher pushes an entry into `OverlayState`, which renders it via `Overlay` / `PopupContainer`.
5. When the dismiss timer fires (or the user dismisses), the watcher calls `pluginState.removeActive(notif)` and `notif.expire()` if available.

---

## Plugin types

### OSD (single-slot popup)

Used when a plugin shows at most one popup at a time (e.g. volume, brightness). The popup reflects live state from the `State` object.

**`State.qml`** — maintain a single-row `activeList`:

```qml
QtObject {
    id: state

    property ListModel activeList: ListModel {}

    // Your live state properties
    property int currentValue: 0

    function showOsd() {
        if (activeList.count === 0) {
            activeList.append({ "notif": {} })
        } else {
            // Re-set triggers dataChanged → PopupWatcher restarts dismiss timer
            activeList.set(0, { "notif": {} })
        }
    }

    function dismiss() { activeList.clear() }

    // Required by PopupWatcher's onDismiss callback
    function removeActive(notif) { dismiss() }
}
```

**`Popup.qml`** — read live data from `popupData` (which will be the `State` object):

```qml
Item {
    property var popupData: null   // State object (currentValue, etc.)
    property var themeProvider: null
    property var iconProvider: null
    // signal requestDismiss  — only needed if the popup has a close button

    // Bind directly to popupData properties — updates reactively
    readonly property int value: popupData?.currentValue ?? 0
}
```

---

### Multi-popup (one entry per item)

Used when a plugin shows several independent popups simultaneously (e.g. notifications). Each row in `activeList` becomes a separate popup.

> **Important**: Quickshell reuses C++ QObject references between events. Never rely on `item.notif` being valid after insertion — it may become `null` when the next event arrives. Always snapshot display data into plain JS at append time.

**`State.qml`** — append one row per item with a stable id and a plain-JS data snapshot:

```qml
QtObject {
    id: state

    property ListModel activeList: ListModel {}

    function addItem(sourceObj) {
        const actions = [];
        for (let i = 0; i < sourceObj.actions.length; i++) {
            const act = sourceObj.actions[i];
            // Capture invoke() in a closure so it survives QObject reuse
            ;(function(a) {
                actions.push({
                    identifier: a.identifier,
                    text: a.text,
                    invoke: function() { a.invoke(); }
                });
            })(act);
        }

        activeList.append({
            "notif":     sourceObj,           // QObject ref — may become null later
            "notifId":   sourceObj.id,        // stable primitive — use for identity
            "notifData": {                    // plain-JS snapshot — use for display
                id:       sourceObj.id,
                title:    sourceObj.title || "",
                body:     sourceObj.body  || "",
                actions:  actions
            },
            "notifTimeout": 0                 // 0 = use overlay default; -1 = no auto-dismiss
        });
    }

    // PopupWatcher matches by notifId, so accept both QObjects and plain proxies {id: N}
    function removeActive(notif) {
        if (!notif) return;
        const searchId = notif.id ?? notif.notifId;
        for (let i = 0; i < activeList.count; i++) {
            if (activeList.get(i).notifId === searchId) {
                activeList.remove(i);
                return;
            }
        }
    }
}
```

**`Popup.qml`** — read from `popupData` (plain-JS `notifData` snapshot):

```qml
Item {
    property var popupData: null   // plain-JS notifData snapshot
    property var themeProvider: null
    property var iconProvider: null
    signal requestDismiss          // emit to trigger manual dismiss

    Text { text: popupData?.title || "" }
    Text { text: popupData?.body  || "" }
}
```

---

## activeList row schema

| Role | Type | Required | Description |
| --- | --- | --- | --- |
| `notif` | QObject / `{}` | ✅ | Live object reference. Used for `expire()`. May be `null` for OSD plugins or after QObject reuse. |
| `notifId` | number / string | Multi-popup only | Stable primitive id. Used by `PopupWatcher` to derive the overlay entry id and by `removeActive` for matching. |
| `notifData` | plain JS object | Multi-popup only | Snapshot of all display data. Consumed by `Popup.qml` as `popupData`. Must be set at append time, before the QObject can be reused. |
| `notifTimeout` | int (ms) | ❌ | `-1` = no auto-dismiss, `0` = use overlay default (`popupTimeout`), `>0` = explicit ms. |

---

## Popup.qml interface

The framework injects these properties into every loaded `Popup.qml`:

| Property | Type | Description |
| --- | --- | --- |
| `popupData` | `var` | Display data. Plain-JS `notifData` for multi-popup plugins; live `State` object for OSD plugins. |
| `themeProvider` | `var` | Theme provider (colors, font, spacing, radius). |
| `iconProvider` | `var` | Icon provider for `LucideIcon`. |

To request a manual dismiss (e.g. a close button), declare and emit:

```qml
signal requestDismiss
```

---

## Overlay position & timeout

Configured globally in `settings.json` under `framework.overlay`:

```json
"overlay": {
    "width": 360,
    "position": { "side": "top", "align": "center" },
    "popupTimeout": 5000,
    "maxVisible": 5
}
```

`maxVisible` controls how many popups are shown at once across all plugins in a given position group. Excess popups are queued and become visible as earlier ones are dismissed.
