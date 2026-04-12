import QtQuick

QtObject {
    id: settings

    property string wallpaperPath: ""

    property var config: ({})
    onConfigChanged: applyConfig(config)

    function applyConfig(cfg) {
        if (!cfg)
            return;
        if (cfg.wallpaperPath !== undefined)
            wallpaperPath = cfg.wallpaperPath;
    }
}
