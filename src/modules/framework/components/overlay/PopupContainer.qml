pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: container

    property var themeProvider: null
    property var iconProvider: null
    property var contentComponent: null
    property var popupData: null

    signal requestDismiss

    readonly property int pad: themeProvider?.spacing ?? 8
    readonly property int r: themeProvider?.radius ?? 8

    implicitHeight: contentLoader.implicitHeight + pad * 2
    height: implicitHeight
    clip: true

    Rectangle {
        anchors.fill: parent
        radius: container.r
        color: container.themeProvider?.surface ?? "#1f1d2e"
        border.color: container.themeProvider?.overlay ?? "#26233a"
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
        property: "themeProvider"
        value: container.themeProvider
        when: contentLoader.item !== null
        restoreMode: Binding.RestoreNone
    }
    Binding {
        target: contentLoader.item
        property: "iconProvider"
        value: container.iconProvider
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
