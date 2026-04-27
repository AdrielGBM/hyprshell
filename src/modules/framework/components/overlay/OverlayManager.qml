pragma ComponentBehavior: Bound

import Quickshell
import QtQuick

Scope {
    id: overlayManager

    required property var overlayState
    required property var barSizes
    required property bool frameMode
    required property var themeProvider
    required property int overlayWidth
    property int maxVisible: 5
    property var iconProvider: null
    property var i18nProvider: null

    function itemsAt(side, align) {
        return overlayManager.overlayState.positionGroups[side + "-" + align]?.items ?? [];
    }

    function isActive(side, align) {
        const items = overlayManager.overlayState.positionGroups[side + "-" + align]?.items;
        return Array.isArray(items) && items.length > 0;
    }

    Loader {
        active: overlayManager.isActive("top", "start")
        sourceComponent: Overlay {
            side: "top"
            align: "start"
            items: overlayManager.itemsAt("top", "start")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("top", "center")
        sourceComponent: Overlay {
            side: "top"
            align: "center"
            items: overlayManager.itemsAt("top", "center")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("top", "end")
        sourceComponent: Overlay {
            side: "top"
            align: "end"
            items: overlayManager.itemsAt("top", "end")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("bottom", "start")
        sourceComponent: Overlay {
            side: "bottom"
            align: "start"
            items: overlayManager.itemsAt("bottom", "start")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("bottom", "center")
        sourceComponent: Overlay {
            side: "bottom"
            align: "center"
            items: overlayManager.itemsAt("bottom", "center")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("bottom", "end")
        sourceComponent: Overlay {
            side: "bottom"
            align: "end"
            items: overlayManager.itemsAt("bottom", "end")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("left", "center")
        sourceComponent: Overlay {
            side: "left"
            align: "center"
            items: overlayManager.itemsAt("left", "center")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }

    Loader {
        active: overlayManager.isActive("right", "center")
        sourceComponent: Overlay {
            side: "right"
            align: "center"
            items: overlayManager.itemsAt("right", "center")
            barSizes: overlayManager.barSizes
            frameMode: overlayManager.frameMode
            themeProvider: overlayManager.themeProvider
            overlayState: overlayManager.overlayState
            overlayWidth: overlayManager.overlayWidth
            maxVisible: overlayManager.maxVisible
            iconProvider: overlayManager.iconProvider
            i18nProvider: overlayManager.i18nProvider
        }
    }
}
