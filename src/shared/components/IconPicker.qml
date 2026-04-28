import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as Controls
import qs.src.shared.theme

Item {
    id: root

    property var iconProvider: null

    property string currentIcon: ""

    signal iconSelected(string name)

    property string searchQuery: ""

    readonly property var filteredIcons: {
        const list = root.iconProvider ? root.iconProvider.iconList : [];
        const q = root.searchQuery.toLowerCase().trim();
        if (q === "")
            return list;
        return list.filter(n => n.includes(q));
    }

    readonly property color bg: Theme.surface
    readonly property color borderColor: Theme.overlay
    readonly property color textColor: Theme.text
    readonly property color muted: Theme.muted
    readonly property color accent: Theme.accent6
    readonly property color hoverColor: Theme.highlightMed
    readonly property color selectedColor: Theme.highlightHigh

    readonly property int spacing: Theme.spacing
    readonly property int spacingXs: Math.round(Theme.spacing / 2)
    readonly property int radius: Theme.radius

    ColumnLayout {
        anchors.fill: parent
        spacing: root.spacing

        Rectangle {
            Layout.fillWidth: true
            height: 32
            radius: root.radius
            color: root.bg
            border.color: searchInput.activeFocus ? root.accent : root.borderColor
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: root.spacing
                anchors.rightMargin: root.spacing
                spacing: root.spacingXs

                LucideIcon {
                    iconProvider: root.iconProvider
                    name: "search"
                    size: 14
                    accent: "muted"
                }

                Controls.TextField {
                    id: searchInput
                    Layout.fillWidth: true
                    placeholderText: "Search icons..."
                    placeholderTextColor: root.muted
                    color: root.textColor
                    font.pixelSize: 12
                    background: null
                    onTextChanged: root.searchQuery = text
                }

                LucideIcon {
                    iconProvider: root.iconProvider
                    name: "x"
                    size: 14
                    accent: "muted"
                    visible: searchInput.text !== ""

                    MouseArea {
                        anchors.fill: parent
                        cursorShape: Qt.PointingHandCursor
                        onClicked: searchInput.text = ""
                    }
                }
            }
        }

        Text {
            visible: text !== ""
            text: {
                if (!root.iconProvider)
                    return "";
                if (root.iconProvider.iconListLoading)
                    return "Fetching icon list...";
                if (root.iconProvider.iconList.length === 0)
                    return "No icons available";
                const n = root.filteredIcons.length;
                return n + " icon" + (n !== 1 ? "s" : "");
            }
            color: root.muted
            font.pixelSize: 11
            leftPadding: 2
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: root.filteredIcons
            cellWidth: 52
            cellHeight: 52
            clip: true

            onModelChanged: scrollToSelected()
            Component.onCompleted: scrollToSelected()

            function scrollToSelected() {
                if (root.currentIcon === "")
                    return;
                const idx = root.filteredIcons.indexOf(root.currentIcon);
                if (idx >= 0)
                    Qt.callLater(() => grid.positionViewAtIndex(idx, GridView.Center));
            }

            delegate: Rectangle {
                required property string modelData
                required property int index

                property bool isSelected: root.currentIcon === modelData
                property bool isHovered: false

                width: grid.cellWidth
                height: grid.cellHeight

                color: isSelected ? root.selectedColor : (isHovered ? root.hoverColor : "transparent")
                radius: root.radius

                LucideIcon {
                    anchors.centerIn: parent
                    iconProvider: root.iconProvider
                    name: modelData
                    size: 22
                    accent: isSelected ? "accent6" : "text"
                }

                Controls.ToolTip {
                    visible: parent.isHovered
                    text: modelData
                    delay: 500
                }

                MouseArea {
                    anchors.fill: parent
                    hoverEnabled: true
                    cursorShape: Qt.PointingHandCursor
                    onEntered: parent.isHovered = true
                    onExited: parent.isHovered = false
                    onClicked: root.iconSelected(modelData)
                }
            }
        }
    }
}
