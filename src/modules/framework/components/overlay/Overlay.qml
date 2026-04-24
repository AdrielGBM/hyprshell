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

    readonly property int gap: themeProvider?.spacing ?? 8
    readonly property bool isHorizontal: side === "top" || side === "bottom"

    readonly property var _m: SideMargins.calc(overlay.side, overlay.align, overlay.barSizes, overlay.frameMode, overlay.gap)

    property var itemsLookup: ({})

    ListModel {
        id: itemsModel
    }

    function _syncItems() {
        const lookup = {};
        for (let i = 0; i < overlay.items.length; i++)
            lookup[overlay.items[i].id] = overlay.items[i];
        overlay.itemsLookup = lookup;

        const newIds = overlay.items.map(x => x.id);

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

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.exclusiveZone: -1

            anchors {
                top: overlay.side === "top" || (!overlay.isHorizontal && overlay.align === "start")
                bottom: overlay.side === "bottom" || (!overlay.isHorizontal && overlay.align === "end")
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
            implicitHeight: stack.implicitHeight

            Column {
                id: stack
                width: parent.width
                spacing: overlay.gap

                Repeater {
                    model: itemsModel

                    Loader {
                        required property string pluginId

                        width: stack.width
                        sourceComponent: overlay.itemsLookup[pluginId]?.component ?? null

                        onLoaded: {
                            item.width = Qt.binding(function () {
                                return stack.width;
                            });

                            const props = overlay.itemsLookup[pluginId]?.props ?? {};
                            for (const key in props)
                                item[key] = props[key];

                            if ("themeProvider" in item)
                                item.themeProvider = Qt.binding(function () {
                                    return overlay.themeProvider;
                                });
                            if ("overlayState" in item)
                                item.overlayState = Qt.binding(function () {
                                    return overlay.overlayState;
                                });
                        }
                    }
                }
            }
        }
    }
}
