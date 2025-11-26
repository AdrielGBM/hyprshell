pragma ComponentBehavior: Bound

import QtQuick

Column {
    id: appearanceTab

    required property var themeProvider
    required property var settings

    spacing: settings.spacing

    ThemeSelector {
        width: parent.width
        themeProvider: appearanceTab.themeProvider
        settings: appearanceTab.settings
    }

    RadiusControl {
        width: parent.width
        themeProvider: appearanceTab.themeProvider
        settings: appearanceTab.settings
    }

    Column {
        width: parent.width
        spacing: 8

        Text {
            text: "Fondo de pantalla"
            color: appearanceTab.themeProvider.accent1
            font.pixelSize: appearanceTab.settings.mediumFontSize
            font.bold: true
        }

        Rectangle {
            width: parent.width
            height: 40
            color: appearanceTab.themeProvider.overlay
            border.color: wallpaperInput.activeFocus ? appearanceTab.themeProvider.accent1 : appearanceTab.themeProvider.surface
            border.width: 1
            radius: appearanceTab.settings.radius

            TextInput {
                id: wallpaperInput
                anchors.fill: parent
                anchors.margins: 10
                text: appearanceTab.settings.wallpaperPath
                color: appearanceTab.themeProvider.text
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
                color: appearanceTab.themeProvider.muted
                font.pixelSize: 12
                anchors.verticalCenter: parent.verticalCenter
                anchors.left: parent.left
                anchors.leftMargin: 10
            }
        }

        Rectangle {
            width: 80
            height: 30
            color: clearButtonArea.containsMouse ? appearanceTab.themeProvider.accent1 : appearanceTab.themeProvider.surface
            border.color: appearanceTab.themeProvider.accent1
            border.width: 1
            radius: appearanceTab.settings.radius
            visible: appearanceTab.settings.wallpaperPath.length > 0

            Text {
                text: "Limpiar"
                color: appearanceTab.themeProvider.text
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
