pragma Singleton
import QtQuick
import Quickshell.Services.Notifications

QtObject {
    id: root

    readonly property NotificationServer server: NotificationServer {
        keepOnReload: true
        bodySupported: true
        bodyMarkupSupported: true
        actionsSupported: true
        imageSupported: true
    }
}
