import QtQuick

QtObject {
    id: settings

    property int scale: 1
    property bool frameMode: true

    property int inactiveBarSize: 16
    property int activeBarSize: 40

    property int drawerWidth: 200
    property int drawerHeight: 200
    property var bars: ({})

    property var config: ({})
    onConfigChanged: applyConfig(config)

    function applyConfig(cfg) {
        if (!cfg)
            return;
        if (cfg.scale !== undefined)
            scale = cfg.scale;
        if (cfg.inactiveBarSize !== undefined)
            inactiveBarSize = cfg.inactiveBarSize;
        if (cfg.activeBarSize !== undefined)
            activeBarSize = cfg.activeBarSize;
        if (cfg.bars !== undefined)
            bars = cfg.bars;
        if (cfg.frameMode !== undefined)
            frameMode = cfg.frameMode;
        if (cfg.drawerWidth !== undefined)
            drawerWidth = cfg.drawerWidth;
        if (cfg.drawerHeight !== undefined)
            drawerHeight = cfg.drawerHeight;
    }
}
