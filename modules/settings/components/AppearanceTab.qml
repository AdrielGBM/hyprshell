pragma ComponentBehavior: Bound

import QtQuick

Column {
    id: appearanceTab

    required property var themeManager
    required property var settings

    spacing: settings.spacing

    ThemeSelector {
        width: parent.width
        themeManager: appearanceTab.themeManager
        settings: appearanceTab.settings
    }

    RadiusControl {
        width: parent.width
        themeManager: appearanceTab.themeManager
        settings: appearanceTab.settings
    }
}
