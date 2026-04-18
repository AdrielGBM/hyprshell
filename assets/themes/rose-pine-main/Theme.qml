import QtQuick

QtObject {
    id: rosePineMainTheme

    // === METADATA ===
    readonly property string name: "Rosé Pine Main"
    readonly property string version: "1.0.0"
    readonly property string author: "Rosé Pine Team"
    readonly property string description: "All natural pine, faux fur and a bit of soho vibes for the classy minimalist."

    // === THEME ===
    readonly property string base: "#191724"
    readonly property string surface: "#1f1d2e"
    readonly property string overlay: "#26233a"
    readonly property string muted: "#6e6a86"
    readonly property string subtle: "#908caa"
    readonly property string text: "#e0def4"

    readonly property string accent1: "#eb6f92"
    readonly property string accent2: "#f6c177"
    readonly property string accent3: "#ebbcba"
    readonly property string accent4: "#31748f"
    readonly property string accent5: "#9ccfd8"
    readonly property string accent6: "#c4a7e7"

    readonly property string success: "#31748f"
    readonly property string warning: "#f6c177"
    readonly property string error: "#eb6f92"
    readonly property string info: "#9ccfd8"

    readonly property string highlightLow: "#21202e"
    readonly property string highlightMed: "#403d52"
    readonly property string highlightHigh: "#524f67"
}
