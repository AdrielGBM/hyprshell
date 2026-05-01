.pragma library

function wire(item, entry, context) {
    if ("drawerState" in item)
        item.drawerState = Qt.binding(function () { return context.drawerState })
    if ("windowState" in item)
        item.windowState = Qt.binding(function () { return context.windowState })
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

    const entryObj = (typeof entry === "object" && entry !== null) ? entry : {}
    if ("accent" in item)
        item.accent = entryObj.accent ?? ""
    if ("variant" in item)
        item.variant = entryObj.variant ?? "default"
    if ("mode" in item)
        item.mode = entryObj.mode ?? "panel"
}
