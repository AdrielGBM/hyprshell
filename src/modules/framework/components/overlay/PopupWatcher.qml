pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: watcher

    property string pluginKey: ""
    property var popupComp: null
    property var moduleRegistry: null
    property var overlayState: null
    property int popupTimeout: 5000

    readonly property var pluginState: moduleRegistry?.states[pluginKey] ?? null

    property var _pushedIds: []

    onPluginStateChanged: {
        _pushedIds = [];
        _fullSync();
    }

    Connections {
        target: watcher.pluginState ? watcher.pluginState.activeList : null

        function onRowsInserted(parent, first, last) {
            watcher._pushRange(first, last);
        }
        function onRowsRemoved(parent, first, last) {
            watcher._syncRemovals();
        }
        function onModelReset() {
            const snapshot = watcher._pushedIds.slice();
            for (let i = 0; i < snapshot.length; i++)
                watcher.overlayState.remove(snapshot[i]);
            watcher._pushedIds = [];
        }
        function onDataChanged(topLeft, bottomRight, roles) {
            watcher._refreshSync(topLeft.row, bottomRight.row);
        }
    }

    function _fullSync() {
        if (!overlayState || !pluginState)
            return;
        const al = pluginState.activeList;
        const currentIds = [];
        for (let i = 0; i < al.count; i++) {
            const item = al.get(i);
            const id = _entryId(item);
            currentIds.push(id);
            if (!overlayState.isVisible(id))
                _pushEntry(id, item, false);
        }
        const snapshot = _pushedIds.slice();
        for (let j = 0; j < snapshot.length; j++) {
            if (currentIds.indexOf(snapshot[j]) < 0)
                overlayState.remove(snapshot[j]);
        }
        _pushedIds = currentIds;
    }

    function _pushRange(first, last) {
        if (!overlayState || !pluginState)
            return;
        const al = pluginState.activeList;
        for (let i = first; i <= last && i < al.count; i++) {
            const item = al.get(i);
            const id = _entryId(item);
            if (!overlayState.isVisible(id))
                _pushEntry(id, item, false);
            if (_pushedIds.indexOf(id) < 0)
                _pushedIds = _pushedIds.concat([id]);
        }
    }

    function _syncRemovals() {
        if (!overlayState || !pluginState)
            return;
        const al = pluginState.activeList;
        const currentIds = [];
        for (let i = 0; i < al.count; i++)
            currentIds.push(_entryId(al.get(i)));
        const snapshot = _pushedIds.slice();
        const remaining = [];
        for (let j = 0; j < snapshot.length; j++) {
            if (currentIds.indexOf(snapshot[j]) < 0)
                overlayState.remove(snapshot[j]);
            else
                remaining.push(snapshot[j]);
        }
        _pushedIds = remaining;
    }

    function _refreshSync(firstRow, lastRow) {
        if (!overlayState || !pluginState)
            return;
        const al = pluginState.activeList;
        for (let i = firstRow; i <= lastRow && i < al.count; i++) {
            const item = al.get(i);
            const id = _entryId(item);
            if (overlayState.isVisible(id))
                _pushEntry(id, item, true);
        }
    }

    function _entryId(item) {
        const stableId = (item.notifId !== undefined && item.notifId !== null) ? item.notifId : item.notif?.id;
        const hasId = stableId !== undefined && stableId !== null;
        return watcher.pluginKey + ":" + (hasId ? String(stableId) : "osd");
    }

    function _pushEntry(id, item, refreshTimer) {
        const notifData = item.notifData;
        const notif = item.notif;
        const hasSnapshot = notifData !== undefined && notifData !== null;
        const hasQObject = notif && typeof notif.id !== "undefined";
        const hasId = hasSnapshot ? true : hasQObject;

        const data = hasSnapshot ? notifData : (hasQObject ? notif : watcher.pluginState);

        let rawTimeout = (item.notifTimeout !== undefined) ? item.notifTimeout : 0;
        let timeout;
        if (rawTimeout < 0)
            timeout = 0;
        else if (rawTimeout === 0)
            timeout = watcher.popupTimeout;
        else
            timeout = rawTimeout;

        const capturedState = watcher.pluginState;
        const capturedNotifId = (item.notifId !== undefined && item.notifId !== null) ? item.notifId : (hasQObject ? notif.id : null);

        const priority = !hasId && watcher._pushedIds.length === 0;

        watcher.overlayState.push(id, watcher.popupComp, data, null, null, timeout, function () {
            if (capturedState && capturedState.removeActive)
                capturedState.removeActive({
                    id: capturedNotifId
                });
            if (capturedNotifId !== null && capturedState) {
                const al = capturedState.activeList;
                for (let i = 0; i < al.count; i++) {
                    const entry = al.get(i).notif;
                    if (entry && entry.id === capturedNotifId) {
                        if (typeof entry.expire === "function")
                            entry.expire();
                        break;
                    }
                }
            }
        }, priority);
    }
}
