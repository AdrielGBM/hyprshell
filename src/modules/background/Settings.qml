import QtQuick

QtObject {
    id: settings

    property var config: ({})

    readonly property string wallpaperPath: config.wallpaperPath ?? ""
}
