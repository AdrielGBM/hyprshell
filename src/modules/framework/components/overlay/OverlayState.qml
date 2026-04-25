import QtQuick

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

    function push(id, component, props, side, align) {
        const s = (side && side !== "") ? side : defaultSide;
        const a = (align && align !== "") ? align : defaultAlign;
        const item = {
            id: id,
            component: component,
            props: props ?? {},
            side: s,
            align: a
        };
        const idx = queue.findIndex(function (x) {
            return x.id === id;
        });
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

    function toggle(id, component, props, side, align) {
        if (isVisible(id))
            remove(id);
        else
            push(id, component, props, side, align);
    }

    function isVisible(id) {
        return queue.some(function (x) {
            return x.id === id;
        });
    }
}
