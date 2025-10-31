pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

Column {
    id: generalTab

    required property var themeManager
    required property var settings

    spacing: settings.spacing

    Text {
        text: "Configuración General"
        color: generalTab.themeManager.accent1
        font.pixelSize: generalTab.settings.mediumFontSize
        font.bold: true
    }

    Rectangle {
        width: parent.width
        height: 80
        color: generalTab.themeManager.overlay
        border.color: generalTab.themeManager.muted
        border.width: 1
        radius: generalTab.settings.radius

        Column {
            anchors.fill: parent
            anchors.margins: generalTab.settings.spacing
            spacing: 4

            Text {
                text: "Información del Sistema"
                color: generalTab.themeManager.text
                font.pixelSize: 13
                font.bold: true
            }

            Text {
                text: "• Shell: QuickShell"
                color: generalTab.themeManager.subtle
                font.pixelSize: 12
            }

            Text {
                text: "• Tema activo: " + (generalTab.themeManager.getThemeMeta(generalTab.themeManager.currentThemeName) ? generalTab.themeManager.getThemeMeta(generalTab.themeManager.currentThemeName).name : "Unknown")
                color: generalTab.themeManager.subtle
                font.pixelSize: 12
            }
        }
    }

    Text {
        text: "Atajos de Teclado"
        color: generalTab.themeManager.accent1
        font.pixelSize: generalTab.settings.mediumFontSize
        font.bold: true
    }

    Rectangle {
        width: parent.width
        height: 60
        color: generalTab.themeManager.overlay
        border.color: generalTab.themeManager.muted
        border.width: 1
        radius: generalTab.settings.radius

        Column {
            anchors.fill: parent
            anchors.margins: generalTab.settings.spacing
            spacing: 6

            Row {
                spacing: 10
                Text {
                    text: "Abrir configuración:"
                    color: generalTab.themeManager.text
                    font.pixelSize: 12
                    width: 150
                }
                Rectangle {
                    width: 80
                    height: 22
                    color: generalTab.themeManager.surface
                    radius: 4
                    Text {
                        text: "Super + S"
                        color: generalTab.themeManager.accent1
                        font.pixelSize: 11
                        anchors.centerIn: parent
                    }
                }
            }
        }
    }
}
