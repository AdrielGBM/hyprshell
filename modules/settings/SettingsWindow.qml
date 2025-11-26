pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

import "../../core/components/panels"
import "components"

Window {
    id: settingsWindow

    property var settings: null
    property var themeProvider: null
    property var dependencyService: null

    windowTitle: "Configuración"

    content: Component {
        Rectangle {
            id: settingsPanel

            property string currentTab: "general"

            color: settingsWindow.themeProvider.base

            RowLayout {
                anchors.fill: parent
                spacing: 0

                // === SIDEBAR ===
                Rectangle {
                    Layout.preferredWidth: 160
                    Layout.fillHeight: true
                    color: settingsWindow.themeProvider.surface

                    Column {
                        anchors.fill: parent
                        anchors.margins: settingsWindow.settings.spacing
                        spacing: settingsWindow.settings.spacing

                        SidebarTab {
                            tabId: "general"
                            label: "General"
                            currentTab: settingsPanel.currentTab
                            themeProvider: settingsWindow.themeProvider
                            settings: settingsWindow.settings
                            onClicked: settingsPanel.currentTab = "general"
                        }

                        SidebarTab {
                            tabId: "appearance"
                            label: "Apariencia"
                            currentTab: settingsPanel.currentTab
                            themeProvider: settingsWindow.themeProvider
                            settings: settingsWindow.settings
                            onClicked: settingsPanel.currentTab = "appearance"
                        }

                        SidebarTab {
                            tabId: "dependencies"
                            label: "Dependencias"
                            currentTab: settingsPanel.currentTab
                            themeProvider: settingsWindow.themeProvider
                            settings: settingsWindow.settings
                            onClicked: settingsPanel.currentTab = "dependencies"
                        }
                    }
                }

                // === CONTENT AREA ===
                ColumnLayout {
                    Layout.fillWidth: true
                    Layout.fillHeight: true
                    spacing: settingsWindow.settings.spacing

                    TabContent {
                        tabId: "general"
                        currentTab: settingsPanel.currentTab

                        Column {
                            anchors.fill: parent
                            anchors.margins: settingsWindow.settings.spacing

                            GeneralTab {
                                width: parent.width
                                themeProvider: settingsWindow.themeProvider
                                settings: settingsWindow.settings
                            }
                        }
                    }

                    TabContent {
                        tabId: "appearance"
                        currentTab: settingsPanel.currentTab

                        Column {
                            anchors.fill: parent
                            anchors.margins: settingsWindow.settings.spacing

                            AppearanceTab {
                                width: parent.width
                                themeProvider: settingsWindow.themeProvider
                                settings: settingsWindow.settings
                            }
                        }
                    }

                    TabContent {
                        tabId: "dependencies"
                        currentTab: settingsPanel.currentTab

                        Column {
                            anchors.fill: parent
                            anchors.margins: settingsWindow.settings.spacing

                            DependenciesTab {
                                width: parent.width
                                themeProvider: settingsWindow.themeProvider
                                settings: settingsWindow.settings
                                dependencyService: settingsWindow.dependencyService
                            }
                        }
                    }

                    Item {
                        Layout.fillHeight: true
                    }
                }
            }
        }
    }
}
