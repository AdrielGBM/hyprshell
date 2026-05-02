pragma Singleton
import QtQuick
import Quickshell
import Qt.labs.folderlistmodel

QtObject {
    id: root

    property string language: "en"
    property var translations: ({})
    property var _partials: ({})
    property url _pluginsDir: ""
    property var _plugins: []
    property var _loaders: []
    property var _loaderComp: null

    function _merge() {
        let merged = {};
        for (const k in _partials)
            Object.assign(merged, _partials[k]);
        translations = merged;
    }

    function _loadTranslations() {
        for (const l of _loaders)
            l.destroy();
        _loaders = [];
        _partials = {};
        translations = {};

        for (const pluginId of _plugins)
            _loadPlugin(pluginId);
    }

    function _loadPlugin(pluginId) {
        if (!_loaderComp || _loaderComp.status !== Component.Ready)
            return;

        const loader = _loaderComp.createObject(root, {
            pluginId: pluginId,
            basePath: Quickshell.shellDir + "/src/plugins/common/" + pluginId + "/i18n/",
            language: root.language
        });
        if (!loader)
            return;

        loader.done.connect(function (id, data) {
            root._partials[id] = data;
            root._merge();
        });

        root._loaders.push(loader);
    }

    property FolderListModel _pluginScanner: FolderListModel {
        showDirs: true
        showFiles: false
        showDotAndDotDot: false

        onStatusChanged: {
            if (status !== FolderListModel.Ready || folder.toString() !== root._pluginsDir.toString())
                return;
            const plugins = [];
            for (let i = 0; i < count; i++)
                plugins.push(get(i, "fileName"));
            root._plugins = plugins;
            _loadTranslations();
        }
    }

    onLanguageChanged: {
        for (const l of _loaders)
            l.destroy();
        _loaders = [];
        _partials = {};
        translations = {};
        for (const pluginId of _plugins)
            _loadPlugin(pluginId);
    }

    Component.onCompleted: {
        _loaderComp = Qt.createComponent(Qt.resolvedUrl("./PluginI18nLoader.qml"));
        if (_loaderComp.status === Component.Error)
            console.error("I18n: failed to load PluginI18nLoader:", _loaderComp.errorString());
        _pluginsDir = Qt.resolvedUrl(Quickshell.shellDir + "/src/plugins/common/");
        _pluginScanner.folder = _pluginsDir;
    }

    function tr(key, params) {
        let value = translations[key] ?? key;
        if (params) {
            for (const k in params)
                value = value.replace("{" + k + "}", params[k]);
        }
        return value;
    }
}
