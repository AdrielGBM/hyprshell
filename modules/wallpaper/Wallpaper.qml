pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick

Scope {
    id: wallpaper

    property var settings: null
    property var themeManager: null

    Connections {
        target: wallpaper.settings
        function onWallpaperPathChanged() {
            wallpaper.applyWallpaper();
        }
    }

    property Process hyprpaperProcess: Process {
        running: false
        command: ["sh", "-c", ""]
    }

    property Process hyprpaperDaemon: Process {
        running: false
        command: ["hyprpaper"]

        onExited: {
            Qt.callLater(() => {
                if (!hyprpaperDaemon.running) {
                    hyprpaperDaemon.running = true;
                }
            });
        }
    }

    function hyprpaper(callback) {
        if (!hyprpaperDaemon.running) {
            hyprpaperDaemon.running = true;
            const timer = Qt.createQmlObject('import QtQuick; Timer { interval: 50; running: true; repeat: false }', wallpaper);
            timer.triggered.connect(() => {
                callback();
                timer.destroy();
            });
        } else {
            callback();
        }
    }

    function applyWallpaper() {
        const path = settings.wallpaperPath;

        hyprpaper(() => {
            if (path && path.length > 0) {
                const cmd = `hyprctl hyprpaper preload "${path}" && hyprctl hyprpaper wallpaper ",${path}"`;
                hyprpaperProcess.command = ["sh", "-c", cmd];
                hyprpaperProcess.running = true;
            }
        });
    }

    Component.onCompleted: {
        applyWallpaper();
    }
}
