import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: provider

    readonly property string baseUrl: "https://unpkg.com/lucide-static@latest/icons"
    readonly property string indexUrl: "https://unpkg.com/lucide-static@latest/icons/?meta"
    readonly property string cacheDir: Quickshell.env("HOME") + "/.config/quickshell/.cache/icons/lucide"
    readonly property string indexPath: provider.cacheDir + "/index.json"

    property var _iconData: ({})
    property var _queue: []
    property var _queued: ({})
    property bool _processing: false
    property string _currentName: ""

    property var iconList: []
    property bool iconListLoading: false

    signal iconReady(string name)
    signal iconFailed(string name)
    signal iconListReady

    property Process _mkdir: Process {
        command: ["mkdir", "-p", provider.cacheDir]
        running: true
        onExited: code => {
            if (code === 0)
                provider._loadIconList();
            else
                console.error("IconProvider: could not create cache dir:", provider.cacheDir);
        }
    }

    property Process _downloader: Process {
        running: false
        onExited: code => {
            if (code === 0) {
                provider._readFile(provider._currentName);
            } else {
                console.warn("IconProvider: download failed for '" + provider._currentName + "' (exit " + code + ")");
                delete provider._queued[provider._currentName];
                provider.iconFailed(provider._currentName);
                provider._processing = false;
                provider._next();
            }
        }
    }

    property Process _indexDownloader: Process {
        running: false
        onExited: code => {
            if (code === 0) {
                provider._readIndexFile();
            } else {
                console.error("IconProvider: failed to fetch icon list (exit " + code + ")");
                provider.iconListLoading = false;
            }
        }
    }

    function request(name) {
        if (!name || name === "")
            return;
        if (provider._iconData[name] !== undefined) {
            provider.iconReady(name);
            return;
        }
        if (provider._queued[name])
            return;
        provider._queued[name] = true;
        provider._queue.push(name);
        if (!provider._processing)
            provider._next();
    }

    function getContent(name) {
        return provider._iconData[name] || null;
    }

    function getDataUri(name, color) {
        const content = provider._iconData[name];
        if (!content)
            return "";
        const hex = color.toString();
        const colored = content.replace(/currentColor/g, hex);
        return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(colored);
    }

    function _loadIconList() {
        provider._readIndexFile();
    }

    function _readIndexFile() {
        const fv = Qt.createQmlObject('import Quickshell.Io; FileView { watchChanges: false }', provider);
        fv.path = provider.indexPath;

        fv.onLoaded.connect(() => {
            const content = fv.text();
            fv.destroy();
            try {
                const data = JSON.parse(content);
                const icons = data.files.filter(f => f.type === "file" && f.path.endsWith('.svg')).map(f => f.path.split('/').pop().replace('.svg', '')).sort();
                provider.iconList = icons;
                provider.iconListLoading = false;
                provider.iconListReady();
            } catch (e) {
                console.error("IconProvider: failed to parse index.json:", e.message, "— re-fetching");
                provider._fetchIconList();
            }
        });

        fv.onLoadFailed.connect(err => {
            fv.destroy();
            if (err === FileViewError.FileNotFound)
                provider._fetchIconList();
            else
                console.error("IconProvider: error reading index.json");
        });

        fv.reload();
    }

    function _fetchIconList() {
        provider.iconListLoading = true;
        provider._indexDownloader.command = ["curl", "-sL", "-f", "--max-time", "30", "-o", provider.indexPath, provider.indexUrl];
        provider._indexDownloader.running = true;
    }

    function _next() {
        if (provider._queue.length === 0) {
            provider._processing = false;
            return;
        }
        provider._processing = true;
        provider._currentName = provider._queue.shift();
        provider._readFile(provider._currentName);
    }

    function _readFile(name) {
        const fv = Qt.createQmlObject('import Quickshell.Io; FileView { watchChanges: false }', provider);
        fv.path = provider.cacheDir + "/" + name + ".svg";

        fv.onLoaded.connect(() => {
            const content = fv.text();
            fv.destroy();

            if (!content || !content.includes('<svg')) {
                console.warn("IconProvider: cached file for '" + name + "' is not a valid SVG — deleting");
                const rm = Qt.createQmlObject('import Quickshell.Io; Process { running: false }', provider);
                rm.command = ["rm", "-f", provider.cacheDir + "/" + name + ".svg"];
                rm.running = true;
                delete provider._queued[name];
                provider.iconFailed(name);
                provider._processing = false;
                provider._next();
                return;
            }

            provider._iconData[name] = content;
            delete provider._queued[name];
            provider.iconReady(name);
            provider._processing = false;
            provider._next();
        });

        fv.onLoadFailed.connect(err => {
            fv.destroy();
            if (err === FileViewError.FileNotFound)
                provider._downloadIcon(name);
            else {
                console.error("IconProvider: unexpected read error for '" + name + "'");
                delete provider._queued[name];
                provider.iconFailed(name);
                provider._processing = false;
                provider._next();
            }
        });

        fv.reload();
    }

    function _downloadIcon(name) {
        const path = provider.cacheDir + "/" + name + ".svg";
        const url = provider.baseUrl + "/" + name + ".svg";

        provider._downloader.command = ["curl", "-sL", "-f", "--max-time", "15", "-o", path, url];
        provider._downloader.running = true;
    }
}
