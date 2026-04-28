pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../sideMargins.js" as SideMargins
import qs.src.shared.theme

Scope {
    id: overlay

    required property string side
    required property string align
    required property var items
    required property var barSizes
    required property bool frameMode
    required property var overlayState
    required property int overlayWidth
    property int maxVisible: 5

    readonly property int gap: Theme.spacing ?? 8
    readonly property bool isHorizontal: side === "top" || side === "bottom"

    readonly property var _m: SideMargins.calc(overlay.side, overlay.align, overlay.barSizes, overlay.frameMode, overlay.gap)

    Variants {
        model: Quickshell.screens

        PanelWindow {
            id: screenWindow
            required property var modelData

            readonly property string stackVAlign: {
                if (overlay.side === "top" || (!overlay.isHorizontal && overlay.align === "start"))
                    return "top";
                if (overlay.side === "bottom" || (!overlay.isHorizontal && overlay.align === "end"))
                    return "bottom";
                return "center";
            }

            readonly property var screenItems: {
                const all = overlay.items;
                const result = [];
                for (let i = 0; i < all.length; i++) {
                    if (all[i].screenName === "" || all[i].screenName === screenWindow.modelData.name)
                        result.push(all[i]);
                }
                return result;
            }

            readonly property int _maxVisible: overlay.maxVisible

            ListModel {
                id: screenModel
            }

            property var _entriesMap: ({})

            function _syncItems() {
                const newItems = screenWindow.screenItems;

                const wasVisibleMap = {};
                for (let i = 0; i < screenModel.count; i++) {
                    const row = screenModel.get(i);
                    wasVisibleMap[row.entryId] = row.entryVisible ?? false;
                }

                const newMap = {};
                for (let i = 0; i < newItems.length; i++)
                    newMap[newItems[i].id] = newItems[i];
                screenWindow._entriesMap = newMap;

                for (let i = screenModel.count - 1; i >= 0; i--) {
                    if (!(screenModel.get(i).entryId in newMap))
                        screenModel.remove(i);
                }

                for (let j = 0; j < newItems.length; j++) {
                    const entry = newItems[j];
                    let found = -1;
                    for (let k = 0; k < screenModel.count; k++) {
                        if (screenModel.get(k).entryId === entry.id) {
                            found = k;
                            break;
                        }
                    }
                    if (found >= 0) {
                        screenModel.setProperty(found, "entryTimestamp", entry.timestamp);
                        screenModel.setProperty(found, "entryTimeout", entry.timeout);
                    } else {
                        let insertAt = screenModel.count;
                        for (let k = 0; k < screenModel.count; k++) {
                            if (screenModel.get(k).entryTimestamp > entry.timestamp) {
                                insertAt = k;
                                break;
                            }
                        }
                        screenModel.insert(insertAt, {
                            entryId: entry.id,
                            entryTimestamp: entry.timestamp,
                            entryTimeout: entry.timeout,
                            entryPriority: entry.priority ?? false,
                            nonPriorityRank: 0,
                            entryVisible: false
                        });
                    }
                }

                let priorityCount = 0;
                let rank = 0;
                for (let i = 0; i < screenModel.count; i++) {
                    if (screenModel.get(i).entryPriority) {
                        screenModel.setProperty(i, "nonPriorityRank", -1);
                        priorityCount++;
                    } else {
                        screenModel.setProperty(i, "nonPriorityRank", rank++);
                    }
                }

                const regularSlots = Math.max(0, screenWindow._maxVisible - priorityCount);
                for (let i = 0; i < screenModel.count; i++) {
                    const row = screenModel.get(i);
                    let vis;
                    if (row.entryPriority) {
                        vis = true;
                    } else {
                        const wasVis = wasVisibleMap[row.entryId] ?? false;
                        vis = wasVis || row.nonPriorityRank < regularSlots;
                    }
                    screenModel.setProperty(i, "entryVisible", vis);
                }
            }

            Component.onCompleted: _syncItems()
            onScreenItemsChanged: _syncItems()
            on_MaxVisibleChanged: _syncItems()

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

                anchors.top: screenWindow.stackVAlign === "top" ? parent.top : undefined
                anchors.bottom: screenWindow.stackVAlign === "bottom" ? parent.bottom : undefined
                anchors.verticalCenter: screenWindow.stackVAlign === "center" ? parent.verticalCenter : undefined

                Repeater {
                    model: screenModel

                    Item {
                        id: popupItem

                        required property string entryId
                        required property int entryTimestamp
                        required property int entryTimeout
                        required property bool entryPriority
                        required property int nonPriorityRank
                        required property bool entryVisible
                        required property int index

                        visible: entryVisible
                        width: stack.width
                        height: visible ? popupContainer.implicitHeight : 0

                        readonly property var _entry: screenWindow._entriesMap[entryId] ?? null

                        function _dismiss() {
                            const id = popupItem.entryId;
                            if (!overlay.overlayState.isVisible(id))
                                return;
                            const entry = screenWindow._entriesMap[id] ?? null;
                            overlay.overlayState.remove(id);
                            if (entry && entry.onDismiss)
                                entry.onDismiss();
                        }

                        Timer {
                            id: dismissTimer
                            interval: popupItem.entryTimeout
                            running: interval > 0 && popupItem.visible
                            repeat: false
                            onTriggered: popupItem._dismiss()
                        }

                        onEntryTimestampChanged: {
                            if (dismissTimer.interval > 0)
                                dismissTimer.restart();
                        }

                        DragHandler {
                            target: null
                            xAxis.enabled: true
                            yAxis.enabled: false

                            onTranslationChanged: {
                                if (popupContainer)
                                    popupContainer.x = translation.x;
                            }

                            onActiveChanged: {
                                if (active)
                                    return;
                                const threshold = popupItem.width * 0.4;
                                const px = popupContainer.x;
                                if (Math.abs(px) >= threshold) {
                                    collapseAnim.start();
                                    slideAnim.to = px > 0 ? popupItem.width * 2 : -popupItem.width * 2;
                                    slideAnim.target = popupContainer;
                                    slideAnim.start();
                                    dragDismissTimer.start();
                                } else {
                                    snapAnim.target = popupContainer;
                                    snapAnim.start();
                                }
                            }
                        }

                        Timer {
                            id: dragDismissTimer
                            interval: 210
                            repeat: false
                            onTriggered: popupItem._dismiss()
                        }

                        NumberAnimation {
                            id: snapAnim
                            property: "x"
                            to: 0
                            duration: 300
                            easing.type: Easing.OutCubic
                        }

                        NumberAnimation {
                            id: collapseAnim
                            target: popupItem
                            property: "height"
                            to: 0
                            duration: 200
                            easing.type: Easing.InCubic
                        }

                        NumberAnimation {
                            id: slideAnim
                            property: "x"
                            duration: 200
                            easing.type: Easing.InCubic
                        }

                        PopupContainer {
                            id: popupContainer
                            width: popupItem.width
                            contentComponent: popupItem._entry?.contentComponent ?? null
                            popupData: popupItem._entry?.data ?? null
                            onRequestDismiss: popupItem._dismiss()
                        }
                    }
                }
            }
        }
    }
}
