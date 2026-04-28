pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "./components/frame"
import "./components/bar"
import "./components/drawer"
import "./components/corner"
import "./components/overlay"
import "../../shared/components"
import "../../shared/services"
import qs.src.shared.theme
import qs.src.shared.settings as App

Scope {
    id: framework

    readonly property var config: App.Settings.framework
    property var pluginStates: ({})

    function saveConfig(values) {
        App.Settings.save("framework", values);
    }

    onPluginStatesChanged: {
        Object.keys(pluginStates).forEach(function (key) {
            if (!rootModuleRegistry.getState(key))
                rootModuleRegistry.registerState(key, pluginStates[key]);
        });
    }

    readonly property color color: Theme.overlay

    DrawerState {
        id: rootDrawerState
        drawerOrientation: rootSettings.drawerOrientation
    }

    OverlayState {
        id: rootOverlayState
        defaultSide: rootSettings.overlaySide
        defaultAlign: rootSettings.overlayAlign
    }

    ModuleRegistry {
        id: rootModuleRegistry
    }

    Settings {
        id: rootSettings
        config: App.Settings.framework
    }

    BarSizes {
        id: rootBarSizes
        settings: rootSettings
    }

    Component {
        id: chipSubScannerComp
        FolderScanner {
            filename: "Chip.qml"
            onItemReady: function (key, comp) {
                rootModuleRegistry.register(key, comp);
            }
            onItemError: function (key, err) {
                if (!err.includes("No such file or directory"))
                    console.error("Plugin load error [" + key + "]:", err);
            }
        }
    }

    Component {
        id: popupWatcherComp
        PopupWatcher {}
    }

    Component {
        id: popupSubScannerComp
        FolderScanner {
            filename: "Popup.qml"
            onItemReady: function (key, comp) {
                popupWatcherComp.createObject(framework, {
                    pluginKey: key,
                    popupComp: comp,
                    moduleRegistry: rootModuleRegistry,
                    overlayState: rootOverlayState,
                    popupTimeout: Qt.binding(function () {
                        return rootSettings.overlayPopupTimeout;
                    })
                });
            }
            onItemError: function (key, err) {
                if (!err.includes("No such file or directory"))
                    console.error("Plugin popup load error [" + key + "]:", err);
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
            const baseFolder = Qt.resolvedUrl("../../plugins/" + key + "/");
            chipSubScannerComp.createObject(framework, {
                folder: baseFolder
            });
            popupSubScannerComp.createObject(framework, {
                folder: baseFolder
            });
        }
    }

    Loader {
        active: rootSettings.frameMode
        sourceComponent: Frame {
            barSizes: rootBarSizes
            color: framework.color
        }
    }

    DrawerManager {
        settings: rootSettings
        barSizes: rootBarSizes
        drawerState: rootDrawerState
        frameMode: rootSettings.frameMode
    }

    OverlayManager {
        overlayState: rootOverlayState
        barSizes: rootBarSizes
        frameMode: rootSettings.frameMode
        overlayWidth: rootSettings.overlayWidth
        maxVisible: rootSettings.overlayMaxVisible
    }

    Loader {
        sourceComponent: Bar {
            position: "top"
            frameMode: rootSettings.frameMode
            barSizes: rootBarSizes
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            overlayState: rootOverlayState
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
            moduleRegistry: rootModuleRegistry
            drawerState: rootDrawerState
            overlayState: rootOverlayState
            itemConfig: rootSettings.corners.bottomRight ?? null
            color: framework.color
        }
    }
}
