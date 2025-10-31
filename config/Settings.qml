import QtQuick

QtObject {
    id: settings

    // === GENERAL ===
    property bool enableBar: true
    property bool enableClock: true

    // === APPEARANCE ===
    property string currentTheme: "rose-pine-main"

    property int spacing: 16
    property int radius: 8

    property int smallFontSize: 12
    property int mediumFontSize: 14
    property int largeFontSize: 16

    // === FUNCTIONS ===
    function saveSettings() {
        // TODO: Implementar guardado en archivo
    }

    function loadSettings() {
        // TODO: Implementar carga desde archivo
    }

    function resetSettings() {
        currentTheme = "rose-pine-main";
        enableBar = true;
        enableClock = true;
    }
}
