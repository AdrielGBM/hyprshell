import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: provider

    property var framework: ({})
    property var background: ({})
    property var theme: ({})

    readonly property var defaultSettings: ({
        framework: {},
        background: {},
        theme: {}
    })

    readonly property string settingsPath: `${Quickshell.env("HOME")}/.config/quickshell/settings.json`

    property FileView fileView: FileView {
        path: provider.settingsPath
        watchChanges: true

        onLoaded: {
            try {
                const content = text();
                if (!content || content.trim().length === 0)
                    return;
                const data = JSON.parse(content);
                if (data.framework !== undefined)
                    provider.framework = data.framework;
                if (data.background !== undefined)
                    provider.background = data.background;
                if (data.theme !== undefined)
                    provider.theme = data.theme;
            } catch (e) {
                console.error("SettingsProvider: parse error:", e.message);
            }
        }

        onFileChanged: reload()

        onLoadFailed: err => {
            if (err === FileViewError.FileNotFound) {
                provider.framework = provider.defaultSettings.framework;
                provider.background = provider.defaultSettings.background;
                provider.theme = provider.defaultSettings.theme;
                provider.saveSettings();
            } else {
                console.error("SettingsProvider: read error:", FileViewError.toString(err));
            }
        }
    }

    property Process writeProcess: Process {
        running: false
        command: ["sh", "-c", ""]

        onExited: (code, status) => {
            if (code !== 0)
                console.error("SettingsProvider: save failed with code", code);
        }
    }

    function saveSettings() {
        const data = {
            "$schema": "./settings.schema.json",
            framework: provider.framework,
            background: provider.background,
            theme: provider.theme
        };
        const json = JSON.stringify(data, null, 2);
        writeProcess.command = ["sh", "-c", `cat > '${provider.settingsPath}' << 'QUICKSHELL_EOF'\n${json}\nQUICKSHELL_EOF`];
        writeProcess.running = true;
    }

    function save(section, values) {
        provider[section] = Object.assign({}, provider[section], values);
        saveSettings();
    }

    Component.onCompleted: fileView.reload()
}
