import QtQuick

QtObject {
    id: settings

    property var config: ({})

    readonly property bool frameMode: config.frameMode ?? true

    readonly property int inactiveBarSize: config.inactiveBarSize ?? 16
    readonly property int activeBarSize: config.activeBarSize ?? 40

    readonly property int drawerWidth: config.drawerWidth ?? 200
    readonly property int drawerHeight: config.drawerHeight ?? 200
    readonly property var bars: config.bars !== undefined ? normalizeBars(config.bars) : ({})
    readonly property var corners: config.corners !== undefined ? normalizeCorners(config.corners) : ({})

    readonly property int overlayWidth: config.overlay?.width ?? 360
    readonly property string overlaySide: config.overlay?.position?.side ?? "top"
    readonly property string overlayAlign: config.overlay?.position?.align ?? "center"
    readonly property int overlayPopupTimeout: config.overlay?.popupTimeout ?? 5000
    readonly property int overlayMaxVisible: config.overlay?.maxVisible ?? 5

    function normalizeCorners(raw) {
        const result = {};
        const positions = ["topLeft", "topRight", "bottomLeft", "bottomRight"];
        for (let i = 0; i < positions.length; i++) {
            const pos = positions[i];
            if (raw[pos] !== undefined && raw[pos] !== null) {
                const val = raw[pos];
                result[pos] = typeof val === "string" ? {
                    id: val
                } : val;
            }
        }
        return result;
    }

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
}
