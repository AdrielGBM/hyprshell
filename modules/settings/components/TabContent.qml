pragma ComponentBehavior: Bound

import QtQuick
import QtQuick.Layouts

ColumnLayout {
    id: tabContent

    required property string tabId
    required property string currentTab
    default property alias content: contentContainer.data

    Layout.fillWidth: true
    Layout.fillHeight: true
    visible: currentTab === tabId

    Item {
        id: contentContainer
        Layout.fillWidth: true
        Layout.fillHeight: true
    }
}
