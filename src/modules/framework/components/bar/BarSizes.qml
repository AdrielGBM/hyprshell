import QtQuick

import "../../"

QtObject {
    required property Settings settings

    property int inactive: settings.inactiveBarSize
    property int active: settings.activeBarSize

    property int top: settings.topBarActive ? active : (settings.frameMode ? inactive : 0)
    property int left: settings.leftBarActive ? active : (settings.frameMode ? inactive : 0)
    property int right: settings.rightBarActive ? active : (settings.frameMode ? inactive : 0)
    property int bottom: settings.bottomBarActive ? active : (settings.frameMode ? inactive : 0)
}
