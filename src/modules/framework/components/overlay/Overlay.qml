pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import "../sideMargins.js" as SideMargins

/// Renders a stacked column of overlay items at a specific side+align position.
/// Created/destroyed by OverlayManager when items appear/disappear at a position.
Scope {
    id: overlay

    /// The screen edge to anchor to: "top" | "bottom" | "left" | "right"
    required property string side
    /// Alignment along the perpendicular axis: "start" | "center" | "end"
    required property string align
    /// The active items at this position: [{ id, component, props }]
    required property var items
    required property var barSizes
    required property bool frameMode
    required property var themeProvider
    required property var overlayState
    /// Maximum width of the overlay window (height is content-driven).
    required property int overlayWidth

    readonly property int gap: themeProvider?.spacing ?? 8
    readonly property bool isHorizontal: side === "top" || side === "bottom"

    readonly property var _m: SideMargins.calc(overlay.side, overlay.align, overlay.barSizes, overlay.frameMode, overlay.gap)

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"
            WlrLayershell.layer: WlrLayer.Overlay
            WlrLayershell.exclusiveZone: -1

            anchors {
                top: overlay.side === "top" || (!overlay.isHorizontal && overlay.align === "start")
                bottom: overlay.side === "bottom" || (!overlay.isHorizontal && overlay.align === "end")
                left: overlay.side === "left" || (overlay.isHorizontal && overlay.align === "start")
                right: overlay.side === "right" || (overlay.isHorizontal && overlay.align === "end")
            }

            margins {
                top: overlay._m.top
                bottom: overlay._m.bottom
                left: overlay._m.left
                right: overlay._m.right
            }

            implicitWidth: overlay.overlayWidth
            implicitHeight: stack.implicitHeight

            Column {
                id: stack
                width: parent.width
                spacing: overlay.gap

                Repeater {
                    model: overlay.items

                    Loader {
                        required property var modelData

                        width: stack.width
                        sourceComponent: modelData.component

                        onLoaded: {
                            item.width = Qt.binding(function () {
                                return stack.width;
                            });

                            const props = modelData.props;
                            for (const key in props)
                                item[key] = props[key];

                            if ("themeProvider" in item)
                                item.themeProvider = Qt.binding(function () {
                                    return overlay.themeProvider;
                                });
                            if ("overlayState" in item)
                                item.overlayState = Qt.binding(function () {
                                    return overlay.overlayState;
                                });
                        }
                    }
                }
            }
        }
    }
}
