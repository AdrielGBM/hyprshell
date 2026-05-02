import QtQuick
import qs.src.shared.services.theme
import qs.src.shared.services.icons

Item {
    id: root

    property string name: ""

    property real size: Theme.iconSize
    property real strokeWidth: Theme.iconStrokeWidth

    property bool tightViewBox: false
    property var visualBounds: null
    readonly property real visualAspectRatio: visualBounds ? (visualBounds.w / visualBounds.h) : 1.0

    readonly property real effectiveWidth: tightViewBox && visualBounds ? (visualAspectRatio >= 1 ? size : size * visualAspectRatio) : size
    readonly property real effectiveHeight: tightViewBox && visualBounds ? (visualAspectRatio <= 1 ? size : size / visualAspectRatio) : size

    implicitWidth: effectiveWidth
    implicitHeight: effectiveHeight

    property string accent: "text"

    readonly property color color: Theme[accent] ?? "transparent"

    width: effectiveWidth
    height: effectiveHeight

    property string dataUri: ""
    property bool failed: false

    Image {
        id: img
        anchors.fill: parent
        source: root.dataUri
        sourceSize.width: root.effectiveWidth
        sourceSize.height: root.effectiveHeight
        fillMode: Image.PreserveAspectFit
        smooth: true
        asynchronous: true
        cache: false
        visible: status === Image.Ready
    }

    Rectangle {
        anchors.fill: parent
        visible: img.status !== Image.Ready && root.name !== ""
        color: "transparent"
        border.color: root.color
        border.width: 1
        opacity: root.failed ? 0.15 : 0.25
        radius: 2
    }

    onNameChanged: {
        root.failed = false;
        root.visualBounds = null;
        root.refreshUri();
        root.requestIcon();
    }

    onColorChanged: root.refreshUri()
    onStrokeWidthChanged: root.refreshUri()

    Connections {
        target: Icons

        function onIconReady(iconName) {
            if (iconName === root.name) {
                root.visualBounds = Icons.getBBox(iconName);
                root.refreshUri();
            }
        }

        function onIconFailed(iconName) {
            if (iconName === root.name)
                root.failed = true;
        }
    }

    Component.onCompleted: root.requestIcon()

    function requestIcon() {
        if (root.name === "")
            return;
        Icons.request(root.name);
    }

    function refreshUri() {
        if (root.name === "") {
            root.dataUri = "";
            return;
        }
        const bbox = root.tightViewBox ? Icons.getBBox(root.name) : null;
        root.dataUri = Icons.getDataUri(root.name, root.color, root.strokeWidth * (24 / root.size), bbox);
    }
}
