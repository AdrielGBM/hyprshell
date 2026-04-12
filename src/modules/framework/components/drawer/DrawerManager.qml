pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: drawerManager

    required property var settings
    required property var barSizes
    property var themeProvider: null

    readonly property color drawerColor: themeProvider ? themeProvider.overlay : "#26233a"

    DrawerSizes {
        id: drawerSizes
        settings: drawerManager.settings
        barSizes: drawerManager.barSizes
    }

    Loader {
        active: drawerManager.settings.topDrawer1Active || drawerManager.settings.topDrawer2Active
        sourceComponent: Drawer {
            side: "top"
            drawerSizes: drawerSizes
            barSizes: drawerManager.barSizes
            settings: drawerManager.settings
            gap: drawerManager.settings.gap
            radius: drawerManager.settings.radius
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.settings.bottomDrawer1Active || drawerManager.settings.bottomDrawer2Active
        sourceComponent: Drawer {
            side: "bottom"
            drawerSizes: drawerSizes
            barSizes: drawerManager.barSizes
            settings: drawerManager.settings
            gap: drawerManager.settings.gap
            radius: drawerManager.settings.radius
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.settings.leftDrawer1Active || drawerManager.settings.leftDrawer2Active
        sourceComponent: Drawer {
            side: "left"
            drawerSizes: drawerSizes
            barSizes: drawerManager.barSizes
            settings: drawerManager.settings
            gap: drawerManager.settings.gap
            radius: drawerManager.settings.radius
            color: drawerManager.drawerColor
        }
    }

    Loader {
        active: drawerManager.settings.rightDrawer1Active || drawerManager.settings.rightDrawer2Active
        sourceComponent: Drawer {
            side: "right"
            drawerSizes: drawerSizes
            barSizes: drawerManager.barSizes
            settings: drawerManager.settings
            gap: drawerManager.settings.gap
            radius: drawerManager.settings.radius
            color: drawerManager.drawerColor
        }
    }
}
