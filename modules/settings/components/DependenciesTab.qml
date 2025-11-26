pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts
import Quickshell.Io

Column {
    id: dependenciesTab

    required property var themeProvider
    required property var settings
    required property var dependencyService

    spacing: settings.spacing

    Text {
        text: "Dependencias del Sistema"
        color: dependenciesTab.themeProvider.accent1
        font.pixelSize: dependenciesTab.settings.mediumFontSize
        font.bold: true
    }

    Text {
        text: "Estado de las dependencias requeridas por los módulos"
        color: dependenciesTab.themeProvider.subtle
        font.pixelSize: dependenciesTab.settings.smallFontSize
        wrapMode: Text.WordWrap
        width: parent.width
    }

    Rectangle {
        id: installAllButton
        width: parent.width
        height: 40
        color: dependenciesTab.themeProvider.overlay
        border.color: installAllButton.missingCount > 0 ? dependenciesTab.themeProvider.accent2 : dependenciesTab.themeProvider.muted
        border.width: 1
        radius: dependenciesTab.settings.radius

        property int missingCount: {
            const deps = dependenciesTab.dependencyService.getAllDependencies();
            return deps.filter(d => !d.available).length;
        }

        Row {
            anchors.centerIn: parent
            spacing: 10

            Text {
                text: installAllButton.missingCount > 0 ? "⚠" : "✓"
                color: installAllButton.missingCount > 0 ? dependenciesTab.themeProvider.accent2 : dependenciesTab.themeProvider.accent3
                font.pixelSize: 16
                anchors.verticalCenter: parent.verticalCenter
            }

            Text {
                text: installAllButton.missingCount > 0 ? `${installAllButton.missingCount} dependencia${installAllButton.missingCount > 1 ? 's' : ''} faltante${installAllButton.missingCount > 1 ? 's' : ''}` : "Todas las dependencias instaladas"
                color: dependenciesTab.themeProvider.text
                font.pixelSize: 13
                anchors.verticalCenter: parent.verticalCenter
            }
        }

        Rectangle {
            id: installAllBtn
            anchors.right: parent.right
            anchors.rightMargin: 10
            anchors.verticalCenter: parent.verticalCenter
            width: 120
            height: 28
            color: installAllButton.missingCount > 0 ? dependenciesTab.themeProvider.accent2 : dependenciesTab.themeProvider.muted
            radius: 4
            opacity: installAllButton.missingCount > 0 ? (installAllMouseArea.containsMouse ? 0.8 : 1.0) : 0.5
            visible: installAllButton.missingCount > 0

            Text {
                text: "Instalar Todas"
                color: dependenciesTab.themeProvider.base
                font.pixelSize: 12
                font.bold: true
                anchors.centerIn: parent
            }

            MouseArea {
                id: installAllMouseArea
                anchors.fill: parent
                hoverEnabled: true
                cursorShape: Qt.PointingHandCursor
                enabled: installAllButton.missingCount > 0

                onClicked: {
                    const deps = dependenciesTab.dependencyService.getAllDependencies();
                    const missing = deps.filter(d => !d.available);
                    const commands = missing.map(d => dependenciesTab.dependencyService.getInstallCommand(d.name)).filter(cmd => cmd && cmd.length > 0);

                    if (commands.length > 0) {
                        const fullCommand = commands.join(" && ");
                        dependenciesTab.openTerminalWithCommand(fullCommand);
                    }
                }
            }
        }
    }

    Column {
        width: parent.width
        spacing: dependenciesTab.settings.spacing / 2

        Repeater {
            model: {
                const deps = dependenciesTab.dependencyService.getAllDependencies();
                return deps.sort((a, b) => {
                    if (a.available === b.available)
                        return a.name.localeCompare(b.name);
                    return a.available ? 1 : -1;
                });
            }

            delegate: Rectangle {
                id: delegateRoot
                required property var modelData

                readonly property var dep: modelData

                width: parent.width
                height: 70
                color: dependenciesTab.themeProvider.overlay
                border.color: dep.available ? dependenciesTab.themeProvider.accent3 : dependenciesTab.themeProvider.accent2
                border.width: 1
                radius: dependenciesTab.settings.radius

                RowLayout {
                    anchors.fill: parent
                    anchors.margins: 10
                    spacing: 10

                    Rectangle {
                        Layout.preferredWidth: 40
                        Layout.preferredHeight: 40
                        Layout.alignment: Qt.AlignVCenter
                        color: delegateRoot.dep.available ? dependenciesTab.themeProvider.accent3 : dependenciesTab.themeProvider.accent2
                        radius: 20

                        Text {
                            text: delegateRoot.dep.available ? "✓" : "✗"
                            color: dependenciesTab.themeProvider.base
                            font.pixelSize: 18
                            font.bold: true
                            anchors.centerIn: parent
                        }
                    }

                    Column {
                        Layout.fillWidth: true
                        spacing: 4

                        Text {
                            text: delegateRoot.dep.name
                            color: dependenciesTab.themeProvider.text
                            font.pixelSize: 13
                            font.bold: true
                        }

                        Text {
                            text: {
                                const typeLabels = {
                                    "process": "Proceso/Daemon",
                                    "command": "Comando CLI",
                                    "service": "Servicio"
                                };
                                return typeLabels[delegateRoot.dep.type] || delegateRoot.dep.type;
                            }
                            color: dependenciesTab.themeProvider.subtle
                            font.pixelSize: 11
                        }

                        Text {
                            text: delegateRoot.dep.available ? "Disponible y funcionando" : `No disponible (${delegateRoot.dep.retryCount} reintentos)`
                            color: delegateRoot.dep.available ? dependenciesTab.themeProvider.accent3 : dependenciesTab.themeProvider.accent2
                            font.pixelSize: 11
                        }

                        Text {
                            text: {
                                const modules = delegateRoot.dep.usedBy || [];
                                if (modules.length === 0)
                                    return "";
                                return "Usado por: " + modules.join(", ");
                            }
                            color: dependenciesTab.themeProvider.subtle
                            font.pixelSize: 10
                            font.italic: true
                            visible: text.length > 0
                        }
                    }

                    Rectangle {
                        Layout.preferredWidth: 100
                        Layout.preferredHeight: 32
                        Layout.alignment: Qt.AlignVCenter
                        color: installMouseArea.containsMouse ? dependenciesTab.themeProvider.accent1 : dependenciesTab.themeProvider.surface
                        border.color: dependenciesTab.themeProvider.accent1
                        border.width: 1
                        radius: 4
                        visible: !delegateRoot.dep.available

                        Text {
                            text: "Instalar"
                            color: installMouseArea.containsMouse ? dependenciesTab.themeProvider.base : dependenciesTab.themeProvider.accent1
                            font.pixelSize: 12
                            anchors.centerIn: parent
                        }

                        MouseArea {
                            id: installMouseArea
                            anchors.fill: parent
                            hoverEnabled: true
                            cursorShape: Qt.PointingHandCursor

                            onClicked: {
                                const cmd = dependenciesTab.dependencyService.getInstallCommand(delegateRoot.dep.name);
                                if (cmd && cmd.length > 0) {
                                    dependenciesTab.openTerminalWithCommand(cmd);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    Timer {
        interval: 2000
        running: true
        repeat: true
        onTriggered: {
            dependenciesTab.dependencyService.dependencies = dependenciesTab.dependencyService.dependencies;
        }
    }

    function openTerminalWithCommand(command) {
        console.log("DependenciesTab: Abriendo terminal con comando:", command);

        const terminals = [["kitty", "-e", "sh", "-c"], ["alacritty", "-e", "sh", "-c"], ["wezterm", "start", "sh", "-c"], ["foot", "sh", "-c"], ["gnome-terminal", "--", "sh", "-c"], ["konsole", "-e", "sh", "-c"], ["xterm", "-e", "sh", "-c"]];

        const fullCommand = `echo "Comando a ejecutar:" && echo "${command}" && echo "" && read -p "Presiona Enter para ejecutar o Ctrl+C para cancelar..." && ${command} && echo "" && read -p "Presiona Enter para cerrar..."`;

        for (let i = 0; i < terminals.length; i++) {
            const term = terminals[i];
            const process = Qt.createQmlObject(`
                import Quickshell.Io
                Process {
                    running: true
                    command: []
                }
            `, dependenciesTab);

            process.command = term.concat([fullCommand]);

            const timer = Qt.createQmlObject('import QtQuick; Timer { interval: 1000; running: true; repeat: false }', dependenciesTab);
            timer.triggered.connect(() => {
                process.destroy();
                timer.destroy();
            });

            break;
        }
    }

    Component.onCompleted: {
        console.log("DependenciesTab: Inicializado");
    }
}
