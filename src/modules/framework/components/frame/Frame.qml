pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick
import qs.src.shared.theme

Scope {
    id: frame

    required property var barSizes
    required property color color

    readonly property int gap: Theme.spacing
    readonly property int radius: Theme.radius

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"

            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Background
            WlrLayershell.keyboardFocus: WlrKeyboardFocus.None

            anchors {
                top: true
                bottom: true
                left: true
                right: true
            }

            Canvas {
                id: frameCanvas
                anchors.fill: parent

                Connections {
                    target: frame
                    function onColorChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onRadiusChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onGapChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onBarSizesChanged() {
                        frameCanvas.requestPaint();
                    }
                }

                Connections {
                    target: frame.barSizes
                    function onTopChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onBottomChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onLeftChanged() {
                        frameCanvas.requestPaint();
                    }
                    function onRightChanged() {
                        frameCanvas.requestPaint();
                    }
                }

                onPaint: {
                    const ctx = getContext("2d");
                    ctx.reset();

                    const innerRadius = frame.radius + frame.gap;

                    const innerLeft = frame.barSizes.left;
                    const innerTop = frame.barSizes.top;
                    const innerRight = width - frame.barSizes.right;
                    const innerBottom = height - frame.barSizes.bottom;

                    ctx.fillStyle = frame.color;

                    ctx.beginPath();
                    ctx.rect(0, 0, width, height);

                    if (innerRadius > 0) {
                        ctx.moveTo(innerLeft + innerRadius, innerTop);
                        ctx.lineTo(innerRight - innerRadius, innerTop);
                        ctx.arcTo(innerRight, innerTop, innerRight, innerTop + innerRadius, innerRadius);
                        ctx.lineTo(innerRight, innerBottom - innerRadius);
                        ctx.arcTo(innerRight, innerBottom, innerRight - innerRadius, innerBottom, innerRadius);
                        ctx.lineTo(innerLeft + innerRadius, innerBottom);
                        ctx.arcTo(innerLeft, innerBottom, innerLeft, innerBottom - innerRadius, innerRadius);
                        ctx.lineTo(innerLeft, innerTop + innerRadius);
                        ctx.arcTo(innerLeft, innerTop, innerLeft + innerRadius, innerTop, innerRadius);
                    } else {
                        ctx.moveTo(innerLeft, innerTop);
                        ctx.lineTo(innerRight, innerTop);
                        ctx.lineTo(innerRight, innerBottom);
                        ctx.lineTo(innerLeft, innerBottom);
                    }
                    ctx.closePath();

                    ctx.fillRule = Qt.OddEvenFill;
                    ctx.fill();
                }
            }
        }
    }
}
