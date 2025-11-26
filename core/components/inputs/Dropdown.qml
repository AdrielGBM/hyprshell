pragma ComponentBehavior: Bound

import QtQuick

Rectangle {
    id: dropdown

    required property var themeProvider
    required property var settings
    required property string currentValue
    required property var options  // Array de objetos: [{key: "id", name: "Display Name", description: "Optional"}]

    signal valueChanged(string newValue)

    width: parent.width
    height: 40
    color: dropdownArea.containsMouse ? themeProvider.surface : themeProvider.overlay
    border.color: isOpen ? themeProvider.accent1 : themeProvider.surface
    border.width: 1
    radius: settings.radius

    property bool isOpen: false
    property string displayText: {
        const option = options.find(opt => opt.key === currentValue);
        return option ? option.name : currentValue;
    }

    Row {
        anchors.fill: parent
        anchors.margins: 10
        anchors.rightMargin: 15
        spacing: 0

        Text {
            text: dropdown.displayText
            color: dropdown.themeProvider.text
            font.pixelSize: 14
            anchors.verticalCenter: parent.verticalCenter
        }

        Item {
            width: parent.width - parent.children[0].width - parent.children[2].width - 20
            height: parent.height
        }

        Text {
            text: dropdown.isOpen ? "▲" : "▼"
            color: dropdown.themeProvider.accent1
            font.pixelSize: 12
            anchors.verticalCenter: parent.verticalCenter
        }
    }

    MouseArea {
        id: dropdownArea
        anchors.fill: parent
        hoverEnabled: true
        onClicked: dropdown.isOpen = !dropdown.isOpen
    }

    Rectangle {
        id: dropdownList
        anchors.top: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.topMargin: 2
        height: dropdown.isOpen ? optionsColumn.height + 8 : 0
        color: dropdown.themeProvider.overlay
        border.color: dropdown.themeProvider.muted
        border.width: dropdown.isOpen ? 1 : 0
        radius: 8
        visible: dropdown.isOpen
        z: 10

        Behavior on height {
            NumberAnimation {
                duration: 150
            }
        }

        Column {
            id: optionsColumn
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.margins: 4

            Repeater {
                model: dropdown.options

                Rectangle {
                    required property var modelData
                    required property int index
                    width: optionsColumn.width
                    height: 35
                    color: optionArea.containsMouse ? dropdown.themeProvider.accent1 : "transparent"
                    radius: 6

                    Text {
                        text: parent.modelData.name
                        color: optionArea.containsMouse ? dropdown.themeProvider.base : dropdown.themeProvider.text
                        font.pixelSize: 13
                        anchors.left: parent.left
                        anchors.leftMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    Rectangle {
                        visible: parent.modelData.key === dropdown.currentValue
                        width: 8
                        height: 8
                        radius: 4
                        color: dropdown.themeProvider.accent1
                        anchors.right: parent.right
                        anchors.rightMargin: 10
                        anchors.verticalCenter: parent.verticalCenter
                    }

                    MouseArea {
                        id: optionArea
                        anchors.fill: parent
                        hoverEnabled: true
                        onClicked: {
                            dropdown.valueChanged(parent.modelData.key);
                            dropdown.isOpen = false;
                        }
                    }
                }
            }
        }
    }
}
