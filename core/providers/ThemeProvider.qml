import QtQuick

QtObject {
    id: themeManager

    // === THEMES ===
    property var themeList: (function () {
            var themes = [
                {
                    key: "rose-pine-main"
                },
                {
                    key: "rose-pine-moon"
                },
                {
                    key: "rose-pine-dawn"
                }
            ];

            var result = [];
            for (var i = 0; i < themes.length; i++) {
                var themeObj = Qt.createComponent("../../assets/themes/" + themes[i].key + "/Theme.qml").createObject(themeManager);
                if (themeObj) {
                    result.push({
                        key: themes[i].key,
                        name: themeObj.name || themes[i].key,
                        author: themeObj.author || "Unknown",
                        description: themeObj.description || "No description available",
                        version: themeObj.version || "1.0.0",
                        theme: themeObj
                    });
                }
            }
            return result;
        })()

    property var themes: (function () {
            var map = {};
            for (var i = 0; i < themeManager.themeList.length; ++i) {
                map[themeManager.themeList[i].key] = themeManager.themeList[i];
            }
            return map;
        })()

    property string currentThemeName: "rose-pine-main"
    property var currentTheme: themeManager.themes[themeManager.currentThemeName].theme

    // === THEME ===
    readonly property color base: currentTheme && currentTheme.base ? currentTheme.base : "#191724"
    readonly property color surface: currentTheme && currentTheme.surface ? currentTheme.surface : "#1f1d2e"
    readonly property color overlay: currentTheme && currentTheme.overlay ? currentTheme.overlay : "#26233a"
    readonly property color text: currentTheme && currentTheme.text ? currentTheme.text : "#e0def4"
    readonly property color subtle: currentTheme && currentTheme.subtle ? currentTheme.subtle : "#908caa"
    readonly property color muted: currentTheme && currentTheme.muted ? currentTheme.muted : "#6e6a86"

    readonly property color accent1: currentTheme && currentTheme.accent1 ? currentTheme.accent1 : "#eb6f92"
    readonly property color accent2: currentTheme && currentTheme.accent2 ? currentTheme.accent2 : "#f6c177"
    readonly property color accent3: currentTheme && currentTheme.accent3 ? currentTheme.accent3 : "#ebbcba"
    readonly property color accent4: currentTheme && currentTheme.accent4 ? currentTheme.accent4 : "#31748f"
    readonly property color accent5: currentTheme && currentTheme.accent5 ? currentTheme.accent5 : "#9ccfd8"
    readonly property color accent6: currentTheme && currentTheme.accent6 ? currentTheme.accent6 : "#c4a7e7"

    readonly property color success: currentTheme && currentTheme.success ? currentTheme.success : "#31748f"
    readonly property color warning: currentTheme && currentTheme.warning ? currentTheme.warning : "#f6c177"
    readonly property color error: currentTheme && currentTheme.error ? currentTheme.error : "#eb6f92"
    readonly property color info: currentTheme && currentTheme.info ? currentTheme.info : "#9ccfd8"

    readonly property color highlightLow: currentTheme && currentTheme.highlightLow ? currentTheme.highlightLow : "#21202e"
    readonly property color highlightMed: currentTheme && currentTheme.highlightMed ? currentTheme.highlightMed : "#403d52"
    readonly property color highlightHigh: currentTheme && currentTheme.highlightHigh ? currentTheme.highlightHigh : "#524f67"

    // === FUNCTIONS ===
    function getThemes() {
        return themeManager.themeList;
    }

    function setTheme(themeName) {
        if (themeManager.themes.hasOwnProperty(themeName)) {
            var oldTheme = currentThemeName;
            currentThemeName = themeName;
            currentTheme = themeManager.themes[themeName].theme;
            themeChanged(themeName, oldTheme);
            return true;
        } else {
            return false;
        }
    }

    function getThemeMeta(themeName) {
        return themeManager.themes[themeName] || null;
    }

    // === SIGNALS ===
    signal themeChanged(string newTheme, string oldTheme)
}
