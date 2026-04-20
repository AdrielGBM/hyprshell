pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: panel

    required property string side
    required property var drawerState
    required property var barSizes
    required property int drawerWidth
    required property int drawerHeight
    property var themeProvider: null
    required property color color

    readonly property int gap: themeProvider?.spacing
    readonly property int radius: themeProvider?.radius

    readonly property bool isHorizontalSide: side === "top" || side === "bottom"
    readonly property string position: drawerState.activePanelPosition

    Variants {
        model: Quickshell.screens

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
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay

            anchors {
                top: panel.isHorizontalSide ? panel.side === "top" : panel.position === "start"
                bottom: panel.isHorizontalSide ? panel.side === "bottom" : panel.position === "end"
                left: panel.isHorizontalSide ? panel.position === "start" : panel.side === "left"
                right: panel.isHorizontalSide ? panel.position === "end" : panel.side === "right"
            }

            margins {
                top: {
                    if (panel.side === "top")
                        return panel.barSizes.top + panel.gap;
                    if (!panel.isHorizontalSide && panel.position === "start")
                        return panel.barSizes.top + panel.gap;
                    return 0;
                }
                bottom: {
                    if (panel.side === "bottom")
                        return panel.barSizes.bottom + panel.gap;
                    if (!panel.isHorizontalSide && panel.position === "end")
                        return panel.barSizes.bottom + panel.gap;
                    return 0;
                }
                left: {
                    if (panel.side === "left")
                        return panel.barSizes.left + panel.gap;
                    if (panel.isHorizontalSide && panel.position === "start")
                        return panel.barSizes.left + panel.gap;
                    return 0;
                }
                right: {
                    if (panel.side === "right")
                        return panel.barSizes.right + panel.gap;
                    if (panel.isHorizontalSide && panel.position === "end")
                        return panel.barSizes.right + panel.gap;
                    return 0;
                }
            }

            implicitWidth: panel.isHorizontalSide ? (modelData.width - panel.gap * 3) / 2 : panel.drawerWidth
            implicitHeight: panel.isHorizontalSide ? panel.drawerHeight : (modelData.height - panel.gap * 3) / 2

            WlrLayershell.exclusiveZone: -1

            Rectangle {
                anchors.fill: parent
                radius: panel.radius
                color: panel.color

                Loader {
                    anchors.fill: parent
                    active: true
                    sourceComponent: panel.drawerState.contents[panel.side] ?? null
                    onLoaded: {
                        const props = panel.drawerState.getContentProperties(panel.side);
                        for (const key in props)
                            item[key] = props[key];
                        if ("themeProvider" in item)
                            item.themeProvider = Qt.binding(function () {
                                return panel.themeProvider;
                            });
                        if ("drawerState" in item)
                            item.drawerState = Qt.binding(function () {
                                return panel.drawerState;
                            });
                    }
                }
            }
        }
    }
}
