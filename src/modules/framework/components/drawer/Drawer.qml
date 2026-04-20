pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: drawer

    required property string side
    required property var drawerState
    required property var barSizes
    required property int drawerWidth
    required property int drawerHeight
    property var themeProvider: null
    required property color color

    readonly property int gap: themeProvider?.spacing
    readonly property int radius: themeProvider?.radius

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            visible: drawer.drawerState.activeScreen === null || modelData === drawer.drawerState.activeScreen
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Bottom

            anchors {
                top: drawer.side === "top" || drawer.side === "left" || drawer.side === "right"
                bottom: drawer.side === "bottom" || drawer.side === "left" || drawer.side === "right"
                left: drawer.side === "left" || drawer.side === "top" || drawer.side === "bottom"
                right: drawer.side === "right" || drawer.side === "top" || drawer.side === "bottom"
            }

            margins {
                top: (drawer.side === "top" || drawer.side === "left" || drawer.side === "right") ? drawer.gap : 0
                bottom: (drawer.side === "bottom" || drawer.side === "left" || drawer.side === "right") ? drawer.gap : 0
                left: (drawer.side === "left" || drawer.side === "top" || drawer.side === "bottom") ? drawer.gap : 0
                right: (drawer.side === "right" || drawer.side === "top" || drawer.side === "bottom") ? drawer.gap : 0
            }

            implicitWidth: (drawer.side === "left" || drawer.side === "right") ? drawer.drawerWidth : modelData.width
            implicitHeight: (drawer.side === "top" || drawer.side === "bottom") ? drawer.drawerHeight : modelData.height

            exclusiveZone: visible ? ((drawer.side === "top" || drawer.side === "bottom") ? drawer.drawerHeight : drawer.drawerWidth) : 0

            Rectangle {
                anchors.fill: parent
                radius: drawer.radius
                color: drawer.color

                Loader {
                    anchors.fill: parent
                    active: true
                    sourceComponent: drawer.drawerState.contents[drawer.side] ?? null
                    onLoaded: {
                        const props = drawer.drawerState.getContentProperties(drawer.side);
                        for (const key in props)
                            item[key] = props[key];
                        if ("themeProvider" in item)
                            item.themeProvider = Qt.binding(function () {
                                return drawer.themeProvider;
                            });
                        if ("drawerState" in item)
                            item.drawerState = Qt.binding(function () {
                                return drawer.drawerState;
                            });
                    }
                }
            }
        }
    }
}
