pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

import "../modules/framework"
import "../modules/background"
import qs.src.shared.services.theme
import qs.src.shared.services.settings
import qs.src.shared.services.i18n
import qs.src.shared.services.notifications
import qs.src.shared.utils

Scope {
    id: core

    readonly property var notificationsService: Notifications

    Binding {
        target: Theme
        property: "config"
        value: Settings.theme
    }

    Binding {
        target: I18n
        property: "language"
        value: Settings.language
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
        pluginStates: core.pluginStates
    }
}
