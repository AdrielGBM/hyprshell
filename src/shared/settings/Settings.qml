pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: root

    property var framework: ({})
    property var background: ({})
    property var theme: ({})
    property string language: "en"

    readonly property var defaultSettings: ({
            framework: {},
            background: {},
            theme: {}
        })

    readonly property string settingsPath: `${Quickshell.env("HOME")}/.config/quickshell/settings.json`

    property FileView fileView: FileView {
        path: root.settingsPath
        watchChanges: true

        onLoaded: {
            try {
                const content = text();
                if (!content || content.trim().length === 0)
                    return;
                const data = JSON.parse(content);
                if (data.framework !== undefined)
                    root.framework = data.framework;
                if (data.background !== undefined)
                    root.background = data.background;
                if (data.theme !== undefined)
                    root.theme = data.theme;
                if (data.language !== undefined)
                    root.language = data.language;
            } catch (e) {
                console.error("Settings: parse error:", e.message);
            }
        }

        onFileChanged: reload()

        onLoadFailed: err => {
            if (err === FileViewError.FileNotFound) {
                root.framework = root.defaultSettings.framework;
                root.background = root.defaultSettings.background;
                root.theme = root.defaultSettings.theme;
                root.saveSettings();
            } else {
                console.error("Settings: read error:", FileViewError.toString(err));
            }
        }
    }

    property Process writeProcess: Process {
        running: false
        command: ["sh", "-c", ""]

        onExited: code => {
            if (code !== 0)
                console.error("Settings: save failed with code", code);
        }
    }

    function saveSettings() {
        const data = {
            "$schema": "./settings.schema.json",
            language: root.language,
            framework: root.framework,
            background: root.background,
            theme: root.theme
        };
        const json = JSON.stringify(data, null, 2);
        writeProcess.command = ["sh", "-c", `cat > '${root.settingsPath}' << 'QUICKSHELL_EOF'\n${json}\nQUICKSHELL_EOF`];
        writeProcess.running = true;
    }

    function save(section, values) {
        root[section] = Object.assign({}, root[section], values);
        saveSettings();
    }

    function saveLanguage(lang) {
        root.language = lang;
        saveSettings();
    }

    Component.onCompleted: fileView.reload()
}
