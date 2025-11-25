pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Io
import QtQuick

QtObject {
    id: dependencyManager

    property var dependencies: ({})

    signal dependencyStatusChanged(string name, bool available)

    function registerDependency(name, type, config) {
        if (dependencies[name]) {
            console.warn("DependencyManager: Dependencia", name, "ya registrada");
            return;
        }

        const depInfo = {
            name: name,
            type: type,
            available: false,
            checking: false,
            config: config || {},
            process: null,
            checkTimer: null,
            retryCount: 0
        };

        dependencies[name] = depInfo;
        dependencies = dependencies;

        console.log("DependencyManager: Registrada dependencia", name, "tipo", type);

        startDependencyCheck(name);
    }

    function startDependencyCheck(name) {
        const dep = dependencies[name];
        if (!dep)
            return;

        if (dep.type === "process") {
            startProcessDependency(name);
        } else if (dep.type === "command") {
            checkCommandDependency(name);
        } else if (dep.type === "service") {
            checkServiceDependency(name);
        }
    }

    function startProcessDependency(name) {
        const dep = dependencies[name];
        if (!dep || !dep.config.command)
            return;

        if (!dep.process) {
            const processComponent = Qt.createQmlObject(`
                import Quickshell.Io
                Process {
                    property string depName: "${name}"
                    running: false
                    command: []
                }
            `, dependencyManager);

            processComponent.command = dep.config.command;

            processComponent.exited.connect((exitCode, exitStatus) => {
                console.warn("DependencyManager: Proceso", name, "terminado con código", exitCode);
                setDependencyStatus(name, false);

                if (!dep.checkTimer) {
                    startCheckTimer(name);
                }
            });

            processComponent.started.connect(() => {
                console.log("DependencyManager: Proceso", name, "iniciado");
            });

            dep.process = processComponent;
            dependencies[name] = dep;
        }

        if (dep.config.autoStart !== false && !dep.process.running) {
            console.log("DependencyManager: Iniciando proceso", name);
            dep.process.running = true;

            const timer = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; running: true; repeat: false }', dependencyManager);
            timer.triggered.connect(() => {
                if (dep.process.running) {
                    setDependencyStatus(name, true);
                } else {
                    startCheckTimer(name);
                }
                timer.destroy();
            });
        } else {
            startCheckTimer(name);
        }
    }

    function checkCommandDependency(name) {
        const dep = dependencies[name];
        if (!dep || !dep.config.checkCommand)
            return;

        if (dep.checking)
            return;
        dep.checking = true;
        dependencies[name] = dep;

        const checkProcess = Qt.createQmlObject(`
            import Quickshell.Io
            Process {
                property string depName: "${name}"
                running: true
                command: []
            }
        `, dependencyManager);

        checkProcess.command = dep.config.checkCommand;

        checkProcess.exited.connect((exitCode, exitStatus) => {
            const available = exitCode === 0;
            setDependencyStatus(name, available);
            dep.checking = false;
            dependencies[name] = dep;
            checkProcess.destroy();

            if (!available) {
                startCheckTimer(name);
            }
        });
    }

    function checkServiceDependency(name) {
        const dep = dependencies[name];
        if (!dep || !dep.config.checkFunction)
            return;

        try {
            const available = dep.config.checkFunction();
            setDependencyStatus(name, available);

            if (!available) {
                startCheckTimer(name);
            }
        } catch (e) {
            console.error("DependencyManager: Error verificando servicio", name, ":", e);
            setDependencyStatus(name, false);
            startCheckTimer(name);
        }
    }

    function startCheckTimer(name) {
        const dep = dependencies[name];
        if (!dep || dep.available)
            return;

        if (dep.checkTimer) {
            dep.checkTimer.destroy();
            dep.checkTimer = null;
        }

        const interval = dep.config.retryInterval || 3000;
        const maxRetries = dep.config.maxRetries || -1;

        const timer = Qt.createQmlObject(`
            import QtQuick
            Timer {
                property string depName: "${name}"
                interval: ${interval}
                running: true
                repeat: true
            }
        `, dependencyManager);

        timer.triggered.connect(() => {
            dep.retryCount++;

            if (maxRetries > 0 && dep.retryCount > maxRetries) {
                console.warn("DependencyManager: Max reintentos alcanzados para", name);
                timer.stop();
                timer.destroy();
                return;
            }

            startDependencyCheck(name);
        });

        dep.checkTimer = timer;
        dependencies[name] = dep;
    }

    function setDependencyStatus(name, available) {
        const dep = dependencies[name];
        if (!dep)
            return;

        const wasAvailable = dep.available;
        dep.available = available;
        dependencies[name] = dep;
        dependencies = dependencies;

        if (available !== wasAvailable) {
            console.log("DependencyManager:", name, "disponible:", available);
            dependencyStatusChanged(name, available);

            if (available) {
                if (dep.checkTimer) {
                    dep.checkTimer.stop();
                    dep.checkTimer.destroy();
                    dep.checkTimer = null;
                }

                if (dep.config.onReady) {
                    Qt.callLater(() => dep.config.onReady());
                }
            }
        }
    }

    function isAvailable(name) {
        const dep = dependencies[name];
        return dep ? dep.available : false;
    }

    function getProcess(name) {
        const dep = dependencies[name];
        return (dep && dep.type === "process") ? dep.process : null;
    }

    function whenAvailable(name, callback) {
        if (isAvailable(name)) {
            Qt.callLater(callback);
        } else {
            const connection = dependencyStatusChanged.connect((depName, available) => {
                if (depName === name && available) {
                    callback();
                    dependencyManager.dependencyStatusChanged.disconnect(connection);
                }
            });
        }
    }

    function unregisterDependency(name) {
        const dep = dependencies[name];
        if (!dep)
            return;

        if (dep.checkTimer) {
            dep.checkTimer.stop();
            dep.checkTimer.destroy();
        }

        if (dep.process) {
            dep.process.running = false;
            dep.process.destroy();
        }

        delete dependencies[name];
        dependencies = dependencies;

        console.log("DependencyManager: Desregistrada dependencia", name);
    }

    function getAllDependencies() {
        return Object.keys(dependencies).map(name => {
            const dep = dependencies[name];
            return {
                name: dep.name,
                type: dep.type,
                available: dep.available,
                retryCount: dep.retryCount
            };
        });
    }

    Component.onDestruction: {
        Object.keys(dependencies).forEach(name => {
            unregisterDependency(name);
        });
    }
}
