pragma ComponentBehavior: Bound

import QtQuick

Rectangle {
    id: sidebarTab

    required property string tabId
    required property string label
    required property string currentTab
    required property var themeManager
    required property var settings

    signal clicked

    width: parent.width
    height: 36
    color: currentTab === tabId ? themeManager.highlightMed : "transparent"
    radius: settings.radius

    Row {
        anchors.fill: parent
        anchors.margins: sidebarTab.settings.spacing
        spacing: sidebarTab.settings.spacing

        Text {
            text: sidebarTab.label
            color: sidebarTab.currentTab === sidebarTab.tabId ? sidebarTab.themeManager.text : sidebarTab.themeManager.muted
            font.pixelSize: sidebarTab.settings.smallFontSize
            font.bold: true
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        onClicked: sidebarTab.clicked()
    }
}
