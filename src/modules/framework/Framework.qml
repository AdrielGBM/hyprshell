pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"

Scope {
    id: framework

    Settings {
        id: settings
    }

    BarSizes {
        id: barSizes
        top: settings.topBarActive ? settings.activeBarSize : settings.frameMode ? settings.inactiveBarSize : 0
        left: settings.leftBarActive ? settings.activeBarSize : settings.frameMode ? settings.inactiveBarSize : 0
        right: settings.rightBarActive ? settings.activeBarSize : settings.frameMode ? settings.inactiveBarSize : 0
        bottom: settings.bottomBarActive ? settings.activeBarSize : settings.frameMode ? settings.inactiveBarSize : 0
        inactive: settings.inactiveBarSize
        active: settings.activeBarSize
    }

    Loader {
        active: settings.frameMode
        sourceComponent: Frame {}
    }

    Loader {
        active: settings.frameMode || barSizes.top !== 0
        sourceComponent: Bar {
            position: "top"
            barSizes: barSizes
        }
    }

    Loader {
        active: settings.frameMode || barSizes.bottom !== 0
        sourceComponent: Bar {
            position: "bottom"
            barSizes: barSizes
        }
    }

    Loader {
        active: settings.frameMode || barSizes.left !== 0
        sourceComponent: Bar {
            position: "left"
            barSizes: barSizes
        }
    }

    Loader {
        active: settings.frameMode || barSizes.right !== 0
        sourceComponent: Bar {
            position: "right"
            barSizes: barSizes
        }
    }
}
