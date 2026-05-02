import QtQuick

Item {
    id: root

    property bool revealed: false
    default property alias contentData: defaultSlot.data
    property Component revealContent

    implicitWidth: defaultSlot.childrenRect.width
    implicitHeight: defaultSlot.childrenRect.height
    width: implicitWidth
    height: implicitHeight

    Item {
        id: defaultSlot
        width: childrenRect.width
        height: childrenRect.height
        visible: !root.revealed
    }

    Loader {
        id: revealLoader
        anchors.centerIn: parent
        sourceComponent: root.revealContent
        visible: root.revealed
    }
}
