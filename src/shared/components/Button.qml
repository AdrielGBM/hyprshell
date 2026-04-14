import QtQuick
import QtQuick.Layouts
import Quickshell.Io

Rectangle {
    id: root

    default property alias contentData: _slot.data

    property var themeProvider: null
    property var iconProvider: null

    property string text: ""
    property string icon: ""

    property var command: []
    property bool detach: true

    property string variant: "filled"
    property string size: "medium"
    signal clicked

    readonly property int _hPad: size === "small" ? 8 : size === "large" ? 16 : 12
    readonly property int _vPad: size === "small" ? 4 : size === "large" ? 10 : 6
    readonly property int _iconSize: size === "small" ? 12 : size === "large" ? 16 : 14
    readonly property int _fontSize: size === "small" ? 11 : size === "large" ? 13 : 12
    readonly property int _gap: size === "small" ? 4 : size === "large" ? 8 : 6
    readonly property int _r: themeProvider?.radius?.md ?? 6

    readonly property bool _hasCustomContent: _slot.children.length > 0

    readonly property bool _hovered: _area.containsMouse && enabled
    readonly property bool _pressed: _area.pressed && enabled

    readonly property color _bg: {
        if (_pressed)
            return variant === "filled" ? Qt.darker(themeProvider?.accent6 ?? "#c4a7e7", 1.15) : (themeProvider?.highlightMed ?? "#403d52");
        if (_hovered)
            return variant === "filled" ? Qt.lighter(themeProvider?.accent6 ?? "#c4a7e7", 1.12) : (themeProvider?.highlightLow ?? "#21202e");
        if (variant === "filled")
            return themeProvider?.accent6 ?? "#c4a7e7";
        return "transparent";
    }

    readonly property color _borderColor: variant === "outlined" ? (_hovered ? (themeProvider?.accent6 ?? "#c4a7e7") : (themeProvider?.overlay ?? "#26233a")) : "transparent"

    readonly property color _fgColor: {
        if (variant === "filled")
            return themeProvider?.base ?? "#191724";
        return _hovered ? (themeProvider?.text ?? "#e0def4") : (themeProvider?.subtle ?? "#908caa");
    }

    implicitHeight: _hasCustomContent ? _slot.implicitHeight + _vPad * 2 : _defaultLayout.implicitHeight + _vPad * 2

    implicitWidth: _hasCustomContent ? _slot.implicitWidth + _hPad * 2 : _defaultLayout.implicitWidth + _hPad * 2

    color: _bg
    border.color: _borderColor
    border.width: variant === "outlined" ? 1 : 0
    radius: _r
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
        id: _defaultLayout
        visible: !root._hasCustomContent
        anchors.centerIn: parent
        spacing: root._gap

        LucideIcon {
            visible: root.icon !== ""
            iconProvider: root.iconProvider
            name: root.icon
            size: root._iconSize
            color: root._fgColor
            Behavior on color {
                ColorAnimation {
                    duration: 80
                }
            }
        }

        Text {
            visible: root.text !== ""
            text: root.text
            color: root._fgColor
            font.pixelSize: root._fontSize
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
        id: _slot
        visible: root._hasCustomContent
        anchors {
            fill: parent
            leftMargin: root._hPad
            rightMargin: root._hPad
            topMargin: root._vPad
            bottomMargin: root._vPad
        }
    }

    MouseArea {
        id: _area
        anchors.fill: parent
        enabled: root.enabled
        hoverEnabled: true
        cursorShape: Qt.PointingHandCursor
        onClicked: {
            root.clicked();
            if (root.command.length > 0)
                _process.run();
        }
    }

    property Process _process: Process {
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
