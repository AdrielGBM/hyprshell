import Quickshell

import "../modules/framework"
import "../modules/background"
import "../shared/providers"

Scope {
    id: core

    property SettingsProvider settingsProvider: SettingsProvider {}

    Background {
        config: core.settingsProvider.background
    }

    Framework {
        config: core.settingsProvider.framework
    }
}
