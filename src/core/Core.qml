import Quickshell

import "../modules/framework"
import "../modules/background"
import "../shared/providers"

Scope {
    id: core

    property SettingsProvider settingsProvider: SettingsProvider {}
    property ThemeProvider themeProvider: ThemeProvider {
        config: core.settingsProvider.theme
    }
    property IconProvider iconProvider: IconProvider {}

    Background {
        themeProvider: core.themeProvider
        config: core.settingsProvider.background
    }

    Framework {
        themeProvider: core.themeProvider
        iconProvider: core.iconProvider
        config: core.settingsProvider.framework
        saveConfig: values => core.settingsProvider.save("framework", values)
    }
}
