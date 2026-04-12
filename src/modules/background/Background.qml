pragma ComponentBehavior: Bound

import QtQuick
import Quickshell
import Quickshell.Wayland

Scope {
    id: background

    property var config: ({})

    Settings {
        id: settings
        config: background.config
    }

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"
            anchors {
                left: true
                right: true
                top: true
                bottom: true
            }

            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            Rectangle {
                anchors.fill: parent
                color: settings.backgroundColor

                Image {
                    anchors.fill: parent
                    visible: settings.wallpaperPath !== ""
                    source: {
                        const path = settings.wallpaperPath;
                        if (!path)
                            return "";
                        return path.startsWith("file://") ? path : "file://" + path;
                    }
                    fillMode: Image.PreserveAspectCrop
                    smooth: true
                    asynchronous: true
                }
            }
        }
    }
}
