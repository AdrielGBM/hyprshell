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

    readonly property int gap: themeProvider?.spacing?.lg ?? 16
    readonly property int radius: themeProvider?.radius?.md ?? 8

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Top

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

            exclusiveZone: (drawer.drawerState.isPush(drawer.side + "-1") || drawer.drawerState.isPush(drawer.side + "-2")) ? ((drawer.side === "top" || drawer.side === "bottom") ? drawer.drawerHeight : drawer.drawerWidth) : 0

            Item {
                anchors.fill: parent

                Item {
                    id: slot1

                    readonly property bool isOpen: drawer.drawerState.isOpen(drawer.side + "-1")
                    readonly property int openCount: drawer.drawerState.getOpenCount(drawer.side)
                    readonly property var accent: drawer.drawerState.getAccent(drawer.side + "-1")

                    visible: isOpen

                    width: {
                        if (drawer.side === "top" || drawer.side === "bottom")
                            return openCount === 2 ? (parent.width - drawer.gap) / 2 : parent.width;
                        return parent.width;
                    }
                    height: {
                        if (drawer.side === "left" || drawer.side === "right")
                            return openCount === 2 ? (parent.height - drawer.gap) / 2 : parent.height;
                        return parent.height;
                    }

                    Rectangle {
                        anchors.fill: parent
                        radius: drawer.radius
                        color: drawer.color
                        border.color: slot1.accent !== "" ? slot1.accent : "transparent"
                        border.width: slot1.accent !== "" ? 1 : 0

                        Loader {
                            anchors.fill: parent
                            active: slot1.isOpen
                            sourceComponent: drawer.drawerState.contents[drawer.side + "-1"] ?? null
                            onLoaded: {
                                const props = drawer.drawerState.getContentProperties(drawer.side + "-1");
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

                Item {
                    id: slot2

                    readonly property bool isOpen: drawer.drawerState.isOpen(drawer.side + "-2")
                    readonly property var accent: drawer.drawerState.getAccent(drawer.side + "-2")

                    visible: isOpen

                    x: (drawer.side === "top" || drawer.side === "bottom") ? (slot1.isOpen ? slot1.width + drawer.gap : 0) : 0
                    y: (drawer.side === "left" || drawer.side === "right") ? (slot1.isOpen ? slot1.height + drawer.gap : 0) : 0

                    width: {
                        if (drawer.side === "top" || drawer.side === "bottom")
                            return slot1.isOpen ? (parent.width - drawer.gap) / 2 : parent.width;
                        return parent.width;
                    }
                    height: {
                        if (drawer.side === "left" || drawer.side === "right")
                            return slot1.isOpen ? (parent.height - drawer.gap) / 2 : parent.height;
                        return parent.height;
                    }

                    Rectangle {
                        anchors.fill: parent
                        radius: drawer.radius
                        color: drawer.color
                        border.color: slot2.accent !== "" ? slot2.accent : "transparent"
                        border.width: slot2.accent !== "" ? 1 : 0

                        Loader {
                            anchors.fill: parent
                            active: slot2.isOpen
                            sourceComponent: drawer.drawerState.contents[drawer.side + "-2"] ?? null
                            onLoaded: {
                                const props = drawer.drawerState.getContentProperties(drawer.side + "-2");
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
    }
}
