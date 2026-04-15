import QtQuick

QtObject {
    id: settings

    property int scale: 1
    property bool frameMode: true

    property int inactiveBarSize: 16
    property int activeBarSize: 40

    property bool topBarActive: true
    property bool leftBarActive: false
    property bool rightBarActive: true
    property bool bottomBarActive: false

    property int drawerWidth: 200
    property int drawerHeight: 200
    property var pushedDrawerSlots: []
    property var bars: ({})

    property var config: ({})
    onConfigChanged: applyConfig(config)

    function applyConfig(cfg) {
        if (!cfg)
            return;
        if (cfg.scale !== undefined)
            scale = cfg.scale;
        if (cfg.frameMode !== undefined)
            frameMode = cfg.frameMode;
        if (cfg.inactiveBarSize !== undefined)
            inactiveBarSize = cfg.inactiveBarSize;
        if (cfg.activeBarSize !== undefined)
            activeBarSize = cfg.activeBarSize;
        if (cfg.topBarActive !== undefined)
            topBarActive = cfg.topBarActive;
        if (cfg.leftBarActive !== undefined)
            leftBarActive = cfg.leftBarActive;
        if (cfg.rightBarActive !== undefined)
            rightBarActive = cfg.rightBarActive;
        if (cfg.bottomBarActive !== undefined)
            bottomBarActive = cfg.bottomBarActive;
        if (cfg.drawerWidth !== undefined)
            drawerWidth = cfg.drawerWidth;
        if (cfg.drawerHeight !== undefined)
            drawerHeight = cfg.drawerHeight;
        if (cfg.pushedDrawerSlots !== undefined)
            pushedDrawerSlots = cfg.pushedDrawerSlots;
        if (cfg.bars !== undefined)
            bars = cfg.bars;
    }
}
