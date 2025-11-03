import QtQuick
import Quickshell
import Quickshell.Io

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

    // === FILE VIEW ===
    readonly property string settingsPath: `${Quickshell.env("HOME")}/.config/quickshell/settings.json`

    property FileView fileView: FileView {
        path: settings.settingsPath
        watchChanges: true

        onFileChanged: {
            settings.loadSettings();
        }

        onLoaded: {
            try {
                const content = text();
                if (content && content.trim().length > 0) {
                    const data = JSON.parse(content);
                    settings.applySettings(data);
                    console.log("Settings loaded successfully");
                }
            } catch (e) {
                console.error("Failed to load settings:", e.message);
                console.error("File content:", text());
            }
        }

        onLoadFailed: err => {
            if (err !== FileViewError.FileNotFound) {
                console.error("Failed to read settings file:", FileViewError.toString(err));
            } else {
                console.log("Settings file not found, using defaults");
            }
        }

        onSaveFailed: err => {
            console.error("Failed to save settings:", FileViewError.toString(err));
        }
    }

    property Process writeProcess: Process {
        running: false
        command: ["sh", "-c", ""]

        property string jsonData: ""
    }

    // === FUNCTIONS ===
    function saveSettings() {
        const settingsObj = {
            enableBar: enableBar,
            enableClock: enableClock,
            currentTheme: currentTheme,
            spacing: spacing,
            radius: radius,
            smallFontSize: smallFontSize,
            mediumFontSize: mediumFontSize,
            largeFontSize: largeFontSize
        };

        const json = JSON.stringify(settingsObj, null, 2);
        writeProcess.jsonData = json;
        writeProcess.command = ["sh", "-c", `cat > '${settings.settingsPath}' << 'EOF'\n${json}\nEOF`];
        writeProcess.running = true;
    }

    function loadSettings() {
        fileView.reload();
    }

    function applySettings(obj) {
        if (!obj)
            return;
        if (obj.enableBar !== undefined)
            enableBar = obj.enableBar;
        if (obj.enableClock !== undefined)
            enableClock = obj.enableClock;
        if (obj.currentTheme !== undefined)
            currentTheme = obj.currentTheme;
        if (obj.spacing !== undefined)
            spacing = obj.spacing;
        if (obj.radius !== undefined)
            radius = obj.radius;
        if (obj.smallFontSize !== undefined)
            smallFontSize = obj.smallFontSize;
        if (obj.mediumFontSize !== undefined)
            mediumFontSize = obj.mediumFontSize;
        if (obj.largeFontSize !== undefined)
            largeFontSize = obj.largeFontSize;
    }

    function resetSettings() {
        currentTheme = "rose-pine-main";
        enableBar = true;
        enableClock = true;
        spacing = 16;
        radius = 8;
        smallFontSize = 12;
        mediumFontSize = 14;
        largeFontSize = 16;
        saveSettings();
    }

    Component.onCompleted: {
        loadSettings();
    }
}
