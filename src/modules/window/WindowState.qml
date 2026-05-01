import QtQuick

QtObject {
    id: windowState

    property var openWindows: ({})

    signal windowOpened(string key)
    signal windowClosed(string key)

    function openWindow(key, component, props) {
        const windowExists = openWindows[key] !== undefined;
        const sameComponent = windowExists && openWindows[key].component === component;

        if (windowExists && sameComponent) {
            closeWindow(key);
            return;
        }

        const updated = Object.assign({}, openWindows);
        updated[key] = {
            component: component,
            props: props ?? {}
        };
        openWindows = updated;
        windowOpened(key);
    }

    function closeWindow(key) {
        if (openWindows[key] === undefined)
            return;

        const updated = Object.assign({}, openWindows);
        delete updated[key];
        openWindows = updated;
        windowClosed(key);
    }

    function toggleWindow(key, component, props) {
        if (isOpen(key) && openWindows[key].component === component) {
            closeWindow(key);
        } else {
            openWindow(key, component, props);
        }
    }

    function isOpen(key) {
        return openWindows[key] !== undefined;
    }

    function getComponent(key) {
        return openWindows[key]?.component ?? null;
    }

    function getProps(key) {
        return openWindows[key]?.props ?? {};
    }
}
