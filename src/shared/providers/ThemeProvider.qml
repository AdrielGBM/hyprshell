import QtQuick
import "../services"

QtObject {
    id: themeProvider

    property var config: ({})
    onConfigChanged: {
        if (config?.name !== undefined)
            setTheme(config.name);
    }

    readonly property var fallback: ({
            base: "#191724",
            surface: "#1f1d2e",
            overlay: "#26233a",
            text: "#e0def4",
            subtle: "#908caa",
            muted: "#6e6a86",
            accent1: "#eb6f92",
            accent2: "#f6c177",
            accent3: "#ebbcba",
            accent4: "#31748f",
            accent5: "#9ccfd8",
            accent6: "#c4a7e7",
            success: "#31748f",
            warning: "#f6c177",
            error: "#eb6f92",
            info: "#9ccfd8",
            highlightLow: "#21202e",
            highlightMed: "#403d52",
            highlightHigh: "#524f67"
        })

    readonly property var active: currentTheme !== null ? currentTheme : fallback

    property var themeList: []
    property var themes: ({})

    property string pendingThemeName: "rose-pine-main"

    property string currentThemeName: ""
    property var currentTheme: null

    readonly property color base: active["base"]
    readonly property color surface: active["surface"]
    readonly property color overlay: active["overlay"]
    readonly property color text: active["text"]
    readonly property color subtle: active["subtle"]
    readonly property color muted: active["muted"]

    readonly property color accent: active["accent"] ?? accent6
    readonly property color accent1: active["accent1"]
    readonly property color accent2: active["accent2"]
    readonly property color accent3: active["accent3"]
    readonly property color accent4: active["accent4"]
    readonly property color accent5: active["accent5"]
    readonly property color accent6: active["accent6"]

    readonly property color success: active["success"]
    readonly property color warning: active["warning"]
    readonly property color error: active["error"]
    readonly property color info: active["info"]

    readonly property color highlightLow: active["highlightLow"]
    readonly property color highlightMed: active["highlightMed"]
    readonly property color highlightHigh: active["highlightHigh"]

    readonly property int spacing: config?.spacing ?? 8
    readonly property int radius: config?.radius ?? 8
    readonly property var font: config?.font ?? ({
            family: "",
            size: 12
        })

    readonly property string defaultAccent: config?.accent ?? ""
    readonly property string defaultVariant: config?.variant ?? "default"
    readonly property color defaultAccentColor: {
        if (defaultAccent !== "")
            return themeProvider[defaultAccent] ?? accent1;
        return accent1;
    }

    property var themeScanner: FolderScanner {
        folder: Qt.resolvedUrl("../../../assets/themes/")
        filename: "Theme.qml"

        onItemReady: function (key, comp) {
            const obj = comp.createObject(themeProvider);
            if (!obj)
                return;
            const entry = {
                key: key,
                name: obj.name || key,
                author: obj.author || "Unknown",
                description: obj.description || "",
                version: obj.version || "1.0.0",
                theme: obj
            };
            const list = themeProvider.themeList.slice();
            list.push(entry);
            const map = Object.assign({}, themeProvider.themes);
            map[key] = entry;
            themeProvider.themeList = list;
            themeProvider.themes = map;
            if (themeProvider.pendingThemeName && map[themeProvider.pendingThemeName])
                themeProvider.setTheme(themeProvider.pendingThemeName);
        }

        onItemError: function (key, error) {
            console.error("Theme load error [" + key + "]:", error);
        }
    }

    function getThemes() {
        return themeProvider.themeList;
    }

    function setTheme(themeName) {
        if (themeProvider.themes.hasOwnProperty(themeName)) {
            const oldTheme = currentThemeName;
            currentThemeName = themeName;
            currentTheme = themeProvider.themes[themeName].theme;
            pendingThemeName = "";
            themeChanged(themeName, oldTheme);
            return true;
        }
        pendingThemeName = themeName;
        return false;
    }

    function getThemeMeta(themeName) {
        return themeProvider.themes[themeName] || null;
    }

    signal themeChanged(string newTheme, string oldTheme)
}
