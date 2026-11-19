import QtQuick
import QtQuick.Controls as QQC2
Item {
    id: row
    component ToolBarSettings: QtObject {
        property int showNavigationButtons: 0
    }
    component Layers: QtObject {
        property var view
        readonly property int depth: view ? view.depth : 0
        readonly property var currentItem: view ? view.currentItem : null
        function push(page, properties) { return view.push(page, properties || {}); }
        function pop() { return view.pop(); }
    }
    property Item initialPage: null
    property ToolBarSettings globalToolBar: ToolBarSettings {}
    property Layers layers: Layers { view: layerView }
    readonly property int depth: view.depth
    readonly property var currentItem: view.currentItem
    onInitialPageChanged: if (initialPage) view.push(initialPage)
    function push(page, properties) { return view.push(page, properties || {}); }
    function pop() { return view.pop(); }
    function get(index) { return view.get(index); }
    QQC2.StackView {
        id: view
        anchors.fill: parent
        visible: layerView.depth === 0
    }
    QQC2.StackView {
        id: layerView
        anchors.fill: parent
        visible: depth > 0
    }
}
