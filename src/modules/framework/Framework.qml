pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"
import "./components/drawer"
import "./components/corner"
import "../../shared/components"
import "../../shared/services"

Scope {
    id: framework

    property var themeProvider: null
    property var iconProvider: null
    property var config: ({})
    property var saveConfig: null

    readonly property color color: themeProvider?.overlay

    DrawerState {
        id: rootDrawerState
        drawerOrientation: rootSettings.drawerOrientation
    }

    ModuleRegistry {
        id: rootModuleRegistry
    }

    Settings {
        id: rootSettings
        config: framework.config
    }

    BarSizes {
        id: rootBarSizes
        settings: rootSettings
    }

    Component {
        id: subScannerComp
        FolderScanner {
            filename: "Chip.qml"
            onItemReady: function (key, comp) {
                rootModuleRegistry.register(key, comp);
            }
            onItemError: function (key, err) {
                console.error("Plugin load error [" + key + "]:", err);
            }
        }
    }

    FolderScanner {
        id: pluginScanner
        folder: Qt.resolvedUrl("../../plugins/")
        filename: "Chip.qml"
        onItemReady: function (key, comp) {
            rootModuleRegistry.register(key, comp);
        }
        onDirFound: function (key) {
            subScannerComp.createObject(framework, {
                folder: Qt.resolvedUrl("../../plugins/" + key + "/")
            });
        }
    }

    Loader {
        active: rootSettings.frameMode
        sourceComponent: Frame {
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            color: framework.color
        }
    }

    DrawerManager {
        settings: rootSettings
        barSizes: rootBarSizes
        drawerState: rootDrawerState
        frameMode: rootSettings.frameMode
        themeProvider: framework.themeProvider
    }

    Loader {
        sourceComponent: Bar {
            position: "top"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.top ?? {}
            color: framework.color
        }
    }

    Loader {
        sourceComponent: Bar {
            position: "bottom"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.bottom ?? {}
            color: framework.color
        }
    }

    Loader {
        sourceComponent: Bar {
            position: "left"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.left ?? {}
            color: framework.color
        }
    }

    Loader {
        sourceComponent: Bar {
            position: "right"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.right ?? {}
            color: framework.color
        }
    }

    Loader {
        active: rootBarSizes.top > rootBarSizes.inactive && rootBarSizes.left > rootBarSizes.inactive && !!(rootSettings.corners.topLeft)
        sourceComponent: Corner {
            position: "topLeft"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            itemConfig: rootSettings.corners.topLeft ?? null
            color: framework.color
        }
    }

    Loader {
        active: rootBarSizes.top > rootBarSizes.inactive && rootBarSizes.right > rootBarSizes.inactive && !!(rootSettings.corners.topRight)
        sourceComponent: Corner {
            position: "topRight"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            itemConfig: rootSettings.corners.topRight ?? null
            color: framework.color
        }
    }

    Loader {
        active: rootBarSizes.bottom > rootBarSizes.inactive && rootBarSizes.left > rootBarSizes.inactive && !!(rootSettings.corners.bottomLeft)
        sourceComponent: Corner {
            position: "bottomLeft"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            itemConfig: rootSettings.corners.bottomLeft ?? null
            color: framework.color
        }
    }

    Loader {
        active: rootBarSizes.bottom > rootBarSizes.inactive && rootBarSizes.right > rootBarSizes.inactive && !!(rootSettings.corners.bottomRight)
        sourceComponent: Corner {
            position: "bottomRight"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            itemConfig: rootSettings.corners.bottomRight ?? null
            color: framework.color
        }
    }
}
