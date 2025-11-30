import QtQuick

QtObject {
    required property var settings
    required property var barSizes

    function getDrawerCount(side) {
        if (side === "top") {
            return (settings.topDrawer1Active ? 1 : 0) + (settings.topDrawer2Active ? 1 : 0);
        } else if (side === "bottom") {
            return (settings.bottomDrawer1Active ? 1 : 0) + (settings.bottomDrawer2Active ? 1 : 0);
        } else if (side === "left") {
            return (settings.leftDrawer1Active ? 1 : 0) + (settings.leftDrawer2Active ? 1 : 0);
        } else if (side === "right") {
            return (settings.rightDrawer1Active ? 1 : 0) + (settings.rightDrawer2Active ? 1 : 0);
        }
        return 0;
    }

    function isDrawerActive(side, index) {
        if (side === "top") {
            return index === 0 ? settings.topDrawer1Active : settings.topDrawer2Active;
        } else if (side === "bottom") {
            return index === 0 ? settings.bottomDrawer1Active : settings.bottomDrawer2Active;
        } else if (side === "left") {
            return index === 0 ? settings.leftDrawer1Active : settings.leftDrawer2Active;
        } else if (side === "right") {
            return index === 0 ? settings.rightDrawer1Active : settings.rightDrawer2Active;
        }
        return false;
    }
}
