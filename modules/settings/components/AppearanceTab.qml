pragma ComponentBehavior: Bound

import QtQuick

Column {
    id: appearanceTab

    required property var themeManager
    required property var settings

    spacing: settings.spacing

    ThemeSelector {
        width: parent.width
        themeManager: appearanceTab.themeManager
        settings: appearanceTab.settings
    }

    RadiusControl {
        width: parent.width
        themeManager: appearanceTab.themeManager
        settings: appearanceTab.settings
    }

    Column {
        width: parent.width
        spacing: 8

        Text {
            text: "Fondo de pantalla"
            color: appearanceTab.themeManager.accent1
            font.pixelSize: appearanceTab.settings.mediumFontSize
            font.bold: true
        }

        Rectangle {
            width: parent.width
            height: 40
            color: appearanceTab.themeManager.overlay
            border.color: wallpaperInput.activeFocus ? appearanceTab.themeManager.accent1 : appearanceTab.themeManager.surface
            border.width: 1
            radius: appearanceTab.settings.radius

            TextInput {
                id: wallpaperInput
                anchors.fill: parent
                anchors.margins: 10
                text: appearanceTab.settings.wallpaperPath
                color: appearanceTab.themeManager.text
                font.pixelSize: 14
                verticalAlignment: TextInput.AlignVCenter
                selectByMouse: true

                onTextChanged: {
                    appearanceTab.settings.wallpaperPath = text;
                    appearanceTab.settings.saveSettings();
                }
            }

            Text {
                visible: wallpaperInput.text.length === 0
                text: "Ruta de imagen (ej: /home/user/wallpaper.jpg) o vacío para color surface"
                color: appearanceTab.themeManager.muted
                font.pixelSize: 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 10
            }
        }

        Rectangle {
            width: 80
            height: 30
            color: clearButtonArea.containsMouse ? appearanceTab.themeManager.accent1 : appearanceTab.themeManager.surface
            border.color: appearanceTab.themeManager.accent1
            border.width: 1
            radius: appearanceTab.settings.radius
            visible: appearanceTab.settings.wallpaperPath.length > 0

            Text {
                text: "Limpiar"
                color: appearanceTab.themeManager.text
                font.pixelSize: 12
                anchors.centerIn: parent
            }

            MouseArea {
                id: clearButtonArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: {
                    wallpaperInput.text = "";
                }
            }
        }
    }
}
