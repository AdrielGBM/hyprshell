pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"
import "./components/drawer"

Scope {
    id: framework

    Settings {
        id: settings
    }

    BarSizes {
        id: barSizes
        settings: settings
    }

    Loader {
        active: settings.frameMode
        sourceComponent: Frame {
            barSizes: barSizes
            gap: settings.gap
            radius: settings.radius
            color: settings.color
        }
    }

    DrawerManager {
        settings: settings
        barSizes: barSizes
    }

    Loader {
        active: settings.frameMode || barSizes.top !== 0
        sourceComponent: Bar {
            position: "top"
            barSizes: barSizes
            gap: settings.gap
            radius: settings.radius
            color: settings.color
        }
    }

    Loader {
        active: settings.frameMode || barSizes.bottom !== 0
        sourceComponent: Bar {
            position: "bottom"
            barSizes: barSizes
            gap: settings.gap
            radius: settings.radius
            color: settings.color
        }
    }

    Loader {
        active: settings.frameMode || barSizes.left !== 0
        sourceComponent: Bar {
            position: "left"
            barSizes: barSizes
            gap: settings.gap
            radius: settings.radius
            color: settings.color
        }
    }

    Loader {
        active: settings.frameMode || barSizes.right !== 0
        sourceComponent: Bar {
            position: "right"
            barSizes: barSizes
            gap: settings.gap
            radius: settings.radius
            color: settings.color
        }
    }
}
