import QtQuick
import Quickshell.Hyprland

QtObject {
    id: overlayState

    property string defaultSide: "top"
    property string defaultAlign: "end"

    property var queue: []

    function canonicalKey(side, align) {
        if (side === "left" && align === "start")
            return "top-start";
        if (side === "left" && align === "end")
            return "bottom-start";
        if (side === "right" && align === "start")
            return "top-end";
        if (side === "right" && align === "end")
            return "bottom-end";
        return side + "-" + align;
    }

    readonly property var positionGroups: {
        const groups = {};
        for (let i = 0; i < queue.length; i++) {
            const item = queue[i];
            const key = canonicalKey(item.side, item.align);
            if (!groups[key]) {
                const dash = key.indexOf("-");
                groups[key] = {
                    side: key.substring(0, dash),
                    align: key.substring(dash + 1),
                    items: []
                };
            }
            groups[key].items.push(item);
        }
        return groups;
    }

    function push(id, contentComponent, data, side, align, timeout, onDismiss, priority) {
        const s = (side && side !== "") ? side : defaultSide;
        const a = (align && align !== "") ? align : defaultAlign;
        const idx = queue.findIndex(function (x) {
            return x.id === id;
        });
        const existingPriority = idx >= 0 ? queue[idx].priority : (priority ?? false);
        const existingScreen = idx >= 0 ? queue[idx].screenName : (Hyprland.focusedMonitor?.name ?? "");
        const item = {
            id: id,
            contentComponent: contentComponent,
            data: data ?? null,
            side: s,
            align: a,
            timeout: timeout ?? 0,
            timestamp: Date.now(),
            onDismiss: onDismiss ?? null,
            priority: existingPriority,
            screenName: existingScreen
        };
        if (idx >= 0) {
            const updated = queue.slice();
            updated[idx] = item;
            queue = updated;
        } else {
            queue = queue.concat([item]);
        }
    }

    function remove(id) {
        const idx = queue.findIndex(function (x) {
            return x.id === id;
        });
        if (idx < 0)
            return;
        const updated = queue.slice();
        updated.splice(idx, 1);
        queue = updated;
    }

    function isVisible(id) {
        return queue.some(function (x) {
            return x.id === id;
        });
    }

    function dismissAll() {
        const snapshot = queue.slice();
        queue = [];
        for (let i = 0; i < snapshot.length; i++) {
            if (snapshot[i].onDismiss)
                snapshot[i].onDismiss();
        }
    }
}
