import QtQuick
import Qt.labs.folderlistmodel

QtObject {
    id: scanner

    property url folder
    property string filename

    signal dirFound(string key)
    signal itemReady(string key, var component)
    signal itemError(string key, string error)

    property var model: FolderListModel {
        folder: scanner.folder
        showDirs: true
        showFiles: false
        showDotAndDotDot: false

        onStatusChanged: {
            if (status !== FolderListModel.Ready)
                return;

            for (let i = 0; i < count; i++) {
                (function (key) {
                        scanner.dirFound(key);

                        const url = scanner.folder + key + "/" + scanner.filename;
                        const comp = Qt.createComponent(url);

                        function tryEmit(c) {
                            if (c.status === Component.Ready)
                                scanner.itemReady(key, c);
                            else if (c.status === Component.Error)
                                scanner.itemError(key, c.errorString());
                        }

                        if (comp.status === Component.Loading)
                            comp.statusChanged.connect(function () {
                                tryEmit(comp);
                            });
                        else
                            tryEmit(comp);
                    })(get(i, "fileName"));
            }
        }
    }
}
