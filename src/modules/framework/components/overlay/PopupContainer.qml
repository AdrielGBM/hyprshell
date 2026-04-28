pragma ComponentBehavior: Bound

import QtQuick
import qs.src.shared.theme

Item {
    id: container

    property var contentComponent: null
    property var popupData: null

    signal requestDismiss

    readonly property int pad: Theme.spacing
    readonly property int r: Theme.radius

    implicitHeight: contentLoader.implicitHeight + pad * 2
    height: implicitHeight
    clip: true

    Rectangle {
        anchors.fill: parent
        radius: container.r
        color: Theme.surface
        border.color: Theme.overlay
        border.width: 1
    }

    Loader {
        id: contentLoader

        anchors {
            left: parent.left
            right: parent.right
            top: parent.top
            margins: container.pad
        }

        sourceComponent: container.contentComponent
    }

    Binding {
        target: contentLoader.item
        property: "popupData"
        value: container.popupData
        when: contentLoader.item !== null
        restoreMode: Binding.RestoreNone
    }
    Binding {
        target: contentLoader.item
        property: "width"
        value: contentLoader.width
        when: contentLoader.item !== null
        restoreMode: Binding.RestoreNone
    }

    Connections {
        target: contentLoader.item
        ignoreUnknownSignals: true
        function onRequestDismiss() {
            container.requestDismiss();
        }
    }
}
