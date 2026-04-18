import QtQuick

QtObject {
    id: drawerState

    property string activeSide: ""

    property var openSlots: []
    property var pushedSlots: ({})

    property var pushedBars: ({})

    property int slot1BarIndex: -1
    property int slot2BarIndex: -1

    readonly property bool hasVisibleDrawers: {
        if (activeSide !== "" && openSlots.length > 0)
            return true;
        return Object.keys(pushedSlots).length > 0;
    }

    property var contents: ({})
    property var contentProperties: ({})

    property var accents: ({})

    signal drawerOpened(string id)
    signal drawerClosed(string id)

    function isOpen(id) {
        return isOpenOverlay(id) || isPush(id);
    }

    function isPush(id) {
        return pushedSlots[id] === true;
    }

    function open(id) {
        if (isPush(id))
            return;
        openOverlay(id);
        drawerOpened(id);
    }

    function close() {
        const prevSide = activeSide;
        const prevSlots = openSlots.slice();
        activeSide = "";
        openSlots = [];
        slot1BarIndex = -1;
        slot2BarIndex = -1;
        for (let i = 0; i < prevSlots.length; i++) {
            drawerClosed(prevSide + "-" + prevSlots[i]);
        }
    }

    function closeSide(side) {
        const slot1Id = side + "-1";
        const slot2Id = side + "-2";
        if (isPush(slot1Id)) {
            slot1BarIndex = -1;
            disablePush(slot1Id);
        }
        if (isPush(slot2Id)) {
            slot2BarIndex = -1;
            disablePush(slot2Id);
        }
        if (activeSide === side) {
            const prevSlots = openSlots.slice();
            openSlots = [];
            activeSide = "";
            slot1BarIndex = -1;
            slot2BarIndex = -1;
            for (let i = 0; i < prevSlots.length; i++)
                drawerClosed(side + "-" + prevSlots[i]);
        }
    }

    function convertSide(side, toPush) {
        const slot1Id = side + "-1";
        const slot2Id = side + "-2";

        if (toPush) {
            if (activeSide !== side)
                return;
            const slots = openSlots.slice();
            openSlots = [];
            activeSide = "";
            for (let i = 0; i < slots.length; i++)
                _addPushedSlot(side + "-" + slots[i]);
        } else {
            const wasPush1 = isPush(slot1Id);
            const wasPush2 = isPush(slot2Id);
            if (!wasPush1 && !wasPush2)
                return;

            _closePreviousActiveSide(side);

            const newOpen = openSlots.slice();
            if (wasPush1) {
                _removePushedSlot(slot1Id);
                if (newOpen.indexOf(1) === -1)
                    newOpen.push(1);
            }
            if (wasPush2) {
                _removePushedSlot(slot2Id);
                if (newOpen.indexOf(2) === -1)
                    newOpen.push(2);
            }
            newOpen.sort(function (a, b) {
                return a - b;
            });
            activeSide = side;
            openSlots = newOpen;
        }
    }

    function openDrawer(side, barIndex, contentComponent, properties, accent) {
        const slot1Id = side + "-1";
        const slot2Id = side + "-2";

        if (pushedBars[side]) {
            if (isPush(slot1Id) && slot1BarIndex === barIndex) {
                slot1BarIndex = -1;
                disablePush(slot1Id);
                return;
            }
            if (isPush(slot2Id) && slot2BarIndex === barIndex) {
                slot2BarIndex = -1;
                disablePush(slot2Id);
                return;
            }
            if (isPush(slot1Id) && isPush(slot2Id))
                return;
            if (!isPush(slot1Id)) {
                slot1BarIndex = barIndex;
                setContent(slot1Id, contentComponent, properties, accent);
                _addPushedSlot(slot1Id);
                drawerOpened(slot1Id);
                return;
            }
            slot2BarIndex = barIndex;
            setContent(slot2Id, contentComponent, properties, accent);
            _addPushedSlot(slot2Id);
            drawerOpened(slot2Id);
            return;
        }

        if (activeSide === side && isOpenOverlay(slot1Id) && slot1BarIndex === barIndex) {
            if (isOpenOverlay(slot2Id)) {
                openSlots = openSlots.filter(function (s) {
                    return s !== 1;
                });
                slot1BarIndex = -1;
                drawerClosed(slot1Id);
            } else {
                close();
            }
            return;
        }

        if (activeSide === side && isOpenOverlay(slot2Id) && slot2BarIndex === barIndex) {
            closeOverlay(slot2Id);
            drawerClosed(slot2Id);
            slot2BarIndex = -1;
            return;
        }

        if (isPush(slot1Id)) {
            slot2BarIndex = barIndex;
            setContent(slot2Id, contentComponent, properties, accent);
            if (!isOpenOverlay(slot2Id)) {
                _closePreviousActiveSide(side);
                activeSide = side;
                if (openSlots.indexOf(2) === -1) {
                    openSlots = openSlots.concat([2]);
                }
                drawerOpened(slot2Id);
            }
            return;
        }

        if (activeSide !== side || !isOpenOverlay(slot1Id)) {
            slot1BarIndex = barIndex;
            slot2BarIndex = -1;
            setContent(slot1Id, contentComponent, properties, accent);
            open(slot1Id);
            return;
        }

        if (barIndex < slot1BarIndex) {
            setContent(slot2Id, getContent(slot1Id), getContentProperties(slot1Id), getAccent(slot1Id));
            slot2BarIndex = slot1BarIndex;
            slot1BarIndex = barIndex;
            setContent(slot1Id, contentComponent, properties, accent);
            if (!isOpenOverlay(slot2Id)) {
                const ns = openSlots.slice();
                if (ns.indexOf(2) === -1) {
                    ns.push(2);
                    ns.sort(function (a, b) {
                        return a - b;
                    });
                    openSlots = ns;
                    drawerOpened(slot2Id);
                }
            }
        } else {
            slot2BarIndex = barIndex;
            setContent(slot2Id, contentComponent, properties, accent);
            if (!isOpenOverlay(slot2Id)) {
                open(slot2Id);
            }
        }
    }

    function toggle(id) {
        if (isPush(id)) {
            disablePush(id);
        } else if (isOpenOverlay(id)) {
            closeOverlay(id);
            drawerClosed(id);
        } else {
            open(id);
        }
    }

    function enablePush(id) {
        if (!isOpen(id) || isPush(id))
            return;
        const parts = parseId(id);
        if (activeSide === parts.side) {
            openSlots = openSlots.filter(function (s) {
                return s !== parts.slot;
            });
            if (openSlots.length === 0)
                activeSide = "";
        }
        _addPushedSlot(id);
        drawerOpened(id);
    }

    function disablePush(id) {
        if (!isPush(id))
            return;
        _removePushedSlot(id);
        drawerClosed(id);
    }

    function togglePush(id) {
        if (isPush(id))
            disablePush(id);
        else
            enablePush(id);
    }

    function setContent(id, component, properties, accent) {
        const updated = Object.assign({}, contents);
        updated[id] = component;
        contents = updated;

        if (properties !== undefined) {
            const propsUpdated = Object.assign({}, contentProperties);
            propsUpdated[id] = properties;
            contentProperties = propsUpdated;
        }

        if (accent !== undefined) {
            const accUpdated = Object.assign({}, accents);
            accUpdated[id] = accent;
            accents = accUpdated;
        }
    }

    function getContent(id) {
        return contents[id] ?? null;
    }

    function getContentProperties(id) {
        return contentProperties[id] ?? {};
    }

    function getAccent(id) {
        return accents[id] ?? "";
    }

    function getOpenCount(side) {
        let count = 0;
        if (isOpen(side + "-1"))
            count++;
        if (isOpen(side + "-2"))
            count++;
        return count;
    }

    function parseId(id) {
        const dash = id.lastIndexOf("-");
        return {
            side: id.substring(0, dash),
            slot: parseInt(id.substring(dash + 1))
        };
    }

    function isOpenOverlay(id) {
        const parts = parseId(id);
        return activeSide === parts.side && openSlots.indexOf(parts.slot) !== -1;
    }

    function openOverlay(id) {
        const parts = parseId(id);
        const side = parts.side;
        const slot = parts.slot;

        _closePreviousActiveSide(side);
        activeSide = side;

        if (slot === 2 && openSlots.indexOf(1) === -1 && !isPush(side + "-1"))
            return;

        if (openSlots.indexOf(slot) === -1) {
            const newSlots = openSlots.slice();
            newSlots.push(slot);
            newSlots.sort(function (a, b) {
                return a - b;
            });
            openSlots = newSlots;
        }
    }

    function closeOverlay(id) {
        const parts = parseId(id);
        if (activeSide !== parts.side)
            return;
        const slot = parts.slot;
        if (slot === 1) {
            if (openSlots.indexOf(2) !== -1) {
                openSlots = openSlots.filter(function (s) {
                    return s !== 1;
                });
                slot1BarIndex = -1;
            } else {
                openSlots = [];
                activeSide = "";
                slot1BarIndex = -1;
            }
        } else {
            openSlots = openSlots.filter(function (s) {
                return s !== slot;
            });
            slot2BarIndex = -1;
        }
        if (openSlots.length === 0)
            activeSide = "";
    }

    function _addPushedSlot(id) {
        const updated = Object.assign({}, pushedSlots);
        updated[id] = true;
        pushedSlots = updated;
    }

    function _removePushedSlot(id) {
        const updated = Object.assign({}, pushedSlots);
        delete updated[id];
        pushedSlots = updated;
    }

    function _closePreviousActiveSide(side) {
        if (activeSide !== "" && activeSide !== side) {
            const prevSide = activeSide;
            const prevSlots = openSlots.slice();
            openSlots = [];
            activeSide = "";
            for (let i = 0; i < prevSlots.length; i++)
                drawerClosed(prevSide + "-" + prevSlots[i]);
        }
    }
}
