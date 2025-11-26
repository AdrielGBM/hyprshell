pragma ComponentBehavior: Bound

import QtQuick
import Quickshell

import "../modules/status-bar"
import "../modules/settings"
import "../modules/wallpaper"
import "../config"
import "../themes"

Scope {
    id: core

    property DependencyManager dependencyManager: DependencyManager {}

    property Settings settings: Settings {}
    property ThemeManager themeManager: ThemeManager {}

    Wallpaper {
        settings: core.settings
        themeManager: core.themeManager
        dependencyManager: core.dependencyManager
    }

    Connections {
        target: core.themeManager
        function onThemeChanged(newTheme, oldTheme) {
            if (core.settings.currentTheme !== newTheme) {
                core.settings.currentTheme = newTheme;
                core.settings.saveSettings();
            }
        }
    }

    Connections {
        target: core.settings
        function onRadiusChanged() {
            core.settings.saveSettings();
        }
        function onSpacingChanged() {
            core.settings.saveSettings();
        }
        function onEnableBarChanged() {
            core.settings.saveSettings();
        }
        function onEnableClockChanged() {
            core.settings.saveSettings();
        }
        function onSmallFontSizeChanged() {
            core.settings.saveSettings();
        }
        function onMediumFontSizeChanged() {
            core.settings.saveSettings();
        }
        function onLargeFontSizeChanged() {
            core.settings.saveSettings();
        }
        function onWallpaperPathChanged() {
            core.settings.saveSettings();
        }
    }

    property SettingsWindow settingsWindow: SettingsWindow {
        settings: core.settings
        themeManager: core.themeManager
        dependencyManager: core.dependencyManager
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
