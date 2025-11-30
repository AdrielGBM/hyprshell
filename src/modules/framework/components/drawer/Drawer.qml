pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: drawer

    required property string side
    required property var drawerSizes
    required property var barSizes
    required property var settings
    required property int gap
    required property int radius
    required property string color

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"

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
                right: drawer.side === "right" || drawer.side === "top" || drawer.side === "bottom" ? drawer.gap : 0
            }

            implicitWidth: (drawer.side === "left" || drawer.side === "right") ? drawer.settings.drawerWidth : modelData.width
            implicitHeight: (drawer.side === "top" || drawer.side === "bottom") ? drawer.settings.drawerHeight : modelData.height

            Item {
                anchors.fill: parent

                Rectangle {
                    id: drawer1Container
                    radius: drawer.radius
                    color: drawer.color

                    visible: drawer.drawerSizes.isDrawerActive(drawer.side, 0)

                    implicitWidth: {
                        if (drawer.side === "top" || drawer.side === "bottom") {
                            var count = drawer.drawerSizes.getDrawerCount(drawer.side);
                            if (count === 1) {
                                return parent.width;
                            } else if (count === 2) {
                                return (parent.width - drawer.gap) / 2;
                            }
                            return parent.width;
                        }
                        return parent.width;
                    }

                    implicitHeight: {
                        if (drawer.side === "left" || drawer.side === "right") {
                            var count = drawer.drawerSizes.getDrawerCount(drawer.side);
                            if (count === 1) {
                                return parent.height;
                            } else if (count === 2) {
                                return (parent.height - drawer.gap) / 2;
                            }
                            return parent.height;
                        }
                        return parent.height;
                    }
                }

                Rectangle {
                    id: drawer2Container
                    radius: drawer.radius
                    color: drawer.color

                    visible: drawer.drawerSizes.isDrawerActive(drawer.side, 1)

                    x: {
                        if (drawer.side === "top" || drawer.side === "bottom") {
                            return drawer1Container.width + drawer.gap;
                        }
                        return 0;
                    }

                    y: {
                        if (drawer.side === "left" || drawer.side === "right") {
                            return drawer1Container.height + drawer.gap;
                        }
                        return 0;
                    }

                    implicitWidth: {
                        if (drawer.side === "top" || drawer.side === "bottom") {
                            return (parent.width - drawer.gap) / 2;
                        }
                        return parent.width;
                    }

                    implicitHeight: {
                        if (drawer.side === "left" || drawer.side === "right") {
                            return (parent.height - drawer.gap) / 2;
                        }
                        return parent.height;
                    }
                }
            }
        }
    }
}
