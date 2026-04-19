pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: corner

    property string position
    property bool frameMode: false
    property var barSizes
    property var settings
    property var themeProvider: null
    property var iconProvider: null
    property var moduleRegistry: null
    property var drawerState: null
    property var itemConfig: null
    property color color

    readonly property string chipBarPosition: {
        if (drawerState && drawerState.drawerOrientation === "horizontal")
            return isTop ? "top" : "bottom";
        return isLeft ? "left" : "right";
    }

    readonly property int gap: settings.baseGap
    readonly property int radius: settings.baseRadius
    readonly property int padding: Math.round(settings.baseGap / 2)

    readonly property bool isTop: position === "topLeft" || position === "topRight"
    readonly property bool isLeft: position === "topLeft" || position === "bottomLeft"

    readonly property int hBarSize: isTop ? barSizes.top : barSizes.bottom
    readonly property int vBarSize: isLeft ? barSizes.left : barSizes.right
    readonly property int cornerSize: Math.min(hBarSize, vBarSize)

    function resolveComponent() {
        if (!corner.itemConfig || !corner.moduleRegistry)
            return null;
        const id = typeof corner.itemConfig === "string" ? corner.itemConfig : corner.itemConfig.id;
        return corner.moduleRegistry.get(id) ?? null;
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

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
                anchors.centerIn: parent
                width: corner.cornerSize - corner.padding * 2
                height: corner.cornerSize - corner.padding * 2
                sourceComponent: corner.resolveComponent()

                onLoaded: {
                    item.themeProvider = Qt.binding(function () {
                        return corner.themeProvider;
                    });
                    if ("iconProvider" in item)
                        item.iconProvider = Qt.binding(function () {
                            return corner.iconProvider;
                        });
                    if ("drawerState" in item)
                        item.drawerState = Qt.binding(function () {
                            return corner.drawerState;
                        });
                    if ("barPosition" in item)
                        item.barPosition = Qt.binding(function () {
                            return corner.chipBarPosition;
                        });
                    if ("barIndex" in item)
                        item.barIndex = 0;
                    if (typeof corner.itemConfig === "object" && corner.itemConfig !== null) {
                        if ("accentColor" in item && corner.itemConfig.accent && corner.themeProvider) {
                            const t = corner.themeProvider.currentTheme;
                            item.accentColor = (t && t[corner.itemConfig.accent] !== undefined) ? t[corner.itemConfig.accent] : corner.themeProvider[corner.itemConfig.accent] ?? "";
                        }
                        if ("variant" in item && corner.itemConfig.variant)
                            item.variant = corner.itemConfig.variant;
                    }
                }
            }
        }
    }
}
