pragma ComponentBehavior: Bound

import QtQuick
import "../../../core/components/inputs"

Column {
    id: themeSelector

    required property var themeManager
    required property var settings

    spacing: settings.spacing

    Text {
        text: "Tema"
        color: themeSelector.themeManager.accent1
        font.pixelSize: themeSelector.settings.mediumFontSize
        font.bold: true
    }

    Dropdown {
        width: parent.width
        themeManager: themeSelector.themeManager
        settings: themeSelector.settings
        currentValue: themeSelector.themeManager ? themeSelector.themeManager.currentThemeName : ""
        options: themeSelector.themeManager ? themeSelector.themeManager.getThemes() : []
        onValueChanged: function (newValue) {
            if (themeSelector.themeManager) {
                themeSelector.themeManager.setTheme(newValue);
            }
        }
    }

    Text {
        text: themeSelector.themeManager && themeSelector.themeManager.getThemeMeta(themeSelector.themeManager.currentThemeName) ? ("• " + themeSelector.themeManager.getThemeMeta(themeSelector.themeManager.currentThemeName).description) : "• Tema principal con tonos púrpura y rosa"
        color: themeSelector.themeManager.subtle
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        width: parent.width
    }
}
