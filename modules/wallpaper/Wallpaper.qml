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

    function applyWallpaper() {
        const path = settings.wallpaperPath;

        if (path && path.length > 0) {
            const cmd = `hyprctl hyprpaper preload "${path}" && hyprctl hyprpaper wallpaper ",${path}"`;
            hyprpaperProcess.command = ["sh", "-c", cmd];
            hyprpaperProcess.running = true;
        }
    }

    Component.onCompleted: {
        if (settings.wallpaperPath && settings.wallpaperPath.length > 0) {
            applyWallpaper();
        }
    }
}
