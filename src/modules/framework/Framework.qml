pragma ComponentBehavior: Bound

import Quickshell
import QtQuick
import Qt.labs.folderlistmodel

import "./components/frame"
import "./components/bar"
import "./components/drawer"
import "../../shared/components"

Scope {
    id: framework

    property var themeProvider: null
    property var iconProvider: null
    property var config: ({})
    property var saveConfig: null

    readonly property color frameColor: themeProvider ? themeProvider.overlay : "#26233a"

    DrawerState {
        id: rootDrawerState
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

    property bool settingsLoaded: false

    Connections {
        target: rootSettings
        function onPushedDrawerSlotsChanged() {
            framework.settingsLoaded = true;

            const pushed = {};
            const slots = rootSettings.pushedDrawerSlots;
            for (let i = 0; i < slots.length; i++)
                pushed[slots[i]] = true;

            const prev = Object.keys(rootDrawerState.pushedSlots).sort().join(",");
            const next = Object.keys(pushed).sort().join(",");
            if (prev !== next)
                rootDrawerState.pushedSlots = pushed;
        }
    }

    Connections {
        target: rootDrawerState
        function onPushedSlotsChanged() {
            if (!framework.settingsLoaded || !framework.saveConfig)
                return;
            const saved = (rootSettings.pushedDrawerSlots || []).slice().sort().join(",");
            const current = Object.keys(rootDrawerState.pushedSlots).sort().join(",");
            if (saved !== current)
                framework.saveConfig({
                    pushedDrawerSlots: Object.keys(rootDrawerState.pushedSlots)
                });
        }
    }

    Component {
        id: groupScannerComponent
        FolderListModel {
            showDirs: true
            showFiles: false
            showDotAndDotDot: false
        }
    }

    FolderListModel {
        id: pluginGroupDirs
        folder: Qt.resolvedUrl("../../plugins/")
        showDirs: true
        showFiles: false
        showDotAndDotDot: false

        onStatusChanged: {
            if (status !== FolderListModel.Ready)
                return;
            for (let i = 0; i < count; i++) {
                (function (entry) {
                        const directComp = Qt.createComponent(Qt.resolvedUrl("../../plugins/" + entry + "/Chip.qml"));
                        if (directComp.status === Component.Ready) {
                            rootModuleRegistry.register(entry, directComp);
                        } else if (directComp.status !== Component.Error) {
                            directComp.statusChanged.connect(function () {
                                if (directComp.status === Component.Ready)
                                    rootModuleRegistry.register(entry, directComp);
                            });
                        }

                        const scanner = groupScannerComponent.createObject(framework, {
                            folder: Qt.resolvedUrl("../../plugins/" + entry + "/")
                        });
                        function processGroup() {
                            if (scanner.status !== FolderListModel.Ready)
                                return;
                            for (let j = 0; j < scanner.count; j++) {
                                (function (name) {
                                        const comp = Qt.createComponent(Qt.resolvedUrl("../../plugins/" + entry + "/" + name + "/Chip.qml"));
                                        if (comp.status === Component.Ready) {
                                            rootModuleRegistry.register(name, comp);
                                        } else if (comp.status === Component.Error) {
                                            console.error("Plugin load error [" + name + "]:", comp.errorString());
                                        } else {
                                            comp.statusChanged.connect(function () {
                                                if (comp.status === Component.Ready)
                                                    rootModuleRegistry.register(name, comp);
                                                else if (comp.status === Component.Error)
                                                    console.error("Plugin load error [" + name + "]:", comp.errorString());
                                            });
                                        }
                                    })(scanner.get(j, "fileName"));
                            }
                            scanner.destroy();
                        }
                        scanner.statusChanged.connect(processGroup);
                        if (scanner.status === FolderListModel.Ready)
                            processGroup();
                    })(get(i, "fileName"));
            }
        }
    }

    Loader {
        active: rootSettings.frameMode
        sourceComponent: Frame {
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            color: framework.frameColor
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
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.top ?? {}
            color: framework.frameColor
        }
    }

    Loader {
        active: rootSettings.frameMode || rootBarSizes.bottom !== 0
        sourceComponent: Bar {
            position: "bottom"
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.bottom ?? {}
            color: framework.frameColor
        }
    }

    Loader {
        active: rootSettings.frameMode || rootBarSizes.left !== 0
        sourceComponent: Bar {
            position: "left"
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.left ?? {}
            color: framework.frameColor
        }
    }

    Loader {
        active: rootSettings.frameMode || rootBarSizes.right !== 0
        sourceComponent: Bar {
            position: "right"
            barSizes: rootBarSizes
            themeProvider: framework.themeProvider
            iconProvider: framework.iconProvider
            drawerState: rootDrawerState
            moduleRegistry: rootModuleRegistry
            slotConfig: rootSettings.bars.right ?? {}
            color: framework.frameColor
        }
    }
}
