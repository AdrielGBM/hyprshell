import QtQuick
import QtQuick.Layouts
import QtQuick.Controls.Basic as Controls

Item {
    id: root

    property var iconProvider: null
    property var themeProvider: null

    property string currentIcon: ""

    signal iconSelected(string name)

    property string _searchQuery: ""

    readonly property var _filteredIcons: {
        const list = root.iconProvider ? root.iconProvider.iconList : [];
        const q = root._searchQuery.toLowerCase().trim();
        if (q === "")
            return list;
        return list.filter(n => n.includes(q));
    }

    readonly property color _bg: root.themeProvider ? root.themeProvider.surface : "#1f1d2e"
    readonly property color _border: root.themeProvider ? root.themeProvider.overlay : "#26233a"
    readonly property color _text: root.themeProvider ? root.themeProvider.text : "#e0def4"
    readonly property color _muted: root.themeProvider ? root.themeProvider.muted : "#6e6a86"
    readonly property color _accent: root.themeProvider ? root.themeProvider.accent6 : "#c4a7e7"
    readonly property color _hover: root.themeProvider ? root.themeProvider.highlightMed : "#403d52"
    readonly property color _selected: root.themeProvider ? root.themeProvider.highlightHigh : "#524f67"

    ColumnLayout {
        anchors.fill: parent
        spacing: 8

        Rectangle {
            Layout.fillWidth: true
            height: 32
            radius: 6
            color: root._bg
            border.color: searchInput.activeFocus ? root._accent : root._border
            border.width: 1

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 8
                anchors.rightMargin: 8
                spacing: 6

                LucideIcon {
                    iconProvider: root.iconProvider
                    name: "search"
                    size: 14
                    color: root._muted
                }

                Controls.TextField {
                    id: searchInput
                    Layout.fillWidth: true
                    placeholderText: "Search icons..."
                    placeholderTextColor: root._muted
                    color: root._text
                    font.pixelSize: 12
                    background: null
                    onTextChanged: root._searchQuery = text
                }

                LucideIcon {
                    iconProvider: root.iconProvider
                    name: "x"
                    size: 14
                    color: root._muted
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
                const n = root._filteredIcons.length;
                return n + " icon" + (n !== 1 ? "s" : "");
            }
            color: root._muted
            font.pixelSize: 11
            leftPadding: 2
        }

        GridView {
            id: grid
            Layout.fillWidth: true
            Layout.fillHeight: true
            model: root._filteredIcons
            cellWidth: 52
            cellHeight: 52
            clip: true

            onModelChanged: _scrollToSelected()
            Component.onCompleted: _scrollToSelected()

            function _scrollToSelected() {
                if (root.currentIcon === "")
                    return;
                const idx = root._filteredIcons.indexOf(root.currentIcon);
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

                color: isSelected ? root._selected : (isHovered ? root._hover : "transparent")
                radius: 6

                LucideIcon {
                    anchors.centerIn: parent
                    iconProvider: root.iconProvider
                    name: modelData
                    size: 22
                    color: isSelected ? root._accent : root._text
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
