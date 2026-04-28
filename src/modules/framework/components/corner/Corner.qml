pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../chipWiring.js" as ChipWiring
import qs.src.shared.theme

Scope {
    id: corner

    property string position
    property bool frameMode: false
    property var barSizes
    property var moduleRegistry: null
    property var drawerState: null
    property var overlayState: null
    property var itemConfig: null
    property color color

    readonly property string chipBarPosition: {
        if (drawerState && drawerState.drawerOrientation === "horizontal")
            return isTop ? "top" : "bottom";
        return isLeft ? "left" : "right";
    }

    readonly property int gap: Theme.spacing
    readonly property int radius: Theme.radius
    readonly property int padding: Math.round(Theme.spacing / 2)

    readonly property bool isTop: position === "topLeft" || position === "topRight"
    readonly property bool isLeft: position === "topLeft" || position === "bottomLeft"

    readonly property int cornerBarIndex: {
        if (position === "topLeft")
            return -1;
        if (position === "topRight")
            return -2;
        if (position === "bottomLeft")
            return -3;
        return -4;
    }

    readonly property int hBarSize: isTop ? barSizes.top : barSizes.bottom
    readonly property int vBarSize: isLeft ? barSizes.left : barSizes.right
    readonly property int cornerSize: Math.min(hBarSize, vBarSize)
    readonly property int chipRadius: Math.max(0, corner.radius - corner.padding)

    function resolveComponent() {
        if (!corner.itemConfig || !corner.moduleRegistry)
            return null;
        const id = typeof corner.itemConfig === "string" ? corner.itemConfig : corner.itemConfig.id;
        return corner.moduleRegistry.get(id) ?? null;
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: cornerWindow
            required property var modelData
            readonly property var screenData: modelData

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Bottom
            exclusionMode: ExclusionMode.Ignore

            anchors {
                top: corner.isTop
                bottom: !corner.isTop
                left: corner.isLeft
                right: !corner.isLeft
            }

            margins {
                top: corner.isTop && !corner.frameMode ? corner.gap : 0
                bottom: !corner.isTop && !corner.frameMode ? corner.gap : 0
                left: corner.isLeft && !corner.frameMode ? corner.gap : 0
                right: !corner.isLeft && !corner.frameMode ? corner.gap : 0
            }

            implicitWidth: corner.cornerSize
            implicitHeight: corner.cornerSize

            Rectangle {
                anchors.fill: parent
                color: corner.color
                radius: corner.radius
            }

            Loader {
                id: chipLoader
                anchors.centerIn: parent
                width: corner.cornerSize - corner.padding * 2
                height: corner.cornerSize - corner.padding * 2
                sourceComponent: corner.resolveComponent()

                function doWire() {
                    if (!item)
                        return;
                    ChipWiring.wire(item, corner.itemConfig, {
                        drawerState: corner.drawerState,
                        overlayState: corner.overlayState,
                        moduleRegistry: corner.moduleRegistry,
                        barPosition: corner.chipBarPosition,
                        barIndex: corner.cornerBarIndex,
                        barScreen: cornerWindow.screenData,
                        chipRadius: corner.chipRadius
                    });
                }

                onLoaded: doWire()
            }

            Connections {
                target: corner
                function onItemConfigChanged() {
                    chipLoader.doWire();
                }
            }
        }
    }
}
