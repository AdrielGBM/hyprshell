import QtQuick

Item {
    id: root

    property var iconProvider: null

    property string name: ""

    property real size: 16

    property color color: "#ffffff"

    width: size
    height: size

    property string _dataUri: ""
    property bool _failed: false

    Image {
        id: img
        anchors.fill: parent
        source: root._dataUri
        sourceSize.width: root.size
        sourceSize.height: root.size
        fillMode: Image.PreserveAspectFit
        smooth: true
        asynchronous: true
        visible: status === Image.Ready
    }

    Rectangle {
        anchors.fill: parent
        visible: img.status !== Image.Ready && root.name !== ""
        color: "transparent"
        border.color: root.color
        border.width: 1
        opacity: root._failed ? 0.15 : 0.25
        radius: 2
    }

    onNameChanged: {
        root._dataUri = "";
        root._failed = false;
        root._requestIcon();
    }

    onColorChanged: root._refreshUri()
    onIconProviderChanged: root._requestIcon()

    Connections {
        target: root.iconProvider

        function onIconReady(iconName) {
            if (iconName === root.name)
                root._refreshUri();
        }

        function onIconFailed(iconName) {
            if (iconName === root.name)
                root._failed = true;
        }
    }

    Component.onCompleted: root._requestIcon()

    function _requestIcon() {
        if (!root.iconProvider || root.name === "")
            return;
        root.iconProvider.request(root.name);
    }

    function _refreshUri() {
        if (!root.iconProvider || root.name === "") {
            root._dataUri = "";
            return;
        }
        root._dataUri = root.iconProvider.getDataUri(root.name, root.color);
    }
}
