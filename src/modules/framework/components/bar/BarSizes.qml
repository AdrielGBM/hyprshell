import QtQuick

import "../../"

QtObject {
    required property Settings settings

    property int inactive: settings.inactiveBarSize
    property int active: settings.activeBarSize

    function hasContent(barConfig) {
        if (!barConfig)
            return false;
        const values = Object.values(barConfig);
        for (let i = 0; i < values.length; i++) {
            if (Array.isArray(values[i]) && values[i].length > 0)
                return true;
        }
        return false;
    }

    property int top: hasContent(settings.bars["top"]) ? active : (settings.frameMode ? inactive : 0)
    property int left: hasContent(settings.bars["left"]) ? active : (settings.frameMode ? inactive : 0)
    property int right: hasContent(settings.bars["right"]) ? active : (settings.frameMode ? inactive : 0)
    property int bottom: hasContent(settings.bars["bottom"]) ? active : (settings.frameMode ? inactive : 0)
}
