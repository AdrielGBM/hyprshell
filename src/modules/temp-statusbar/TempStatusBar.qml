pragma ComponentBehavior: Bound

import QtQuick
import Quickshell.Hyprland
import Quickshell.Services.UPower
import Quickshell.Services.SystemTray
import "../../shared/components"

Item {
    id: statusBar

    property var themeProvider: null

    Row {
        spacing: 0
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom

        Repeater {
            model: 10

            Button {
                id: wsBtn
                required property int index

                property int wsId: index + 1
                property bool isActive: Hyprland.focusedMonitor?.activeWorkspace?.id === wsId
                property bool hasWindows: {
                    const count = Hyprland.workspaces.rowCount();
                    for (let i = 0; i < count; i++) {
                        if (Hyprland.workspaces.values[i]?.id === wsId)
                            return true;
                    }
                    return false;
                }

                themeProvider: statusBar.themeProvider
                variant: wsBtn.isActive ? "filled" : "ghost"
                height: parent.height
                width: height
                onClicked: Hyprland.dispatch("workspace " + wsBtn.wsId)

                Text {
                    anchors.centerIn: parent
                    text: wsBtn.wsId
                    font.pixelSize: 12
                    font.bold: wsBtn.isActive
                    color: wsBtn.isActive ? (statusBar.themeProvider?.base ?? "#191724") : wsBtn.hasWindows ? (statusBar.themeProvider?.text ?? "#e0def4") : (statusBar.themeProvider?.muted ?? "#6e6a86")
                }
            }
        }
    }

    Row {
        anchors.right: parent.right
        anchors.rightMargin: 8
        anchors.verticalCenter: parent.verticalCenter
        spacing: 12

        Text {
            visible: SystemTray.items.rowCount() > 0
            text: "tray:" + SystemTray.items.rowCount()
            color: statusBar.themeProvider ? statusBar.themeProvider.subtle : "#ffffff"
            font.pixelSize: 11
        }

        Text {
            property var battery: {
                const devices = UPower.devices;
                const count = devices.rowCount();
                for (let i = 0; i < count; i++) {
                    const d = devices.values[i];
                    if (d?.type === 2)
                        return d;
                }
                return UPower.displayDevice;
            }

            visible: battery !== null
            text: {
                if (!battery)
                    return "";
                const pct = Math.round(battery.percentage * 100);
                const charging = battery.state === 1 || battery.state === 5;
                return (charging ? "+" : "") + pct + "%";
            }
            color: statusBar.themeProvider ? statusBar.themeProvider.text : "#ffffff"
            font.pixelSize: 11
        }

        Text {
            id: clock
            color: statusBar.themeProvider ? statusBar.themeProvider.text : "#ffffff"
            font.pixelSize: 11

            Timer {
                interval: 1000
                running: true
                repeat: true
                triggeredOnStart: true
                onTriggered: {
                    const now = new Date();
                    const h = String(now.getHours()).padStart(2, '0');
                    const m = String(now.getMinutes()).padStart(2, '0');
                    const d = String(now.getDate()).padStart(2, '0');
                    const mo = String(now.getMonth() + 1).padStart(2, '0');
                    clock.text = h + ":" + m + "  " + d + "/" + mo;
                }
            }
        }
    }
}
