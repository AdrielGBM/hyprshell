pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: watcher

    property string pluginKey: ""
    property var popupComp: null
    property var moduleRegistry: null
    property var overlayState: null
    property int popupTimeout: 5000

    readonly property var pluginState: moduleRegistry?.states[pluginKey] ?? null
    readonly property bool hasActive: (pluginState?.activeList?.count ?? 0) > 0

    onHasActiveChanged: {
        if (!watcher.overlayState)
            return;
        if (watcher.hasActive) {
            watcher.overlayState.push(watcher.pluginKey, null, {
                pluginState: watcher.pluginState,
                popupComp: watcher.popupComp,
                popupTimeout: watcher.popupTimeout
            });
        } else {
            watcher.overlayState.remove(watcher.pluginKey);
        }
    }
}
