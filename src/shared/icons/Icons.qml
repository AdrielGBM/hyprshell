pragma Singleton
import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: provider

    readonly property string baseUrl: "https://unpkg.com/lucide-static@latest/icons"
    readonly property string indexUrl: "https://unpkg.com/lucide-static@latest/icons/?meta"
    readonly property string cacheDir: Quickshell.env("HOME") + "/.config/quickshell/.cache/icons/lucide"
    readonly property string indexPath: provider.cacheDir + "/index.json"

    property var iconData: ({})
    property var queue: []
    property var queued: ({})
    property int activeCount: 0
    readonly property int concurrency: 3

    property var iconList: []
    property bool iconListLoading: false

    signal iconReady(string name)
    signal iconFailed(string name)
    signal iconListReady

    property Process mkdir: Process {
        command: ["mkdir", "-p", provider.cacheDir]
        running: true
        onExited: code => {
            if (code === 0)
                provider.loadIconList();
            else
                console.error("IconProvider: could not create cache dir:", provider.cacheDir);
        }
    }

    property Process indexDownloader: Process {
        running: false
        onExited: code => {
            if (code === 0) {
                provider.readIndexFile();
            } else {
                console.error("IconProvider: failed to fetch icon list (exit " + code + ")");
                provider.iconListLoading = false;
            }
        }
    }

    function request(name) {
        if (!name || name === "")
            return;
        if (provider.iconData[name] !== undefined) {
            provider.iconReady(name);
            return;
        }
        if (provider.queued[name])
            return;
        provider.queued[name] = true;
        provider.queue.push(name);
        provider.next();
    }

    function getContent(name) {
        return provider.iconData[name] || null;
    }

    function getDataUri(name, color) {
        const content = provider.iconData[name];
        if (!content)
            return "";
        const hex = color.toString();
        const colored = content.replace(/currentColor/g, hex);
        return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(colored);
    }

    function loadIconList() {
        provider.readIndexFile();
    }

    function readIndexFile() {
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
                provider.fetchIconList();
            }
        });

        fv.onLoadFailed.connect(err => {
            fv.destroy();
            if (err === FileViewError.FileNotFound)
                provider.fetchIconList();
            else
                console.error("IconProvider: error reading index.json");
        });

        fv.reload();
    }

    function fetchIconList() {
        provider.iconListLoading = true;
        provider.indexDownloader.command = ["curl", "-sL", "-f", "--max-time", "30", "-o", provider.indexPath, provider.indexUrl];
        provider.indexDownloader.running = true;
    }

    function next() {
        while (provider.queue.length > 0 && provider.activeCount < provider.concurrency) {
            provider.activeCount++;
            const name = provider.queue.shift();
            provider.readFile(name, false);
        }
    }

    function completeSlot(name) {
        provider.activeCount--;
        provider.next();
    }

    function readFile(name, isRetry) {
        const fv = Qt.createQmlObject('import Quickshell.Io; FileView { watchChanges: false }', provider);
        fv.path = provider.cacheDir + "/" + name + ".svg";

        fv.onLoaded.connect(() => {
            const content = fv.text();
            fv.destroy();

            if (!content || !content.includes('<svg')) {
                console.warn("IconProvider: cached file for '" + name + "' is not a valid SVG — deleting");
                const rm = Qt.createQmlObject('import Quickshell.Io; Process { running: false }', provider);
                rm.command = ["rm", "-f", provider.cacheDir + "/" + name + ".svg"];
                rm.onExited.connect(() => rm.destroy());
                rm.running = true;
                delete provider.queued[name];
                provider.iconFailed(name);
                provider.completeSlot(name);
                return;
            }

            provider.iconData[name] = content;
            delete provider.queued[name];
            provider.iconReady(name);
            provider.completeSlot(name);
        });

        fv.onLoadFailed.connect(err => {
            fv.destroy();
            if (!isRetry && err === FileViewError.FileNotFound) {
                provider.downloadIcon(name);
            } else {
                console.error("IconProvider: unexpected read error for '" + name + "'");
                delete provider.queued[name];
                provider.iconFailed(name);
                provider.completeSlot(name);
            }
        });

        fv.reload();
    }

    function downloadIcon(name) {
        const path = provider.cacheDir + "/" + name + ".svg";
        const url = provider.baseUrl + "/" + name + ".svg";

        const proc = Qt.createQmlObject('import Quickshell.Io; Process { running: false }', provider);
        proc.command = ["curl", "-sL", "-f", "--max-time", "15", "-o", path, url];

        proc.onExited.connect(code => {
            proc.destroy();
            if (code === 0) {
                provider.readFile(name, true);
            } else {
                console.warn("IconProvider: download failed for '" + name + "' (exit " + code + ")");
                delete provider.queued[name];
                provider.iconFailed(name);
                provider.completeSlot(name);
            }
        });

        proc.running = true;
    }
}
