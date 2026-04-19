pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"
import "./components/drawer"
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
            settings: rootSettings
        }
    }

    DrawerManager {
        settings: rootSettings
        barSizes: rootBarSizes
        drawerState: rootDrawerState
        themeProvider: framework.themeProvider
    }

    Loader {
        active: rootSettings.frameMode || rootBarSizes.top !== 0
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
        active: rootSettings.frameMode || rootBarSizes.bottom !== 0
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
        active: rootSettings.frameMode || rootBarSizes.left !== 0
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
        active: rootSettings.frameMode || rootBarSizes.right !== 0
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
}
