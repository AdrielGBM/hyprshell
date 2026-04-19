import QtQuick

QtObject {
    id: settings

    property bool frameMode: true

    property int baseGap: 8
    property int baseRadius: 8

    property int inactiveBarSize: 16
    property int activeBarSize: 40

    property int drawerWidth: 200
    property int drawerHeight: 200
    property string drawerOrientation: "vertical"
    property var bars: ({})

    property var config: ({})
    onConfigChanged: applyConfig(config)

    function normalizeBars(raw) {
        const result = {};
        const sides = ["top", "bottom", "left", "right"];
        for (let i = 0; i < sides.length; i++) {
            const side = sides[i];
            if (!raw[side])
                continue;
            const sideConfig = Object.assign({}, raw[side]);
            const slots = ["left", "center", "right", "top", "bottom"];
            for (let j = 0; j < slots.length; j++) {
                const slot = slots[j];
                if (Array.isArray(sideConfig[slot])) {
                    sideConfig[slot] = sideConfig[slot].map(function (entry) {
                        return typeof entry === "string" ? {
                            id: entry
                        } : entry;
                    });
                }
            }
            result[side] = sideConfig;
        }
        return result;
    }

    function applyConfig(cfg) {
        if (!cfg)
            return;
        if (cfg.baseGap !== undefined)
            baseGap = cfg.baseGap;
        if (cfg.baseRadius !== undefined)
            baseRadius = cfg.baseRadius;
        if (cfg.inactiveBarSize !== undefined)
            inactiveBarSize = cfg.inactiveBarSize;
        if (cfg.activeBarSize !== undefined)
            activeBarSize = cfg.activeBarSize;
        if (cfg.bars !== undefined)
            bars = normalizeBars(cfg.bars);
        if (cfg.frameMode !== undefined)
            frameMode = cfg.frameMode;
        if (cfg.drawerWidth !== undefined)
            drawerWidth = cfg.drawerWidth;
        if (cfg.drawerHeight !== undefined)
            drawerHeight = cfg.drawerHeight;
        if (cfg.drawerOrientation !== undefined)
            drawerOrientation = cfg.drawerOrientation;
    }
}
