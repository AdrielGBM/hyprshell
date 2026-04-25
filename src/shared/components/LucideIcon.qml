import QtQuick

Item {
    id: root

    property var iconProvider: null

    property string name: ""

    property real size: 16

    implicitWidth: size
    implicitHeight: size

    property var themeProvider: null
    property string accent: "text"

    readonly property color color: themeProvider ? themeProvider[accent] : "transparent"

    width: size
    height: size

    property string dataUri: ""
    property bool failed: false

    Image {
        id: img
        anchors.fill: parent
        source: root.dataUri
        sourceSize.width: root.size
        sourceSize.height: root.size
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
        root.refreshUri();
        root.requestIcon();
    }

    onColorChanged: root.refreshUri()

    onIconProviderChanged: root.requestIcon()

    Connections {
        target: root.iconProvider

        function onIconReady(iconName) {
            if (iconName === root.name)
                root.refreshUri();
        }

        function onIconFailed(iconName) {
            if (iconName === root.name)
                root.failed = true;
        }
    }

    Component.onCompleted: root.requestIcon()

    function requestIcon() {
        if (!root.iconProvider || root.name === "")
            return;
        root.iconProvider.request(root.name);
    }

    function refreshUri() {
        if (!root.iconProvider || root.name === "") {
            root.dataUri = "";
            return;
        }
        root.dataUri = root.iconProvider.getDataUri(root.name, root.color);
    }
}
