import QtQuick

QtObject {
    id: rosePineMainTheme

    // === METADATA ===
    readonly property string name: "Rosé Pine Main"
    readonly property string version: "1.0.0"
    readonly property string author: "Rosé Pine Team"
    readonly property string description: "All natural pine, faux fur and a bit of soho vibes for the classy minimalist."

    // === ROSÉ PINE MAIN ===
    readonly property var originalTheme: ({
            base: "#191724",
            surface: "#1f1d2e",
            overlay: "#26233a",
            muted: "#6e6a86",
            subtle: "#908caa",
            text: "#e0def4",
            love: "#eb6f92",
            gold: "#f6c177",
            rose: "#ebbcba",
            pine: "#31748f",
            foam: "#9ccfd8",
            iris: "#c4a7e7",
            highlightLow: "#21202e",
            highlightMed: "#403d52",
            highlightHigh: "#524f67"
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
