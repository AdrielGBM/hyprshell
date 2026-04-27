import QtQuick
import Quickshell
import Quickshell.Io

QtObject {
    id: i18nProvider

    property string language: "en"
    property var translations: ({})

    property FileView fileView: FileView {
        watchChanges: false

        onLoaded: {
            try {
                const content = text();
                if (!content || content.trim().length === 0)
                    return;
                i18nProvider.translations = JSON.parse(content);
            } catch (e) {
                console.error("I18nProvider: parse error:", e.message);
            }
        }

        onLoadFailed: err => {
            console.error("I18nProvider: failed to load language file:", FileViewError.toString(err));
        }
    }

    onLanguageChanged: {
        fileView.path = Quickshell.shellDir + "/i18n/" + language + ".json";
        fileView.reload();
    }

    function tr(key, params) {
        let value = translations[key] ?? key;
        if (params) {
            for (const k in params)
                value = value.replace("{" + k + "}", params[k]);
        }
        return value;
    }

    Component.onCompleted: {
        fileView.path = Quickshell.shellDir + "/i18n/" + language + ".json";
        fileView.reload();
    }
}
