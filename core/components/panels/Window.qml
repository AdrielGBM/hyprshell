pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell

Scope {
    id: window

    property var settings: null
    property var themeManager: null

    property string windowTitle: ""
    property Component content: null
    property bool isOpen: false

    Variants {
        model: window.isOpen ? [Quickshell.screens[0]] : []

        FloatingWindow {
            required property var modelData
            screen: modelData

            onVisibleChanged: {
                if (!visible) {
                    window.isOpen = false;
                }
            }

            // === WINDOW TITLE ===
            ColumnLayout {
                anchors.fill: parent
                spacing: 0

                Rectangle {
                    Layout.fillWidth: true
                    Layout.preferredHeight: 48
                    color: window.themeManager.surface

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: window.settings.spacing
                        anchors.rightMargin: window.settings.spacing
                        spacing: window.settings.spacing

                        Text {
                            text: window.windowTitle
                            color: window.themeManager.text
                            font.pixelSize: window.settings.largeFontSize
                            font.bold: true
                            verticalAlignment: Text.AlignVCenter
                            Layout.fillWidth: true
                        }

                        Rectangle {
                            Layout.preferredWidth: 24
                            Layout.preferredHeight: 24
                            color: closeButtonArea.containsMouse ? window.themeManager.accent1 : "transparent"
                            radius: window.settings.radius

                            Text {
                                text: "✕"
                                color: closeButtonArea.containsMouse ? window.themeManager.base : window.themeManager.subtle
                                font.pixelSize: window.settings.largeFontSize
                                anchors.centerIn: parent
                            }

                            MouseArea {
                                id: closeButtonArea
                                anchors.fill: parent
                                hoverEnabled: true
                                onClicked: window.close()
                            }
                        }
                    }
                }

                // === CONTENT ===
                Loader {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    sourceComponent: window.content
                }
            }
        }
    }

    // === FUNCTIONS ===
    function toggle() {
        isOpen = !isOpen;
    }

    function open() {
        isOpen = true;
    }

    function close() {
        isOpen = false;
    }
}
