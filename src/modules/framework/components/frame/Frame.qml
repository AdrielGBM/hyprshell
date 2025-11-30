pragma ComponentBehavior: Bound

import Quickshell
import Quickshell.Wayland
import QtQuick

Scope {
    id: frame

    required property var barSizes
    required property int gap
    required property int radius

    required property string color

    Variants {
        model: Quickshell.screens

        PanelWindow {
            required property var modelData

            screen: modelData
            color: "transparent"

            exclusionMode: ExclusionMode.Ignore
            WlrLayershell.layer: WlrLayer.Bottom
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

                onPaint: {
                    var ctx = getContext("2d");
                    ctx.reset();

                    var innerRadius = frame.radius + frame.gap;

                    var innerLeft = frame.barSizes.left;
                    var innerTop = frame.barSizes.top;
                    var innerRight = width - frame.barSizes.right;
                    var innerBottom = height - frame.barSizes.bottom;

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
