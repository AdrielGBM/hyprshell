pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick

Scope {
    id: wallpaper

    property var settings: null
    property var themeManager: null
    property var dependencyManager: null

    readonly property string hyprpaperDependencyName: "hyprpaper"

    property Process hyprpaperDaemon: Process {
        running: true
        command: ["hyprpaper"]

        onExited: (exitCode, exitStatus) => {
            console.warn("Wallpaper: hyprpaper daemon terminó con código", exitCode);
            Qt.callLater(() => {
                if (!hyprpaperDaemon.running) {
                    hyprpaperDaemon.running = true;
                }
            });
        }

        onStarted: () => {
            console.log("Wallpaper: hyprpaper daemon iniciado");
            const timer = Qt.createQmlObject('import QtQuick; Timer { interval: 500; running: true; repeat: false }', wallpaper);
            timer.triggered.connect(() => {
                wallpaper.applyWallpaper();
                timer.destroy();
            });
        }
    }

    Component.onCompleted: {
        if (dependencyManager) {
            dependencyManager.registerDependency("hyprctl", "command", {
                checkCommand: ["which", "hyprctl"],
                retryInterval: 5000,
                maxRetries: 3,
                onReady: () => {
                    console.log("Wallpaper: hyprctl está disponible");
                }
            });
        } else {
            console.error("Wallpaper: DependencyManager no está disponible");
        }
    }

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

    function applyWallpaper() {
        const path = settings.wallpaperPath;
        if (!path || path.length === 0) {
            console.warn("Wallpaper: No hay ruta de wallpaper configurada");
            return;
        }

        if (!hyprpaperDaemon.running) {
            console.warn("Wallpaper: hyprpaper daemon no está ejecutándose");
            return;
        }

        const cmd = `hyprctl hyprpaper preload "${path}" && hyprctl hyprpaper wallpaper ",${path}"`;
        hyprpaperProcess.command = ["sh", "-c", cmd];
        hyprpaperProcess.running = true;
    }
}
