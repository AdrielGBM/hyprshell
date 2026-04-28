pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "../modules/framework"
import "../modules/background"
import qs.src.shared.theme
import qs.src.shared.settings
import "../shared/providers"
import "../shared/services"

Scope {
    id: core

    property IconProvider iconProvider: IconProvider {}
    property I18nProvider i18nProvider: I18nProvider {
        language: Settings.language
    }

    Binding {
        target: Theme
        property: "config"
        value: Settings.theme
    }

    property var pluginStates: ({})

    Component {
        id: stateSubScannerComp
        FolderScanner {
            filename: "State.qml"
            onItemReady: function (key, comp) {
                const instance = comp.createObject(core);
                if (instance) {
                    const updated = Object.assign({}, core.pluginStates);
                    updated[key] = instance;
                    core.pluginStates = updated;
                }
            }
            onItemError: function (key, err) {
                if (!err.includes("No such file or directory"))
                    console.error("State load error [" + key + "]:", err);
            }
        }
    }

    FolderScanner {
        folder: Qt.resolvedUrl("../plugins/")
        filename: "State.qml"
        onDirFound: function (key) {
            stateSubScannerComp.createObject(core, {
                folder: Qt.resolvedUrl("../plugins/" + key + "/")
            });
        }
    }

    Background {}

    Framework {
        iconProvider: core.iconProvider
        i18nProvider: core.i18nProvider
        pluginStates: core.pluginStates
    }
}
