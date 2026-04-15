pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: drawerManager

    required property var settings
    required property var barSizes
    required property var drawerState
    property var themeProvider: null

    readonly property color drawerColor: themeProvider ? themeProvider.overlay : "#26233a"

    Variants {
        model: drawerManager.drawerState.activeSide !== "" && drawerManager.drawerState.openSlots.length > 0 ? Quickshell.screens : []

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Top
            WlrLayershell.exclusiveZone: -1

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            margins {
                top: drawerManager.barSizes.top
                bottom: drawerManager.barSizes.bottom
                left: drawerManager.barSizes.left
                right: drawerManager.barSizes.right
            }

            MouseArea {
                anchors.fill: parent
                onClicked: drawerManager.drawerState.close()
            }
        }
    }

    Loader {
        active: drawerManager.drawerState.isOpen("top-1") || drawerManager.drawerState.isOpen("top-2")
        sourceComponent: Drawer {
            side: "top"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            themeProvider: drawerManager.themeProvider
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.drawerState.isOpen("bottom-1") || drawerManager.drawerState.isOpen("bottom-2")
        sourceComponent: Drawer {
            side: "bottom"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            themeProvider: drawerManager.themeProvider
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.drawerState.isOpen("left-1") || drawerManager.drawerState.isOpen("left-2")
        sourceComponent: Drawer {
            side: "left"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            themeProvider: drawerManager.themeProvider
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.drawerState.isOpen("right-1") || drawerManager.drawerState.isOpen("right-2")
        sourceComponent: Drawer {
            side: "right"
            drawerState: drawerManager.drawerState
            barSizes: drawerManager.barSizes
            drawerWidth: drawerManager.settings.drawerWidth
            drawerHeight: drawerManager.settings.drawerHeight
            themeProvider: drawerManager.themeProvider
            color: drawerManager.drawerColor
        }
    }
}
