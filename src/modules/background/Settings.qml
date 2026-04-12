import QtQuick

QtObject {
    id: settings

    property string wallpaperPath: ""
    property string backgroundColor: "#191724"

    property var config: ({})
    onConfigChanged: applyConfig(config)

    function applyConfig(cfg) {
        if (!cfg)
            return;
        if (cfg.wallpaperPath !== undefined)
            wallpaperPath = cfg.wallpaperPath;
        if (cfg.backgroundColor !== undefined)
            backgroundColor = cfg.backgroundColor;
    }
}
