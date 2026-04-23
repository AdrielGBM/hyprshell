pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: root

    property var pluginState: null
    property var popupComp: null
    property var themeProvider: null
    property int popupTimeout: 5000
    property int maxVisible: 5

    readonly property int spacing: themeProvider?.spacing ?? 8
    implicitHeight: col.implicitHeight

    Column {
        id: col
        width: parent.width
        spacing: root.spacing

        Repeater {
            model: root.pluginState?.activeList ?? []

            Item {
                required property var notif
                required property int index

                visible: index < root.maxVisible
                width: parent.width
                height: _popup ? _popup.height : 0

                property var _popup: null

                Component.onCompleted: {
                    if (!root.popupComp)
                        return;
                    const self = this;
                    self._popup = root.popupComp.createObject(self, {
                        notif: self.notif,
                        pluginState: root.pluginState,
                        popupTimeout: root.popupTimeout,
                        themeProvider: Qt.binding(function () {
                            return root.themeProvider;
                        }),
                        width: Qt.binding(function () {
                            return self.width;
                        })
                    });
                    if (self._popup)
                        self._popup.height = Qt.binding(function () {
                            return self._popup.implicitHeight;
                        });
                }
            }
        }
    }
}
