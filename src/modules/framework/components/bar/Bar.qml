pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: bar

    property string position
    property BarSizes barSizes
    property var themeProvider: null

    property color color

    property Component content

    readonly property int gap: themeProvider?.spacing?.lg ?? 16
    readonly property int radius: themeProvider?.radius?.md ?? 8
    readonly property int padding: themeProvider?.spacing?.xs ?? 4

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"

            anchors {
                top: bar.position === "top" || bar.position === "left" || bar.position === "right"
                bottom: bar.position === "bottom" || bar.position === "left" || bar.position === "right"
                left: bar.position === "left" || bar.position === "top" || bar.position === "bottom"
                right: bar.position === "right" || bar.position === "top" || bar.position === "bottom"
            }

            margins {
                top: (bar.position === "left" || bar.position === "right" ? bar.barSizes.top + bar.gap : 0)
                bottom: (bar.position === "left" || bar.position === "right" ? bar.barSizes.bottom + bar.gap : 0)
                left: (bar.position === "top" || bar.position === "bottom" ? bar.gap : 0)
                right: (bar.position === "top" || bar.position === "bottom" ? bar.gap : 0)
            }

            implicitWidth: bar.position === "left" || bar.position === "right" ? bar.barSizes[bar.position] : modelData.width
            implicitHeight: bar.position === "top" || bar.position === "bottom" ? bar.barSizes[bar.position] : modelData.height

            Rectangle {
                anchors.fill: parent
                radius: bar.radius
                color: bar.color
            }

            Loader {
                anchors.fill: parent
                anchors.margins: bar.padding
                active: bar.barSizes[bar.position] !== bar.barSizes.inactive
                sourceComponent: bar.content
            }
        }
    }
}
