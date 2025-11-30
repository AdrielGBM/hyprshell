pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: bar

    property string position
    property BarSizes barSizes
    property int gap
    property int radius

    property string color

    property Component content

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
                top: (bar.position === "left" || bar.position === "right" ? bar.barSizes.top + bar.gap * 2 : 0)
                bottom: (bar.position === "left" || bar.position === "right" ? bar.barSizes.bottom + bar.gap * 2 : 0)
                left: (bar.position === "top" || bar.position === "bottom" ? bar.gap * 2 : 0)
                right: (bar.position === "top" || bar.position === "bottom" ? bar.gap * 2 : 0)
            }

            width: bar.position === "left" || bar.position === "right" ? bar.barSizes[bar.position] : parent.width
            height: bar.position === "top" || bar.position === "bottom" ? bar.barSizes[bar.position] : parent.height

            Rectangle {
                anchors.fill: parent
                radius: bar.radius
                color: bar.color
            }

            Loader {
                anchors.fill: parent
                active: bar.barSizes[bar.position] !== bar.barSizes.inactive
                sourceComponent: bar.content
            }
        }
    }
}
