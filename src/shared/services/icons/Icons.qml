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

    function getDataUri(name, color, strokeWidth, bbox) {
        const content = provider.iconData[name];
        if (!content)
            return "";
        const hex = color.toString();
        let svg = content.replace(/currentColor/g, hex);
        if (strokeWidth !== undefined)
            svg = svg.replace(/stroke-width="[^"]*"/g, 'stroke-width="' + strokeWidth + '"');
        if (bbox) {
            const pad = 1;
            const vb = (bbox.x - pad) + " " + (bbox.y - pad) + " " + (bbox.w + pad * 2) + " " + (bbox.h + pad * 2);
            svg = svg.replace(/viewBox="[^"]*"/, 'viewBox="' + vb + '"');
        }
        return "data:image/svg+xml;charset=utf-8," + encodeURIComponent(svg);
    }

    property var bboxCache: ({})

    function getBBox(name) {
        if (provider.bboxCache[name] !== undefined)
            return provider.bboxCache[name];
        const content = provider.iconData[name];
        if (!content)
            return null;
        const bbox = provider.parseSvgBBox(content);
        provider.bboxCache[name] = bbox;
        return bbox;
    }

    function parseSvgBBox(svg) {
        let minX = 24, minY = 24, maxX = 0, maxY = 0, found = false;

        function expand(x, y) {
            found = true;
            if (x < minX)
                minX = x;
            if (x > maxX)
                maxX = x;
            if (y < minY)
                minY = y;
            if (y > maxY)
                maxY = y;
        }

        function attr(s, k, d) {
            const m = s.match(new RegExp('\\b' + k + '="([^"]*)"'));
            return m ? +m[1] : (d !== undefined ? d : 0);
        }

        function parsePath(d) {
            const toks = d.match(/[MmLlHhVvCcSsQqTtAaZz]|[-+]?(?:\d*\.\d+|\d+)(?:[eE][-+]?\d+)?/g);
            if (!toks)
                return;
            let i = 0, cmd = '', cx = 0, cy = 0, sx = 0, sy = 0;
            function n() {
                return parseFloat(toks[i++]);
            }
            while (i < toks.length) {
                if (/[MmLlHhVvCcSsQqTtAaZz]/.test(toks[i]))
                    cmd = toks[i++];
                switch (cmd) {
                case 'M':
                    cx = n();
                    cy = n();
                    sx = cx;
                    sy = cy;
                    expand(cx, cy);
                    cmd = 'L';
                    break;
                case 'm':
                    cx += n();
                    cy += n();
                    sx = cx;
                    sy = cy;
                    expand(cx, cy);
                    cmd = 'l';
                    break;
                case 'L':
                    cx = n();
                    cy = n();
                    expand(cx, cy);
                    break;
                case 'l':
                    cx += n();
                    cy += n();
                    expand(cx, cy);
                    break;
                case 'H':
                    cx = n();
                    expand(cx, cy);
                    break;
                case 'h':
                    cx += n();
                    expand(cx, cy);
                    break;
                case 'V':
                    cy = n();
                    expand(cx, cy);
                    break;
                case 'v':
                    cy += n();
                    expand(cx, cy);
                    break;
                case 'C':
                    {
                        const x1 = n(), y1 = n(), x2 = n(), y2 = n(), x = n(), y = n();
                        expand(x1, y1);
                        expand(x2, y2);
                        expand(x, y);
                        cx = x;
                        cy = y;
                        break;
                    }
                case 'c':
                    {
                        const dx1 = n(), dy1 = n(), dx2 = n(), dy2 = n(), dx = n(), dy = n();
                        expand(cx + dx1, cy + dy1);
                        expand(cx + dx2, cy + dy2);
                        cx += dx;
                        cy += dy;
                        expand(cx, cy);
                        break;
                    }
                case 'S':
                    {
                        const x2 = n(), y2 = n(), x = n(), y = n();
                        expand(x2, y2);
                        expand(x, y);
                        cx = x;
                        cy = y;
                        break;
                    }
                case 's':
                    {
                        const dx2 = n(), dy2 = n(), dx = n(), dy = n();
                        expand(cx + dx2, cy + dy2);
                        cx += dx;
                        cy += dy;
                        expand(cx, cy);
                        break;
                    }
                case 'Q':
                    {
                        const x1 = n(), y1 = n(), x = n(), y = n();
                        expand(x1, y1);
                        expand(x, y);
                        cx = x;
                        cy = y;
                        break;
                    }
                case 'q':
                    {
                        const dx1 = n(), dy1 = n(), dx = n(), dy = n();
                        expand(cx + dx1, cy + dy1);
                        cx += dx;
                        cy += dy;
                        expand(cx, cy);
                        break;
                    }
                case 'T':
                    cx = n();
                    cy = n();
                    expand(cx, cy);
                    break;
                case 't':
                    cx += n();
                    cy += n();
                    expand(cx, cy);
                    break;
                case 'A':
                    {
                        const rx = Math.abs(n()), ry = Math.abs(n());
                        n();
                        n();
                        n();
                        const ex = n(), ey = n();
                        expand(cx - rx, cy - ry);
                        expand(cx + rx, cy + ry);
                        expand(ex - rx, ey - ry);
                        expand(ex + rx, ey + ry);
                        cx = ex;
                        cy = ey;
                        break;
                    }
                case 'a':
                    {
                        const rx = Math.abs(n()), ry = Math.abs(n());
                        n();
                        n();
                        n();
                        const ex = cx + n(), ey = cy + n();
                        expand(cx - rx, cy - ry);
                        expand(cx + rx, cy + ry);
                        expand(ex - rx, ey - ry);
                        expand(ex + rx, ey + ry);
                        cx = ex;
                        cy = ey;
                        break;
                    }
                case 'Z':
                case 'z':
                    cx = sx;
                    cy = sy;
                    break;
                default:
                    i++;
                    break;
                }
            }
        }

        let m;
        const pathRe = /<path[^>]+\bd="([^"]+)"/g;
        while ((m = pathRe.exec(svg)) !== null)
            parsePath(m[1]);

        const lineRe = /<line([^/>]+)/g;
        while ((m = lineRe.exec(svg)) !== null) {
            const a = m[1];
            expand(attr(a, 'x1'), attr(a, 'y1'));
            expand(attr(a, 'x2'), attr(a, 'y2'));
        }

        const rectRe = /<rect([^/>]+)/g;
        while ((m = rectRe.exec(svg)) !== null) {
            const a = m[1];
            const x = attr(a, 'x'), y = attr(a, 'y'), w = attr(a, 'width'), h = attr(a, 'height');
            expand(x, y);
            expand(x + w, y + h);
        }

        const circleRe = /<circle([^/>]+)/g;
        while ((m = circleRe.exec(svg)) !== null) {
            const a = m[1];
            const cx = attr(a, 'cx', 12), cy = attr(a, 'cy', 12), r = attr(a, 'r');
            expand(cx - r, cy - r);
            expand(cx + r, cy + r);
        }

        const ellipseRe = /<ellipse([^/>]+)/g;
        while ((m = ellipseRe.exec(svg)) !== null) {
            const a = m[1];
            const cx = attr(a, 'cx', 12), cy = attr(a, 'cy', 12);
            const rx = attr(a, 'rx'), ry = attr(a, 'ry');
            expand(cx - rx, cy - ry);
            expand(cx + rx, cy + ry);
        }

        const polyRe = /<poly(?:line|gon)[^>]+points="([^"]+)"/g;
        while ((m = polyRe.exec(svg)) !== null) {
            const pts = m[1].trim().split(/[\s,]+/);
            for (let j = 0; j + 1 < pts.length; j += 2)
                expand(+pts[j], +pts[j + 1]);
        }

        return found ? {
            x: minX,
            y: minY,
            w: maxX - minX,
            h: maxY - minY
        } : null;
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
