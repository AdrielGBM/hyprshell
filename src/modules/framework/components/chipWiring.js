.pragma library

function wire(item, entry, context) {
    item.themeProvider = Qt.binding(function () { return context.themeProvider })

    if ("iconProvider" in item)
        item.iconProvider = Qt.binding(function () { return context.iconProvider })
    if ("i18nProvider" in item)
        item.i18nProvider = Qt.binding(function () { return context.i18nProvider })
    if ("drawerState" in item)
        item.drawerState = Qt.binding(function () { return context.drawerState })
    if ("overlayState" in item)
        item.overlayState = Qt.binding(function () { return context.overlayState })
    if ("moduleRegistry" in item)
        item.moduleRegistry = Qt.binding(function () { return context.moduleRegistry })
    if ("barPosition" in item)
        item.barPosition = Qt.binding(function () { return context.barPosition })
    if ("barIndex" in item)
        item.barIndex = context.barIndex
    if ("barScreen" in item)
        item.barScreen = context.barScreen
    if ("chipRadius" in item)
        item.chipRadius = Qt.binding(function () { return context.chipRadius })

    if (typeof entry === "object" && entry !== null) {
        if ("accent" in item && entry.accent)
            item.accent = entry.accent
        if ("variant" in item && entry.variant)
            item.variant = entry.variant
    }
}
