.pragma library

function calc(side, align, barSizes, frameMode, gap) {
    const isHorizontal = side === "top" || side === "bottom"
    const barMult = frameMode ? 1 : 2
    const free = frameMode ? 0 : gap

    function edgeMargin(edge, barSize) {
        const isSide = side === edge
        const isAlignEdge = isHorizontal
            ? ((edge === "left" && align === "start") || (edge === "right" && align === "end"))
            : ((edge === "top"  && align === "start") || (edge === "bottom" && align === "end"))
        return (isSide || isAlignEdge) ? barSize + gap * barMult : free
    }

    return {
        top:    edgeMargin("top",    barSizes.top),
        bottom: edgeMargin("bottom", barSizes.bottom),
        left:   edgeMargin("left",   barSizes.left),
        right:  edgeMargin("right",  barSizes.right)
    }
}
