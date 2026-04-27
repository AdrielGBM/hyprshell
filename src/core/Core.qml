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
    property I18nProvider i18nProvider: I18nProvider {
        language: core.settingsProvider.language
    }

    Background {
        themeProvider: core.themeProvider
        config: core.settingsProvider.background
    }

    Framework {
        themeProvider: core.themeProvider
        iconProvider: core.iconProvider
        settingsProvider: core.settingsProvider
        i18nProvider: core.i18nProvider
    }
}
