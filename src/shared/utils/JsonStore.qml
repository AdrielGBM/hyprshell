import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: store

    required property string filePath
    property bool watchChanges: false

    property var data: null
    property bool loaded: false

    signal dataLoaded
    signal loadFailed(var error)
    signal saveFailed(int code)

    property Process mkdirProcess: Process {
        running: false
        onExited: code => {
            if (code === 0)
                store.fileView.reload();
            else
                console.error("JsonStore: could not create directory for:", store.filePath);
        }
    }

    property FileView fileView: FileView {
        path: store.filePath
        watchChanges: store.watchChanges

        onLoaded: {
            try {
                const content = text();
                if (!content || content.trim().length === 0) {
                    store.loaded = true;
                    store.dataLoaded();
                    return;
                }
                store.data = JSON.parse(content);
                store.loaded = true;
                store.dataLoaded();
            } catch (e) {
                console.error("JsonStore: parse error in", store.filePath, ":", e.message);
            }
        }

        onFileChanged: {
            if (!store._isSaving)
                reload();
        }

        onLoadFailed: err => {
            if (err === FileViewError.FileNotFound) {
                store.loaded = true;
                store.dataLoaded();
            } else {
                console.error("JsonStore: read error:", FileViewError.toString(err));
                store.loadFailed(err);
            }
        }
    }

    property bool _isSaving: false
    property var _pendingSave: undefined

    property Process writeProcess: Process {
        running: false
        command: ["sh", "-c", ""]

        onExited: code => {
            store._isSaving = false;
            if (code !== 0) {
                console.error("JsonStore: save failed (exit", code + ") for:", store.filePath);
                store.saveFailed(code);
            }
            if (store._pendingSave !== undefined) {
                const pending = store._pendingSave;
                store._pendingSave = undefined;
                store._doSave(pending);
            }
        }
    }

    function _doSave(newData) {
        const json = JSON.stringify(newData, null, 2);
        store._isSaving = true;
        writeProcess.command = ["sh", "-c", `cat > '${store.filePath}' << 'QUICKSHELL_EOF'\n${json}\nQUICKSHELL_EOF`];
        writeProcess.running = true;
    }

    function save(newData) {
        store.data = newData;
        if (store._isSaving) {
            store._pendingSave = newData;
            return;
        }
        store._doSave(newData);
    }

    Component.onCompleted: {
        const dir = store.filePath.substring(0, store.filePath.lastIndexOf('/'));
        mkdirProcess.command = ["mkdir", "-p", dir];
        mkdirProcess.running = true;
    }
}
