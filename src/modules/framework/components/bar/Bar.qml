pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: bar

    property string position
    property BarSizes barSizes
    property var themeProvider: null
    property var iconProvider: null
    property var drawerState: null
    property var moduleRegistry: null
    property var slotConfig: ({})

    property color color

    readonly property int gap: themeProvider?.spacing?.lg ?? 16
    readonly property int radius: themeProvider?.radius?.md ?? 8
    readonly property int padding: themeProvider?.spacing?.xs ?? 4
    readonly property bool isHorizontal: position === "top" || position === "bottom"

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

    function wireItem(item, entry, slotName, slotIndex) {
        item.themeProvider = Qt.binding(function () {
            return bar.themeProvider;
        });
        if ("iconProvider" in item)
            item.iconProvider = Qt.binding(function () {
                return bar.iconProvider;
            });
        if ("drawerState" in item)
            item.drawerState = Qt.binding(function () {
                return bar.drawerState;
            });
        if ("moduleRegistry" in item)
            item.moduleRegistry = Qt.binding(function () {
                return bar.moduleRegistry;
            });
        if ("barPosition" in item)
            item.barPosition = Qt.binding(function () {
                return bar.position;
            });
        if ("barIndex" in item)
            item.barIndex = bar.slotOffset(slotName) + slotIndex;
        if (typeof entry === "object" && entry !== null) {
            if ("accentColor" in item && entry.accent)
                item.accentColor = bar.resolveAccent(entry);
        }
    }

    function resolveAccent(entry) {
        if (typeof entry === "string" || !entry || !entry.accent || !bar.themeProvider)
            return "";
        const t = bar.themeProvider.currentTheme;
        if (t && t[entry.accent] !== undefined)
            return t[entry.accent];
        return bar.themeProvider[entry.accent] ?? "";
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

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
                top: (bar.position === "left" || bar.position === "right" ? bar.barSizes.top + bar.gap : 0)
                bottom: (bar.position === "left" || bar.position === "right" ? bar.barSizes.bottom + bar.gap : 0)
                left: (bar.position === "top" || bar.position === "bottom" ? bar.gap : 0)
                right: (bar.position === "top" || bar.position === "bottom" ? bar.gap : 0)
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

                            onLoaded: bar.wireItem(item, modelData, "left", index)
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

                            onLoaded: bar.wireItem(item, modelData, "center", index)
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

                            onLoaded: bar.wireItem(item, modelData, "right", index)
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

                            onLoaded: bar.wireItem(item, modelData, "top", index)
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

                            onLoaded: bar.wireItem(item, modelData, "center", index)
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

                            onLoaded: bar.wireItem(item, modelData, "bottom", index)
                        }
                    }
                }
            }
        }
    }
}
