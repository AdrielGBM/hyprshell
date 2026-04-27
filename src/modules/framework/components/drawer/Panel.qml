pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../sideMargins.js" as SideMargins

Scope {
    id: panel

    required property string side
    required property var drawerState
    required property var barSizes
    required property int drawerWidth
    required property int drawerHeight
    property bool frameMode: false
    property var themeProvider: null
    property var i18nProvider: null
    required property color color

    readonly property int gap: themeProvider?.spacing
    readonly property int radius: themeProvider?.radius

    readonly property bool isHorizontalSide: side === "top" || side === "bottom"
    readonly property string position: drawerState.activePanelPosition

    readonly property var m: SideMargins.calc(panel.side, panel.position, panel.barSizes, panel.frameMode, panel.gap)

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            visible: panel.drawerState.activeScreen === null || modelData === panel.drawerState.activeScreen
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
                top: panel.barSizes.top
                bottom: panel.barSizes.bottom
                left: panel.barSizes.left
                right: panel.barSizes.right
            }

            MouseArea {
                anchors.fill: parent
                onClicked: panel.drawerState.closePanel()
            }
        }
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            visible: panel.drawerState.activeScreen === null || modelData === panel.drawerState.activeScreen
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.exclusiveZone: -1

            anchors {
                top: panel.isHorizontalSide ? panel.side === "top" : panel.position === "start"
                bottom: panel.isHorizontalSide ? panel.side === "bottom" : panel.position === "end"
                left: panel.isHorizontalSide ? panel.position === "start" : panel.side === "left"
                right: panel.isHorizontalSide ? panel.position === "end" : panel.side === "right"
            }

            margins {
                top: panel.m.top
                bottom: panel.m.bottom
                left: panel.m.left
                right: panel.m.right
            }

            implicitWidth: panel.drawerWidth
            implicitHeight: panel.drawerHeight

            Rectangle {
                anchors.fill: parent
                radius: panel.radius
                color: panel.color

                Loader {
                    id: panelContentLoader
                    anchors.fill: parent
                    sourceComponent: panel.drawerState.contents[panel.side] ?? null
                    onLoaded: {
                        const props = panel.drawerState.getContentProperties(panel.side);
                        for (const key in props)
                            item[key] = props[key];
                        if ("drawerState" in item)
                            item.drawerState = Qt.binding(function () {
                                return panel.drawerState;
                            });
                    }

                    Binding {
                        when: panelContentLoader.item !== null
                        target: panelContentLoader.item
                        property: "themeProvider"
                        value: panel.themeProvider
                        restoreMode: Binding.RestoreNone
                    }

                    Binding {
                        when: panelContentLoader.item !== null
                        target: panelContentLoader.item
                        property: "i18nProvider"
                        value: panel.i18nProvider
                        restoreMode: Binding.RestoreNone
                    }
                }
            }
        }
    }
}
