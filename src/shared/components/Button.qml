import QtQuick
import QtQuick.Layouts
import Quickshell.Io

Rectangle {
    id: root

    default property alias contentData: slot.data

    property var themeProvider: null
    property var iconProvider: null

    property string text: ""
    property string icon: ""

    property var command: []
    property bool detach: true

    property string variant: "filled"
    property string size: "medium"
    property string accent: "accent6"
    property color accentColor: themeProvider ? (themeProvider[accent] ?? themeProvider.accent6) : "transparent"
    signal clicked

    readonly property int hPad: size === "small" ? 8 : size === "large" ? 16 : 12
    readonly property int vPad: size === "small" ? 4 : size === "large" ? 10 : 6
    readonly property int iconSize: size === "small" ? 12 : size === "large" ? 16 : 14
    readonly property int fontSize: {
        const base = themeProvider?.font?.size ?? 12;
        if (size === "small")
            return base - 1;
        if (size === "large")
            return base + 1;
        return base;
    }
    readonly property int gap: size === "small" ? 4 : size === "large" ? 8 : 6
    readonly property int r: themeProvider?.radius?.md ?? 6

    readonly property bool hasCustomContent: slot.children.length > 0

    readonly property bool hovered: area.containsMouse && enabled
    readonly property bool pressed: area.pressed && enabled

    readonly property color bg: {
        if (pressed)
            return variant === "filled" ? Qt.darker(root.accentColor, 1.15) : themeProvider?.highlightMed;
        if (hovered)
            return variant === "filled" ? Qt.lighter(root.accentColor, 1.12) : themeProvider?.highlightLow;
        if (variant === "filled")
            return root.accentColor;
        return "transparent";
    }

    readonly property color borderColor: variant === "outlined" ? (hovered ? root.accentColor : themeProvider?.overlay) : "transparent"

    readonly property color fgColor: {
        if (variant === "filled")
            return themeProvider?.base;
        return hovered ? themeProvider?.text : themeProvider?.subtle;
    }

    implicitHeight: hasCustomContent ? slot.implicitHeight + vPad * 2 : defaultLayout.implicitHeight + vPad * 2

    implicitWidth: hasCustomContent ? slot.implicitWidth + hPad * 2 : defaultLayout.implicitWidth + hPad * 2

    color: bg
    border.color: borderColor
    border.width: variant === "outlined" ? 1 : 0
    radius: r
    opacity: enabled ? 1.0 : 0.4
    clip: true

    Behavior on color {
        ColorAnimation {
            duration: 80
        }
    }
    Behavior on border.color {
        ColorAnimation {
            duration: 80
        }
    }

    RowLayout {
        id: defaultLayout
        visible: !root.hasCustomContent
        anchors.centerIn: parent
        spacing: root.gap

        LucideIcon {
            visible: root.icon !== ""
            iconProvider: root.iconProvider
            themeProvider: root.themeProvider
            name: root.icon
            size: root.iconSize
            accent: root.variant === "filled" ? "base" : (root.hovered ? "text" : "subtle")
        }

        Text {
            visible: root.text !== ""
            text: root.text
            color: root.fgColor
            font.pixelSize: root.fontSize
            font.family: root.themeProvider?.font?.family ?? ""
            font.weight: Font.Medium
            verticalAlignment: Text.AlignVCenter
            Behavior on color {
                ColorAnimation {
                    duration: 80
                }
            }
        }
    }

    Item {
        id: slot
        visible: root.hasCustomContent
        anchors {
            fill: parent
            leftMargin: root.hPad
            rightMargin: root.hPad
            topMargin: root.vPad
            bottomMargin: root.vPad
        }
    }

    MouseArea {
        id: area
        anchors.fill: parent
        enabled: root.enabled
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            root.clicked();
            if (root.command.length > 0)
                process.run();
        }
    }

    property Process process: Process {
        running: false
        function run() {
            if (root.command.length === 0)
                return;
            if (root.detach) {
                const quoted = root.command.map(a => "'" + a.replace(/'/g, "'\\''") + "'").join(" ");
                command = ["sh", "-c", quoted + " &"];
            } else {
                command = root.command;
            }
            running = true;
        }
        onExited: code => {
            if (code !== 0 && !root.detach)
                console.warn("Button: command exited with code", code, root.command);
        }
    }
}
