pragma ComponentBehavior: Bound

import QtQuick

Item {
    id: root

    property var pluginState: null
    property var popupComp: null
    property var themeProvider: null
    property var iconProvider: null
    property int popupTimeout: 5000
    property int maxVisible: 5

    readonly property int spacing: themeProvider?.spacing ?? 8
    implicitHeight: column.implicitHeight

    Column {
        id: column
        width: parent.width
        spacing: root.spacing

        Repeater {
            model: root.pluginState?.activeList ?? []

            delegate: Item {
                required property var notif
                required property int index

                visible: index < root.maxVisible
                width: column.width
                height: _popup ? _popup.implicitHeight : 0

                property var _popup: null

                Component.onCompleted: {
                    if (!root.popupComp || index >= root.maxVisible)
                        return;
                    const self = this;
                    self._popup = root.popupComp.createObject(self, {
                        notif: self.notif,
                        pluginState: root.pluginState,
                        popupTimeout: root.popupTimeout,
                        themeProvider: Qt.binding(function () {
                            return root.themeProvider;
                        }),
                        iconProvider: Qt.binding(function () {
                            return root.iconProvider;
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
