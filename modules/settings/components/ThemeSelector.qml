pragma ComponentBehavior: Bound

import QtQuick
import "../../../core/components/inputs"

Column {
    id: themeSelector

    required property var themeProvider
    required property var settings

    spacing: settings.spacing

    Text {
        text: "Tema"
        color: themeSelector.themeProvider.accent1
        font.pixelSize: themeSelector.settings.mediumFontSize
        font.bold: true
    }

    Dropdown {
        width: parent.width
        themeProvider: themeSelector.themeProvider
        settings: themeSelector.settings
        currentValue: themeSelector.themeProvider ? themeSelector.themeProvider.currentThemeName : ""
        options: themeSelector.themeProvider ? themeSelector.themeProvider.getThemes() : []
        onValueChanged: function (newValue) {
            if (themeSelector.themeProvider) {
                themeSelector.themeProvider.setTheme(newValue);
            }
        }
    }

    Text {
        text: themeSelector.themeProvider && themeSelector.themeProvider.getThemeMeta(themeSelector.themeProvider.currentThemeName) ? ("• " + themeSelector.themeProvider.getThemeMeta(themeSelector.themeProvider.currentThemeName).description) : "• Tema principal con tonos púrpura y rosa"
        color: themeSelector.themeProvider.subtle
        font.pixelSize: 12
        wrapMode: Text.WordWrap
        width: parent.width
    }
}
