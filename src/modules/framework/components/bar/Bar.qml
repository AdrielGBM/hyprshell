pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../chipWiring.js" as ChipWiring
import qs.src.shared.theme

Scope {
    id: bar

    property string position
    property bool frameMode: false
    property BarSizes barSizes
    property var drawerState: null
    property var overlayState: null
    property var moduleRegistry: null
    property var slotConfig: ({})

    property color color

    readonly property int gap: Theme.spacing
    readonly property int radius: Theme.radius
    readonly property int padding: Math.round(Theme.spacing / 2)
    readonly property int chipRadius: Math.max(0, bar.radius - bar.padding)
    readonly property bool isHorizontal: position === "top" || position === "bottom"

    property string defaultAccent: ""
    property string defaultVariant: "default"

    function resolveComponent(entry) {
        if (typeof entry === "string")
            return bar.moduleRegistry?.get(entry) ?? null;
        if (entry && entry.id)
            return bar.moduleRegistry?.get(entry.id) ?? null;
        return null;
    }

    function slotOffset(slotName) {
        if (slotName === "center")
            return 100;
        if (slotName === "right" || slotName === "bottom")
            return 200;
        return 0;
    }

    function wireItem(item, entry, slotName, slotIndex, screen) {
        ChipWiring.wire(item, entry, {
            drawerState: bar.drawerState,
            overlayState: bar.overlayState,
            moduleRegistry: bar.moduleRegistry,
            barPosition: bar.position,
            barIndex: bar.slotOffset(slotName) + slotIndex,
            barScreen: screen,
            chipRadius: bar.chipRadius
        });
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: barWindow
            required property var modelData
            readonly property var screenData: modelData

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Bottom

            anchors {
                top: bar.position === "top" || bar.position === "left" || bar.position === "right"
                bottom: bar.position === "bottom" || bar.position === "left" || bar.position === "right"
                left: bar.position === "left" || bar.position === "top" || bar.position === "bottom"
                right: bar.position === "right" || bar.position === "top" || bar.position === "bottom"
            }

            margins {
                top: (bar.position === "left" || bar.position === "right") ? bar.barSizes.top + bar.gap * (bar.frameMode ? 1 : (bar.barSizes.top > 0 ? 2 : 1)) : (bar.position === "top" && !bar.frameMode ? bar.gap : 0)
                bottom: (bar.position === "left" || bar.position === "right") ? bar.barSizes.bottom + bar.gap * (bar.frameMode ? 1 : (bar.barSizes.bottom > 0 ? 2 : 1)) : (bar.position === "bottom" && !bar.frameMode ? bar.gap : 0)
                left: (bar.position === "top" || bar.position === "bottom") ? bar.gap : (bar.position === "left" && !bar.frameMode ? bar.gap : 0)
                right: (bar.position === "top" || bar.position === "bottom") ? bar.gap : (bar.position === "right" && !bar.frameMode ? bar.gap : 0)
            }

            implicitWidth: bar.position === "left" || bar.position === "right" ? bar.barSizes[bar.position] : modelData.width
            implicitHeight: bar.position === "top" || bar.position === "bottom" ? bar.barSizes[bar.position] : modelData.height

            Rectangle {
                anchors.fill: parent
                radius: bar.radius
                color: bar.color
            }

            Item {
                anchors.fill: parent
                anchors.margins: bar.padding
                visible: bar.barSizes[bar.position] !== bar.barSizes.inactive

                readonly property bool hHasLeft: (bar.slotConfig.left ?? []).length > 0
                readonly property bool hHasCenter: (bar.slotConfig.center ?? []).length > 0
                readonly property bool hHasRight: (bar.slotConfig.right ?? []).length > 0
                readonly property bool vHasTop: (bar.slotConfig.top ?? []).length > 0
                readonly property bool vHasCenter: (bar.slotConfig.center ?? []).length > 0
                readonly property bool vHasBottom: (bar.slotConfig.bottom ?? []).length > 0
                readonly property real hSlotMax: (hHasCenter && (hHasLeft || hHasRight)) ? parent.width / 3 : parent.width / Math.max(1, (hHasLeft ? 1 : 0) + (hHasCenter ? 1 : 0) + (hHasRight ? 1 : 0))
                readonly property real vSlotMax: (vHasCenter && (vHasTop || vHasBottom)) ? parent.height / 3 : parent.height / Math.max(1, (vHasTop ? 1 : 0) + (vHasCenter ? 1 : 0) + (vHasBottom ? 1 : 0))

                Row {
                    visible: bar.isHorizontal
                    anchors.left: parent.left
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: Math.min(implicitWidth, parent.hSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.left ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            height: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "left", index, barWindow.screenData)
                        }
                    }
                }

                Row {
                    visible: bar.isHorizontal
                    anchors.centerIn: parent
                    width: Math.min(implicitWidth, parent.hSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.center ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            height: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "center", index, barWindow.screenData)
                        }
                    }
                }

                Row {
                    visible: bar.isHorizontal
                    anchors.right: parent.right
                    anchors.top: parent.top
                    anchors.bottom: parent.bottom
                    width: Math.min(implicitWidth, parent.hSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.right ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            height: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "right", index, barWindow.screenData)
                        }
                    }
                }

                Column {
                    visible: !bar.isHorizontal
                    anchors.top: parent.top
                    anchors.left: parent.left
                    anchors.right: parent.right
                    height: Math.min(implicitHeight, parent.vSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.top ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            width: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "top", index, barWindow.screenData)
                        }
                    }
                }

                Column {
                    visible: !bar.isHorizontal
                    anchors.centerIn: parent
                    height: Math.min(implicitHeight, parent.vSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.center ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            width: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "center", index, barWindow.screenData)
                        }
                    }
                }

                Column {
                    visible: !bar.isHorizontal
                    anchors.bottom: parent.bottom
                    anchors.left: parent.left
                    anchors.right: parent.right
                    height: Math.min(implicitHeight, parent.vSlotMax)
                    spacing: bar.gap
                    clip: true

                    Repeater {
                        model: bar.slotConfig.bottom ?? []

                        Loader {
                            required property var modelData
                            required property int index

                            width: bar.barSizes[bar.position] - bar.padding * 2
                            sourceComponent: bar.resolveComponent(modelData)

                            onLoaded: bar.wireItem(item, modelData, "bottom", index, barWindow.screenData)
                        }
                    }
                }
            }
        }
    }
}
