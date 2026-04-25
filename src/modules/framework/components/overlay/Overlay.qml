pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../sideMargins.js" as SideMargins

Scope {
    id: overlay

    required property string side
    required property string align
    required property var items
    required property var barSizes
    required property bool frameMode
    required property var themeProvider
    required property var overlayState
    required property int overlayWidth
    property int maxVisible: 5
    property var iconProvider: null

    readonly property int gap: themeProvider?.spacing ?? 8
    readonly property bool isHorizontal: side === "top" || side === "bottom"

    readonly property var _m: SideMargins.calc(overlay.side, overlay.align, overlay.barSizes, overlay.frameMode, overlay.gap)

    ListModel {
        id: itemsModel
    }

    property var _propsMap: ({})

    Component {
        id: popupGroupComp
        Item {
            id: groupRoot

            property var pluginState: null
            property var popupComp: null
            property int popupTimeout: 5000
            property int maxVisible: 5
            property var themeProvider: null
            property var iconProvider: null
            property var overlayState: null

            readonly property int _spacing: themeProvider?.spacing ?? 8
            implicitHeight: groupColumn.implicitHeight

            Column {
                id: groupColumn
                width: parent.width
                spacing: groupRoot._spacing

                Repeater {
                    model: groupRoot.pluginState?.activeList ?? []

                    Item {
                        required property var notif
                        required property int index

                        visible: index < groupRoot.maxVisible
                        width: groupColumn.width
                        height: visible && _popup ? _popup.implicitHeight : 0

                        property var _popup: null

                        Component.onCompleted: {
                            if (!groupRoot.popupComp)
                                return;
                            const self = this;
                            self._popup = groupRoot.popupComp.createObject(self, {
                                notif: self.notif,
                                pluginState: Qt.binding(function () {
                                    return groupRoot.pluginState;
                                }),
                                popupTimeout: groupRoot.popupTimeout,
                                themeProvider: Qt.binding(function () {
                                    return groupRoot.themeProvider;
                                }),
                                iconProvider: Qt.binding(function () {
                                    return groupRoot.iconProvider;
                                }),
                                overlayState: Qt.binding(function () {
                                    return groupRoot.overlayState;
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
    }

    function _syncItems() {
        const newIds = overlay.items.map(x => x.id);

        const newMap = {};
        for (let i = 0; i < overlay.items.length; i++) {
            const entry = overlay.items[i];
            newMap[entry.id] = entry.props ?? {};
        }
        _propsMap = newMap;

        for (let i = itemsModel.count - 1; i >= 0; i--) {
            if (!newIds.includes(itemsModel.get(i).pluginId))
                itemsModel.remove(i);
        }

        for (let j = 0; j < newIds.length; j++) {
            let found = false;
            for (let k = 0; k < itemsModel.count; k++) {
                if (itemsModel.get(k).pluginId === newIds[j]) {
                    found = true;
                    break;
                }
            }
            if (!found)
                itemsModel.append({
                    pluginId: newIds[j]
                });
        }
    }

    Component.onCompleted: _syncItems()
    onItemsChanged: _syncItems()

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            readonly property string stackVAlign: {
                if (overlay.side === "top" || (!overlay.isHorizontal && overlay.align === "start"))
                    return "top";
                if (overlay.side === "bottom" || (!overlay.isHorizontal && overlay.align === "end"))
                    return "bottom";
                return "center";
            }

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.exclusiveZone: -1

            anchors {
                top: true
                bottom: true
                left: overlay.side === "left" || (overlay.isHorizontal && overlay.align === "start")
                right: overlay.side === "right" || (overlay.isHorizontal && overlay.align === "end")
            }

            margins {
                top: overlay._m.top
                bottom: overlay._m.bottom
                left: overlay._m.left
                right: overlay._m.right
            }

            implicitWidth: overlay.overlayWidth

            mask: Region {
                item: stack
            }

            Column {
                id: stack
                width: parent.width
                spacing: overlay.gap

                anchors.top: stackVAlign === "top" ? parent.top : undefined
                anchors.bottom: stackVAlign === "bottom" ? parent.bottom : undefined
                anchors.verticalCenter: stackVAlign === "center" ? parent.verticalCenter : undefined

                Repeater {
                    model: itemsModel

                    Loader {
                        required property string pluginId

                        sourceComponent: popupGroupComp
                        visible: false
                        width: parent.width

                        onLoaded: {
                            const self = this;
                            const p = overlay._propsMap[pluginId] ?? {};

                            item.popupComp = p.popupComp ?? null;
                            item.popupTimeout = p.popupTimeout ?? 5000;
                            item.maxVisible = Qt.binding(function () {
                                return overlay.maxVisible;
                            });
                            item.themeProvider = Qt.binding(function () {
                                return overlay.themeProvider;
                            });
                            item.iconProvider = Qt.binding(function () {
                                return overlay.iconProvider;
                            });
                            item.overlayState = Qt.binding(function () {
                                return overlay.overlayState;
                            });
                            item.width = Qt.binding(function () {
                                return self.width;
                            });
                            item.height = Qt.binding(function () {
                                return item.implicitHeight;
                            });
                            item.pluginState = p.pluginState ?? null;
                            visible = true;
                        }
                    }
                }
            }
        }
    }
}
