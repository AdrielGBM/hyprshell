import QtQuick

QtObject {
    id: registry

    property var modules: ({})

    function register(id, component) {
        const updated = Object.assign({}, modules);
        updated[id] = component;
        modules = updated;
    }

    function get(id) {
        return modules[id] ?? null;
    }

    function has(id) {
        return modules.hasOwnProperty(id);
    }

    function ids() {
        return Object.keys(modules);
    }
}
