import QtQuick

QtObject {
    property int gap: 16
    property int radius: 8

    property int scale: 1
    property bool frameMode: true

    property int inactiveBarSize: 16
    property int activeBarSize: 40

    property bool topBarActive: true
    property bool leftBarActive: false
    property bool rightBarActive: true
    property bool bottomBarActive: false

    property string color: "#2a273f"

    property int drawerWidth: 200
    property int drawerHeight: 200

    property bool topDrawer1Active: true
    property bool topDrawer2Active: true

    property bool bottomDrawer1Active: false
    property bool bottomDrawer2Active: false

    property bool leftDrawer1Active: false
    property bool leftDrawer2Active: false

    property bool rightDrawer1Active: true
    property bool rightDrawer2Active: true
}
