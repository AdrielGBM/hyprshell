import QtQuick

QtObject {
    id: overlayState

    property string defaultSide: "top"
    property string defaultAlign: "end"

    property var active: ({})

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
        const ids = Object.keys(active);
        for (let i = 0; i < ids.length; i++) {
            const id = ids[i];
            const item = active[id];
            const key = canonicalKey(item.side, item.align);
            if (!groups[key]) {
                const dash = key.indexOf("-");
                groups[key] = {
                    side: key.substring(0, dash),
                    align: key.substring(dash + 1),
                    items: []
                };
            }
            groups[key].items.push({
                id: id,
                component: item.component,
                props: item.props
            });
        }
        return groups;
    }

    function show(id, component, props, side, align) {
        const s = (side && side !== "") ? side : defaultSide;
        const a = (align && align !== "") ? align : defaultAlign;
        const updated = Object.assign({}, active);
        updated[id] = {
            component: component,
            props: props ?? {},
            side: s,
            align: a
        };
        active = updated;
    }

    function hide(id) {
        if (active[id] === undefined)
            return;
        const updated = Object.assign({}, active);
        delete updated[id];
        active = updated;
    }

    function toggle(id, component, props, side, align) {
        if (isVisible(id))
            hide(id);
        else
            show(id, component, props, side, align);
    }

    function isVisible(id) {
        return active[id] !== undefined;
    }
}
