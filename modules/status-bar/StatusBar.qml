pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Hyprland
import Quickshell.Services.UPower
import Quickshell.Services.SystemTray
import QtQuick

Scope {
    id: bar

    property var settings: null
    property var settingsWindow: null
    property var themeProvider: null

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
            color: bar.themeProvider.base

            Row {
                id: leftContent
                anchors.left: parent.left
                anchors.verticalCenter: parent.verticalCenter
                anchors.margins: 8
                spacing: 12

                Rectangle {
                    width: 30
                    height: 20
                    color: settingsArea.containsMouse ? bar.themeProvider.surface : "transparent"
                    radius: 4
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        text: "⚙"
                        color: bar.themeProvider.accent1
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

                Row {
                    spacing: 4
                    anchors.verticalCenter: parent.verticalCenter

                    Repeater {
                        model: 10

                        Rectangle {
                            required property int index

                            width: 24
                            height: 20
                            radius: 4

                            property int workspaceId: index + 1
                            property bool isActive: Hyprland.focusedMonitor?.activeWorkspace?.id === workspaceId
                            property bool hasWindows: {
                                const ws = Hyprland.workspaces;
                                const count = ws.rowCount();
                                for (let i = 0; i < count; i++) {
                                    const workspace = ws.values[i];
                                    if (workspace && workspace.id === workspaceId) {
                                        return true;
                                    }
                                }
                                return false;
                            }

                            color: {
                                if (isActive) {
                                    return bar.themeProvider.accent1;
                                } else if (hasWindows) {
                                    return bar.themeProvider.highlightMed;
                                } else {
                                    return bar.themeProvider.surface;
                                }
                            }

                            Text {
                                text: parent.index + 1
                                color: {
                                    if (parent.isActive) {
                                        return bar.themeProvider.base;
                                    } else if (parent.hasWindows) {
                                        return bar.themeProvider.text;
                                    } else {
                                        return bar.themeProvider.muted;
                                    }
                                }
                                font.pixelSize: 11
                                font.bold: {
                                    const workspaceId = parent.index + 1;
                                    const activeWorkspace = Hyprland.focusedMonitor?.activeWorkspace?.id ?? -1;
                                    return activeWorkspace === workspaceId;
                                }
                                anchors.centerIn: parent
                            }

                            MouseArea {
                                anchors.fill: parent
                                onClicked: {
                                    const workspace = String(parent.index + 1);
                                    Hyprland.dispatch("workspace " + workspace);
                                }
                            }
                        }
                    }
                }
            }

            Row {
                id: rightContent
                anchors.right: parent.right
                anchors.verticalCenter: parent.verticalCenter
                anchors.margins: 8
                spacing: 12

                Row {
                    spacing: 6
                    anchors.verticalCenter: parent.verticalCenter

                    Repeater {
                        model: SystemTray.items

                        Rectangle {
                            required property var modelData

                            width: 20
                            height: 20
                            color: trayMouseArea.containsMouse ? bar.themeProvider.surface : "transparent"
                            radius: 4
                            anchors.verticalCenter: parent.verticalCenter

                            property var trayItem: modelData

                            Image {
                                source: parent.trayItem.icon ?? ""
                                width: 16
                                height: 16
                                anchors.centerIn: parent
                                smooth: true
                            }

                            MouseArea {
                                id: trayMouseArea
                                anchors.fill: parent
                                hoverEnabled: true
                                acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton

                                onClicked: mouse => {
                                    if (mouse.button === Qt.LeftButton) {
                                        parent.trayItem.activate();
                                    } else if (mouse.button === Qt.RightButton) {
                                        const menu = parent.trayItem.menu;
                                        if (menu) {
                                            menu.open(parent.trayItem);
                                        }
                                    } else if (mouse.button === Qt.MiddleButton) {
                                        parent.trayItem.secondaryActivate();
                                    }
                                }

                                onWheel: wheel => {
                                    parent.trayItem.scroll(wheel.angleDelta.y);
                                }
                            }
                        }
                    }
                }

                Rectangle {
                    width: 1
                    height: 16
                    color: bar.themeProvider.surface
                    anchors.verticalCenter: parent.verticalCenter
                    visible: SystemTray.items.rowCount() > 0
                }

                Row {
                    id: batteryRow
                    spacing: 6
                    anchors.verticalCenter: parent.verticalCenter

                    property var battery: {
                        const devices = UPower.devices;
                        const count = devices.rowCount();
                        for (let i = 0; i < count; i++) {
                            const device = devices.values[i];
                            if (device && device.type === 2) {
                                return device;
                            }
                        }
                        return UPower.displayDevice;
                    }

                    visible: battery !== null

                    Text {
                        text: {
                            if (!batteryRow.battery)
                                return "🔋";
                            const device = batteryRow.battery;
                            const percent = device.percentage * 100;
                            const isCharging = device.state === 1 || device.state === 5;
                            const isFull = device.state === 4;

                            if (isCharging || isFull) {
                                return "⚡";
                            } else if (percent > 60) {
                                return "🔋";
                            } else if (percent > 30) {
                                return "🔋";
                            } else if (percent > 15) {
                                return "🪫";
                            } else {
                                return "🪫";
                            }
                        }
                        color: bar.themeProvider.text
                        font.pixelSize: 12
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Text {
                        text: {
                            if (!batteryRow.battery)
                                return "N/A";
                            const device = batteryRow.battery;
                            const state = device.state;
                            let percent = device.percentage * 100;

                            if (state === 4 && percent > 95) {
                                percent = 100;
                            }

                            return Math.round(percent) + "%";
                        }
                        color: bar.themeProvider.text
                        font.pixelSize: 11
                        anchors.verticalCenter: parent.verticalCenter
                    }
                }

                Rectangle {
                    width: 1
                    height: 16
                    color: bar.themeProvider.surface
                    anchors.verticalCenter: parent.verticalCenter
                    visible: UPower.displayDevice !== null
                }

                Row {
                    id: timeRow
                    spacing: 8
                    anchors.verticalCenter: parent.verticalCenter

                    Text {
                        id: timeText
                        color: bar.themeProvider.text
                        font.pixelSize: 11
                        anchors.verticalCenter: parent.verticalCenter

                        Timer {
                            interval: 1000
                            running: true
                            repeat: true
                            triggeredOnStart: true
                            onTriggered: {
                                const now = new Date();
                                const hours = String(now.getHours()).padStart(2, '0');
                                const minutes = String(now.getMinutes()).padStart(2, '0');
                                timeText.text = hours + ":" + minutes;
                            }
                        }
                    }

                    Text {
                        id: dateText
                        color: bar.themeProvider.text
                        font.pixelSize: 11
                        anchors.verticalCenter: parent.verticalCenter

                        Component.onCompleted: {
                            const now = new Date();
                            const day = String(now.getDate()).padStart(2, '0');
                            const month = String(now.getMonth() + 1).padStart(2, '0');
                            const year = now.getFullYear();
                            dateText.text = day + "/" + month + "/" + year;
                        }

                        Timer {
                            interval: 60000
                            running: true
                            repeat: true
                            onTriggered: {
                                const now = new Date();
                                const day = String(now.getDate()).padStart(2, '0');
                                const month = String(now.getMonth() + 1).padStart(2, '0');
                                const year = now.getFullYear();
                                dateText.text = day + "/" + month + "/" + year;
                            }
                        }
                    }
                }
            }
        }
    }
}
