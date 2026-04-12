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

    Background {
        config: core.settingsProvider.background
        themeProvider: core.themeProvider
    }

    Framework {
        config: core.settingsProvider.framework
        themeProvider: core.themeProvider
    }
}
