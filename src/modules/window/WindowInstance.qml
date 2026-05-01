pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell
import qs.src.shared.services.theme

Scope {
    id: window

    required property string windowKey
    required property var windowComponent
    required property var windowProps
    required property var windowState

    readonly property int gap: Theme.spacing
    readonly property int radius: Theme.radius

    FloatingWindow {
        visible: true
        color: Theme.base

        ColumnLayout {
            anchors.fill: parent
            spacing: 0

            Rectangle {
                Layout.fillWidth: true
                Layout.preferredHeight: 48
                color: Theme.surface

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: window.gap
                    anchors.rightMargin: window.gap
                    spacing: window.gap

                    Item {
                        Layout.fillWidth: true
                    }

                    Rectangle {
                        Layout.preferredWidth: 24
                        Layout.preferredHeight: 24
                        color: closeButtonArea.containsMouse ? Theme.accent1 : "transparent"
                        radius: window.radius

                        Text {
                            text: "✕"
                            color: closeButtonArea.containsMouse ? Theme.base : Theme.subtle
                            font.pixelSize: 14
                            anchors.centerIn: parent
                        }

                        MouseArea {
                            id: closeButtonArea
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: window.windowState.closeWindow(window.windowKey)
                        }
                    }
                }
            }

            Loader {
                id: windowContentLoader
                Layout.fillWidth: true
                Layout.fillHeight: true
                sourceComponent: window.windowComponent
                onLoaded: {
                    const props = window.windowProps;
                    for (const key in props)
                        item[key] = props[key];
                    if ("windowState" in item)
                        item.windowState = Qt.binding(function () {
                            return window.windowState;
                        });
                }
            }
        }
    }
}
