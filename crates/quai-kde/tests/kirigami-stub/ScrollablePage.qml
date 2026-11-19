import QtQuick
// Children (a ListView or a layout) are stretched over the page, as Kirigami does.
Page {
    id: page
    default property alias content: holder.data
    Item {
        id: holder
        anchors.fill: parent
        onChildrenChanged: {
            for (let i = 0; i < children.length; i++) {
                const c = children[i];
                c.anchors.left = holder.left;
                c.anchors.right = holder.right;
                c.anchors.top = holder.top;
                if (c instanceof ListView) {
                    c.anchors.bottom = holder.bottom;
                }
            }
        }
    }
}
