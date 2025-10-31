pragma ComponentBehavior: Bound

import QtQuick

Column {
    id: slider

    required property var themeManager
    required property var settings
    required property int value
    required property int minValue
    required property int maxValue
    required property string unit  // e.g., "px", "%", ""

    signal sliderValueChanged(int newValue)

    spacing: 8

    Row {
        width: parent.width
        spacing: 12

        Rectangle {
            width: parent.width - decrementBtn.width - incrementBtn.width - 24
            height: 6
            color: slider.themeManager.surface
            radius: 3
            anchors.verticalCenter: parent.verticalCenter

            Rectangle {
                width: (parent.width * (slider.value - slider.minValue)) / (slider.maxValue - slider.minValue)
                height: parent.height
                color: slider.themeManager.accent1
                radius: parent.radius
            }

            MouseArea {
                anchors.fill: parent
                onClicked: function (mouse) {
                    const ratio = mouse.x / width;
                    const range = slider.maxValue - slider.minValue;
                    const newValue = Math.round(slider.minValue + (ratio * range));
                    slider.sliderValueChanged(Math.max(slider.minValue, Math.min(slider.maxValue, newValue)));
                }
            }
        }

        Rectangle {
            id: decrementBtn
            width: 30
            height: 24
            color: decrementArea.containsMouse ? slider.themeManager.surface : slider.themeManager.overlay
            border.color: slider.themeManager.muted
            border.width: 1
            radius: 4

            Text {
                text: "−"
                color: slider.themeManager.text
                font.pixelSize: 16
                anchors.centerIn: parent
            }

            MouseArea {
                id: decrementArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: {
                    if (slider.value > slider.minValue) {
                        slider.sliderValueChanged(slider.value - 1);
                    }
                }
            }
        }

        Rectangle {
            id: incrementBtn
            width: 30
            height: 24
            color: incrementArea.containsMouse ? slider.themeManager.surface : slider.themeManager.overlay
            border.color: slider.themeManager.muted
            border.width: 1
            radius: 4

            Text {
                text: "+"
                color: slider.themeManager.text
                font.pixelSize: 16
                anchors.centerIn: parent
            }

            MouseArea {
                id: incrementArea
                anchors.fill: parent
                hoverEnabled: true
                onClicked: {
                    if (slider.value < slider.maxValue) {
                        slider.sliderValueChanged(slider.value + 1);
                    }
                }
            }
        }
    }
}
