import QtQuick

QtObject {
    id: rosePineDawnTheme

    // === METADATA ===
    readonly property string name: "Rosé Pine Dawn"
    readonly property string version: "1.0.0"
    readonly property string author: "Rosé Pine Team"
    readonly property string description: "All natural pine, faux fur and a bit of soho vibes for the classy minimalist."

    // === ROSÉ PINE DAWN ===
    readonly property var originalTheme: ({
            base: "#faf4ed",
            surface: "#fffaf3",
            overlay: "#f2e9e1",
            muted: "#9893a5",
            subtle: "#797593",
            text: "#575279",
            love: "#b4637a",
            gold: "#ea9d34",
            rose: "#d7827e",
            pine: "#286983",
            foam: "#56949f",
            iris: "#907aa9",
            highlightLow: "#f4ede8",
            highlightMed: "#dfdad9",
            highlightHigh: "#cecacd"
        })

    // === THEME ===
    readonly property string base: originalTheme.base
    readonly property string surface: originalTheme.surface
    readonly property string overlay: originalTheme.overlay
    readonly property string muted: originalTheme.muted
    readonly property string subtle: originalTheme.subtle
    readonly property string text: originalTheme.text

    readonly property string accent1: originalTheme.love
    readonly property string accent2: originalTheme.gold
    readonly property string accent3: originalTheme.rose
    readonly property string accent4: originalTheme.pine
    readonly property string accent5: originalTheme.foam
    readonly property string accent6: originalTheme.iris

    readonly property string success: originalTheme.pine
    readonly property string warning: originalTheme.gold
    readonly property string error: originalTheme.love
    readonly property string info: originalTheme.foam

    readonly property string highlightLow: originalTheme.highlightLow
    readonly property string highlightMed: originalTheme.highlightMed
    readonly property string highlightHigh: originalTheme.highlightHigh
}
