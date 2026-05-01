pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: windowManager

    required property var windowState

    Instantiator {
        model: Object.keys(windowManager.windowState.openWindows)

        delegate: WindowInstance {
            required property string modelData

            windowKey: modelData
            windowComponent: windowManager.windowState.getComponent(modelData)
            windowProps: windowManager.windowState.getProps(modelData)
            windowState: windowManager.windowState
        }
    }
}
