pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import qs.src.shared.theme

Scope {
    id: drawerManager

    required property var settings
    required property var barSizes
    required property var drawerState
    property bool frameMode: false

    readonly property color color: Theme.overlay

    Loader {
        active: drawerManager.drawerState.openDrawers["top"] !== undefined
        sourceComponent: Drawer {
            side: "top"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            color: drawerManager.color
        }
    }

    Loader {
        active: drawerManager.drawerState.openDrawers["bottom"] !== undefined
        sourceComponent: Drawer {
            side: "bottom"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            color: drawerManager.color
        }
    }

    Loader {
        active: drawerManager.drawerState.openDrawers["left"] !== undefined
        sourceComponent: Drawer {
            side: "left"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            color: drawerManager.color
        }
    }

    Loader {
        active: drawerManager.drawerState.openDrawers["right"] !== undefined
        sourceComponent: Drawer {
            side: "right"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            color: drawerManager.color
        }
    }

    Loader {
        active: drawerManager.drawerState.activePanelSide !== ""
        sourceComponent: Panel {
            side: drawerManager.drawerState.activePanelSide
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            frameMode: drawerManager.frameMode
            color: drawerManager.color
        }
    }
}
