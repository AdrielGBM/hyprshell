import QtQuick

Item {
    id: root

    default property alias contentData: slot.data

    property var themeProvider: null
    property var i18nProvider: null
    property var drawerState: null
    property string barPosition: ""
    property int barIndex: 0

    property string accent: ""
    property string variant: "default"
    readonly property string effectiveVariant: variant !== "default" ? variant : (themeProvider?.defaultVariant ?? "default")
    readonly property string effectiveAccentToken: {
        if (accent !== "")
            return accent;
        const def = themeProvider?.defaultAccent ?? "";
        if (def !== "")
            return def;
        return "text";
    }
    readonly property color accentColor: {
        if (accent !== "")
            return themeProvider?.[accent] ?? "transparent";
        const def = themeProvider?.defaultAccent ?? "";
        if (def !== "")
            return themeProvider?.[def] ?? "transparent";
        return effectiveVariant === "filled" ? themeProvider?.accent1 : themeProvider?.text;
    }
    readonly property string iconAccent: effectiveVariant === "filled" ? (themeProvider?.foregroundTokenFor(accentColor) ?? "base") : effectiveAccentToken
    property var barScreen: null
    property string panelUrl: ""

    property int chipRadius: -1
    readonly property int pad: themeProvider?.spacing
    readonly property int r: chipRadius >= 0 ? chipRadius : themeProvider?.radius
    readonly property bool isVertical: barPosition === "left" || barPosition === "right"
    readonly property bool hovered: area.containsMouse
    readonly property bool pressed: area.pressed
    readonly property bool interactive: panelUrl !== ""

    readonly property color fgColor: effectiveVariant === "filled" ? (themeProvider?.foregroundFor(accentColor) ?? themeProvider?.base) : accentColor

    readonly property real effectivePad: isVertical ? Math.max(0, (width - slot.childrenRect.width) / 2) : Math.max(0, (height - slot.childrenRect.height) / 2)

    implicitWidth: isVertical ? slot.childrenRect.width + pad * 2 : slot.childrenRect.width + effectivePad * 2
    implicitHeight: isVertical ? slot.childrenRect.height + effectivePad * 2 : slot.childrenRect.height + pad * 2

    property var panelComponent: null
    property var panelProps: ({})

    Component.onCompleted: {
        if (panelUrl === "")
            return;
        const c = Qt.createComponent(panelUrl);
        if (c.status === Component.Ready) {
            panelComponent = c;
        } else {
            const handler = function () {
                if (c.status === Component.Ready) {
                    c.statusChanged.disconnect(handler);
                    root.panelComponent = c;
                } else if (c.status === Component.Error) {
                    c.statusChanged.disconnect(handler);
                }
            };
            c.statusChanged.connect(handler);
        }
    }

    Rectangle {
        id: chip
        anchors.centerIn: parent
        width: root.isVertical ? root.width : slot.childrenRect.width + root.effectivePad * 2
        height: root.isVertical ? slot.childrenRect.height + root.effectivePad * 2 : root.height
        radius: root.r

        readonly property color bg: {
            if (root.effectiveVariant === "filled") {
                if (root.pressed)
                    return Qt.darker(root.accentColor, 1.15);
                if (root.hovered)
                    return Qt.lighter(root.accentColor, 1.12);
                return root.accentColor;
            }
            if (root.pressed)
                return root.themeProvider?.highlightMed;
            if (root.hovered && root.interactive)
                return root.themeProvider?.highlightLow;
            return "transparent";
        }

        color: bg
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
                root.drawerState.openDrawer(root.barPosition, root.barIndex, root.panelComponent, root.panelProps, undefined, root.barScreen);
        }
    }
}
