import QtQuick
import Quickshell.Io
import Qt.labs.folderlistmodel

QtObject {
    id: loader

    property string pluginId: ""
    property string basePath: ""
    property string language: "en"

    signal done(string id, var data)

    property FolderListModel _scanner: FolderListModel {
        showFiles: true
        showDirs: false
        showDotAndDotDot: false
        nameFilters: ["*.json"]

        onStatusChanged: {
            if (status !== FolderListModel.Ready || count === 0)
                return;

            const available = [];
            for (let i = 0; i < count; i++) {
                const fname = get(i, "fileName");
                if (fname.endsWith(".json"))
                    available.push(fname.slice(0, -5));
            }

            const fallbacks = [loader.language, "en", "es"];
            let chosen = null;
            for (const lang of fallbacks) {
                if (available.indexOf(lang) !== -1) {
                    chosen = lang;
                    break;
                }
            }
            if (!chosen)
                chosen = available[0];

            _view.path = loader.basePath + chosen + ".json";
            _view.reload();
        }
    }

    property FileView _view: FileView {
        watchChanges: false

        onLoaded: {
            const content = text();
            if (!content || content.trim().length === 0)
                return;
            try {
                loader.done(loader.pluginId, JSON.parse(content));
            } catch (e) {
                console.error("I18n: parse error for", loader.pluginId, e.message);
            }
        }
    }

    Component.onCompleted: {
        if (basePath)
            _scanner.folder = Qt.resolvedUrl(basePath);
    }
}
