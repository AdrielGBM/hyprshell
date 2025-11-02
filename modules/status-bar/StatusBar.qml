pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: bar

    property var settings: null
    property var settingsWindow: null
    property var themeManager: null

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: panelWindow
            required property var modelData
            screen: modelData

            anchors {
                top: true
                left: true
                right: true
            }

            implicitHeight: 30
            color: bar.themeManager.base

            Row {
                anchors.fill: parent
                anchors.margins: 8

                Rectangle {
                    width: 30
                    height: 20
                    color: settingsArea.containsMouse ? bar.themeManager.surface : "transparent"
                    radius: 4
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        text: "⚙"
                        color: bar.themeManager.accent1
                        font.pixelSize: 14
                        anchors.centerIn: parent
                    }

                    MouseArea {
                        id: settingsArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: {
                            if (bar.settingsWindow) {
                                bar.settingsWindow.toggle();
                            }
                        }
                    }
                }
            }
        }
    }
}
