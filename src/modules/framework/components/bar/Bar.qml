pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: bar

    property string position: "top"
    property BarSizes barSizes

    property Component content

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData

            anchors {
                top: bar.position === "top" || bar.position === "left" || bar.position === "right"
                bottom: bar.position === "bottom" || bar.position === "left" || bar.position === "right"
                left: bar.position === "left" || bar.position === "top" || bar.position === "bottom"
                right: bar.position === "right" || bar.position === "top" || bar.position === "bottom"
            }

            margins {
                top: bar.position === "left" || bar.position === "right" ? bar.barSizes.top : 0
                bottom: bar.position === "left" || bar.position === "right" ? bar.barSizes.bottom : 0
                //left: bar.position === "top" || bar.position === "bottom" ? bar.barSizes.left : 0
                //right: bar.position === "top" || bar.position === "bottom" ? bar.barSizes.right : 0
            }

            width: bar.barSizes[bar.position]
            height: bar.barSizes[bar.position]

            Loader {
                anchors.fill: parent
                active: bar.barSizes[bar.position] !== bar.barSizes.inactive
                sourceComponent: bar.content
            }
        }
    }
}
