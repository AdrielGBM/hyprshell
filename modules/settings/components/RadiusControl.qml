pragma ComponentBehavior: Bound

import QtQuick
import "../../../core/components/inputs"

Column {
    id: radiusControl

    required property var themeManager
    required property var settings

    spacing: settings.spacing

    Text {
        text: "Radio de Bordes"
        color: radiusControl.themeManager.accent1
        font.pixelSize: radiusControl.settings.mediumFontSize
        font.bold: true
    }

    Rectangle {
        width: parent.width
        height: 80
        color: radiusControl.themeManager.overlay
        border.color: radiusControl.themeManager.muted
        border.width: 1
        radius: radiusControl.settings.radius

        Column {
            anchors.fill: parent
            anchors.margins: radiusControl.settings.spacing
            spacing: 8

            Row {
                width: parent.width
                spacing: 12

                Text {
                    text: "Valor actual:"
                    color: radiusControl.themeManager.text
                    font.pixelSize: 13
                    anchors.verticalCenter: parent.verticalCenter
                }

                Rectangle {
                    width: 40
                    height: 24
                    color: radiusControl.themeManager.surface
                    radius: 4

                    Text {
                        text: radiusControl.settings.radius + "px"
                        color: radiusControl.themeManager.accent1
                        font.pixelSize: 12
                        font.bold: true
                        anchors.centerIn: parent
                    }
                }
            }

            Slider {
                width: parent.width
                themeManager: radiusControl.themeManager
                settings: radiusControl.settings
                value: radiusControl.settings.radius
                minValue: 0
                maxValue: 20
                unit: "px"
                onSliderValueChanged: function (newValue) {
                    radiusControl.settings.radius = newValue;
                }
            }
        }
    }

    Text {
        text: "• Controla el redondeo de las esquinas de los componentes"
        color: radiusControl.themeManager.subtle
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        width: parent.width
    }
}
