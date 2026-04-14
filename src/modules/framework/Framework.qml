pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"
import "./components/drawer"
import "../temp-statusbar"

Scope {
    id: framework

    property var config: ({})
    property var themeProvider: null

    readonly property color frameColor: themeProvider ? themeProvider.overlay : "#26233a"

    Settings {
        id: settings
        config: framework.config
    }

    BarSizes {
        id: barSizes
        settings: settings
    }

    Loader {
        active: settings.frameMode
        sourceComponent: Frame {
            barSizes: barSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
        }
    }

    DrawerManager {
        settings: settings
        barSizes: barSizes
        themeProvider: framework.themeProvider
    }

    Loader {
        active: settings.frameMode || barSizes.top !== 0
        sourceComponent: Bar {
            position: "top"
            barSizes: barSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
            content: Component {
                TempStatusBar {
                    themeProvider: framework.themeProvider
                }
            }
        }
    }

    Loader {
        active: settings.frameMode || barSizes.bottom !== 0
        sourceComponent: Bar {
            position: "bottom"
            barSizes: barSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
        }
    }

    Loader {
        active: settings.frameMode || barSizes.left !== 0
        sourceComponent: Bar {
            position: "left"
            barSizes: barSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
        }
    }

    Loader {
        active: settings.frameMode || barSizes.right !== 0
        sourceComponent: Bar {
            position: "right"
            barSizes: barSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
        }
    }
}
