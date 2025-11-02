pragma ComponentBehavior: Bound

import QtQuick
import Quickshell

import "../modules/status-bar"
import "../modules/settings"
import "../config"
import "../themes"

Scope {
    id: core

    property Settings settings: Settings {}
    property ThemeManager themeManager: ThemeManager {}

    Connections {
        target: core.themeManager
        function onThemeChanged(newTheme, oldTheme) {
            if (core.settings.currentTheme !== newTheme) {
                core.settings.currentTheme = newTheme;
            }
        }
    }

    property SettingsWindow settingsWindow: SettingsWindow {
        settings: core.settings
        themeManager: core.themeManager
    }

    Loader {
        active: core.settings.enableBar
        sourceComponent: Component {
            StatusBar {
                settings: core.settings
                settingsWindow: core.settingsWindow
                themeManager: core.themeManager
            }
        }
    }
}
