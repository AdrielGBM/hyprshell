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
    property int maxVisible: 5
    property var themeProvider: null

    readonly property var pluginState: moduleRegistry?.states[pluginKey] ?? null
    readonly property bool hasActive: (pluginState?.activeList?.count ?? 0) > 0

    Component {
        id: popupListComp
        PopupList {
            pluginState: watcher.pluginState
            popupComp: watcher.popupComp
            popupTimeout: watcher.popupTimeout
            maxVisible: watcher.maxVisible
            themeProvider: watcher.themeProvider
        }
    }

    onHasActiveChanged: {
        if (!overlayState)
            return;
        if (hasActive) {
            overlayState.show(watcher.pluginKey, popupListComp, {});
        } else {
            overlayState.hide(watcher.pluginKey);
        }
    }
}
