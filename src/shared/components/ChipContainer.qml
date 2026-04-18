import QtQuick

Item {
    id: root

    default property alias contentData: slot.data

    property var themeProvider: null
    property var drawerState: null
    property string barPosition: ""
    property int barIndex: 0
    property color accentColor: themeProvider?.accent ?? "#c4a7e7"
    property string variant: "ghost"
    property string panelUrl: ""

    readonly property int pad: themeProvider?.spacing?.sm ?? 8
    readonly property int r: themeProvider?.radius?.md ?? 8
    readonly property bool hovered: area.containsMouse
    readonly property bool pressed: area.pressed
    readonly property bool interactive: panelUrl !== ""

    readonly property color fgColor: variant === "filled" ? (themeProvider?.base ?? "#191724") : accentColor

    implicitWidth: slot.childrenRect.width + pad * 2
    implicitHeight: slot.childrenRect.height + pad * 2

    property var panelComponent: null

    Component.onCompleted: {
        if (panelUrl === "")
            return;
        const c = Qt.createComponent(panelUrl);
        if (c.status === Component.Ready) {
            panelComponent = c;
        } else {
            c.statusChanged.connect(function () {
                if (c.status === Component.Ready)
                    root.panelComponent = c;
            });
        }
    }

    Rectangle {
        id: chip
        anchors.centerIn: parent
        width: slot.childrenRect.width + root.pad * 2
        height: slot.childrenRect.height + root.pad * 2
        radius: root.r

        readonly property color bg: {
            if (root.variant === "filled") {
                if (root.pressed)
                    return Qt.darker(root.accentColor, 1.15);
                if (root.hovered)
                    return Qt.lighter(root.accentColor, 1.12);
                return root.accentColor;
            }
            if (root.pressed)
                return root.themeProvider?.highlightMed ?? "#403d52";
            if (root.hovered && root.interactive)
                return root.themeProvider?.highlightLow ?? "#21202e";
            return "transparent";
        }

        color: bg

        Behavior on color {
            ColorAnimation {
                duration: 80
            }
        }
    }

    Item {
        id: slot
        anchors.centerIn: parent
        width: childrenRect.width
        height: childrenRect.height
    }

    MouseArea {
        id: area
        anchors.fill: chip
        hoverEnabled: root.interactive
        cursorShape: root.interactive ? Qt.PointingHandCursor : Qt.ArrowCursor
        onClicked: {
            if (root.panelComponent && root.drawerState)
                root.drawerState.openDrawer(root.barPosition, root.barIndex, root.panelComponent, {});
        }
    }
}
