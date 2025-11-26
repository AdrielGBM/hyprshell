pragma ComponentBehavior: Bound

import QtQuick
import Quickshell

import "./services"
import "./providers"
import "../modules/status-bar"
import "../modules/settings"
import "../modules/wallpaper"

Scope {
    id: core

    property DependencyService dependencyService: DependencyService {}

    property SettingsProvider settings: SettingsProvider {}
    property ThemeProvider themeProvider: ThemeProvider {}

    Wallpaper {
        settings: core.settings
        themeProvider: core.themeProvider
        dependencyService: core.dependencyService
    }

    Connections {
        target: core.themeProvider
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
        themeProvider: core.themeProvider
        dependencyService: core.dependencyService
    }

    Loader {
        active: core.settings.enableBar
        sourceComponent: Component {
            StatusBar {
                settings: core.settings
                settingsWindow: core.settingsWindow
                themeProvider: core.themeProvider
            }
        }
    }
}
