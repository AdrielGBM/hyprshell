pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import QtQuick.Controls
import qs.src.shared.services.theme

Scope {
    id: drawer

    required property string side
    required property var drawerState
    required property var barSizes
    required property int drawerWidth
    required property color color

    readonly property int gap: Theme.spacing
    readonly property int radius: Theme.radius

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            visible: drawer.drawerState.activeScreen === null || modelData === drawer.drawerState.activeScreen
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Bottom

            anchors {
                top: true
                bottom: true
                left: drawer.side === "left"
                right: drawer.side === "right"
            }

            margins {
                top: drawer.gap
                bottom: drawer.gap
                left: drawer.side === "left" ? drawer.gap : 0
                right: drawer.side === "right" ? drawer.gap : 0
            }

            implicitWidth: drawer.drawerWidth
            implicitHeight: modelData.height

            exclusiveZone: visible ? drawer.drawerWidth : 0

            Rectangle {
                anchors.fill: parent
                radius: drawer.radius
                color: drawer.color
                clip: true

                Flickable {
                    id: drawerFlick
                    anchors.fill: parent
                    clip: true
                    contentWidth: width
                    contentHeight: Math.max(height, drawerContentLoader.implicitHeight)
                    interactive: contentHeight > height

                    Loader {
                        id: drawerContentLoader
                        width: drawerFlick.width
                        height: drawerFlick.contentHeight
                        sourceComponent: drawer.drawerState.contents[drawer.side] ?? null
                        onLoaded: {
                            const props = drawer.drawerState.getContentProperties(drawer.side);
                            for (const key in props)
                                item[key] = props[key];
                            if ("drawerState" in item)
                                item.drawerState = Qt.binding(function () {
                                    return drawer.drawerState;
                                });
                        }
                    }

                    ScrollBar.vertical: ScrollBar {
                        policy: ScrollBar.AsNeeded
                    }
                }
            }
        }
    }
}
