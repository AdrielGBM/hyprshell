import QtQuick

QtObject {
    id: drawerState

    property string drawerOrientation: "vertical"

    readonly property var drawerSides: drawerOrientation === "vertical" ? ["left", "right"] : ["top", "bottom"]

    property var openDrawers: ({})

    property string activePanelSide: ""
    property int activePanelBarIndex: -1
    property string activePanelPosition: "start"

    property var contents: ({})
    property var contentProperties: ({})
    property var accents: ({})

    readonly property bool hasVisibleDrawers: Object.keys(openDrawers).length > 0 || activePanelSide !== ""

    signal drawerOpened(string side)
    signal drawerClosed(string side)

    function isDrawerSide(side) {
        return drawerSides.indexOf(side) !== -1;
    }

    function isOpen(side) {
        if (isDrawerSide(side))
            return openDrawers[side] !== undefined;
        return activePanelSide === side;
    }

    function openDrawer(side, barIndex, component, props, accent) {
        if (isDrawerSide(side))
            _openPushedDrawer(side, barIndex, component, props, accent);
        else
            _openPanel(side, barIndex, component, props, accent);
    }

    function closeSide(side) {
        if (isDrawerSide(side))
            _closePushedDrawer(side);
        else if (activePanelSide === side)
            _closePanel();
    }

    function closePanel() {
        _closePanel();
    }

    function _openPushedDrawer(side, barIndex, component, props, accent) {
        if (openDrawers[side] !== undefined && openDrawers[side] === barIndex) {
            _closePushedDrawer(side);
            return;
        }
        const wasOpen = openDrawers[side] !== undefined;
        setContent(side, component, props, accent);
        const updated = Object.assign({}, openDrawers);
        updated[side] = barIndex;
        openDrawers = updated;
        if (!wasOpen)
            drawerOpened(side);
    }

    function _closePushedDrawer(side) {
        if (openDrawers[side] === undefined)
            return;
        const updated = Object.assign({}, openDrawers);
        delete updated[side];
        openDrawers = updated;
        drawerClosed(side);
    }

    function _openPanel(side, barIndex, component, props, accent) {
        if (activePanelSide === side && activePanelBarIndex === barIndex) {
            _closePanel();
            return;
        }
        const prevSide = activePanelSide;
        setContent(side, component, props, accent);
        activePanelBarIndex = barIndex;
        activePanelPosition = barIndex < 100 ? "start" : (barIndex < 200 ? "center" : "end");
        activePanelSide = side;
        if (prevSide !== "" && prevSide !== side)
            drawerClosed(prevSide);
        drawerOpened(side);
    }

    function _closePanel() {
        if (activePanelSide === "")
            return;
        const prev = activePanelSide;
        activePanelSide = "";
        activePanelBarIndex = -1;
        activePanelPosition = "start";
        drawerClosed(prev);
    }

    function setContent(id, component, properties, accent) {
        const updated = Object.assign({}, contents);
        updated[id] = component;
        contents = updated;

        if (properties !== undefined) {
            const propsUpdated = Object.assign({}, contentProperties);
            propsUpdated[id] = properties;
            contentProperties = propsUpdated;
        }

        if (accent !== undefined) {
            const accUpdated = Object.assign({}, accents);
            accUpdated[id] = accent;
            accents = accUpdated;
        }
    }

    function getContent(id) {
        return contents[id] ?? null;
    }

    function getContentProperties(id) {
        return contentProperties[id] ?? {};
    }

    function getAccent(id) {
        return accents[id] ?? "";
    }
}
