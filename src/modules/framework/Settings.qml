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

    property bool topDrawer1Active: false
    property bool topDrawer2Active: false

    property bool bottomDrawer1Active: false
    property bool bottomDrawer2Active: false

    property bool leftDrawer1Active: false
    property bool leftDrawer2Active: false

    property bool rightDrawer1Active: false
    property bool rightDrawer2Active: false

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
        if (cfg.topDrawer1Active !== undefined)
            topDrawer1Active = cfg.topDrawer1Active;
        if (cfg.topDrawer2Active !== undefined)
            topDrawer2Active = cfg.topDrawer2Active;
        if (cfg.bottomDrawer1Active !== undefined)
            bottomDrawer1Active = cfg.bottomDrawer1Active;
        if (cfg.bottomDrawer2Active !== undefined)
            bottomDrawer2Active = cfg.bottomDrawer2Active;
        if (cfg.leftDrawer1Active !== undefined)
            leftDrawer1Active = cfg.leftDrawer1Active;
        if (cfg.leftDrawer2Active !== undefined)
            leftDrawer2Active = cfg.leftDrawer2Active;
        if (cfg.rightDrawer1Active !== undefined)
            rightDrawer1Active = cfg.rightDrawer1Active;
        if (cfg.rightDrawer2Active !== undefined)
            rightDrawer2Active = cfg.rightDrawer2Active;
    }
}
