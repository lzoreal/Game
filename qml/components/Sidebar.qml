import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: sidebar

    objectName: "sidebar"

    signal addRequested()
    signal updateRequested()
    signal importRequested()
    signal refreshRequested()
    signal deleteRequested()
    signal closeRequested()

    width: 232
    color: theme.colors.sidebar

    property string contextSource: ""
    property bool collapsed: false

    function formatCount(value) {
        return String(value).replace(/\B(?=(\d{3})+(?!\d))/g, ",")
    }

    Rectangle {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 1
        color: theme.colors.border
    }

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 14
        anchors.rightMargin: 12
        anchors.topMargin: 16
        anchors.bottomMargin: 12
        spacing: 10

        RowLayout {
            spacing: 8
            Item {
                implicitWidth: headerLabel.implicitWidth
                implicitHeight: Math.max(headerLabel.implicitHeight, 22)

                RowLayout {
                    id: headerLabel
                    anchors.verticalCenter: parent.verticalCenter
                    spacing: 4

                    AppIcon {
                        name: "chevron"
                        size: 10
                        rotation: sidebar.collapsed ? -90 : 0
                        color: theme.colors.muted
                        Behavior on rotation {
                            NumberAnimation { duration: 120 }
                        }
                    }
                    Text {
                        text: "资料库"
                        color: theme.colors.muted
                        font.pixelSize: 11
                        font.weight: 500
                    }
                }

                MouseArea {
                    anchors.fill: parent
                    cursorShape: Qt.PointingHandCursor
                    ToolTip.visible: containsMouse
                    ToolTip.text: sidebar.collapsed ? "展开资料库" : "折叠资料库"
                    onClicked: sidebar.collapsed = !sidebar.collapsed
                }
            }
            Rectangle {
                height: 18
                width: countText.implicitWidth + 12
                radius: 9
                color: theme.colors.soft
                border.width: 1
                border.color: theme.colors.border
                Text {
                    id: countText
                    anchors.centerIn: parent
                    text: Math.max(0, backend.sources.length - 1)
                    color: theme.colors.faint
                    font.pixelSize: 10
                }
            }
            Item { Layout.fillWidth: true }
            AppButton {
                width: 28
                height: 28
                implicitWidth: 28
                implicitHeight: 28
                kind: "ghost"
                iconName: "plus"
                iconSize: 15
                ToolTip.visible: hovered
                ToolTip.text: "添加源"
                onClicked: sidebar.addRequested()
            }
            AppButton {
                objectName: "sidebarClose"
                width: 28
                height: 28
                implicitWidth: 28
                implicitHeight: 28
                kind: "ghost"
                iconName: "close"
                iconSize: 14
                ToolTip.visible: hovered
                ToolTip.text: "关闭资料库 (Ctrl+B)"
                onClicked: sidebar.closeRequested()
            }
        }

        Item {
            visible: sidebar.collapsed
            Layout.fillWidth: true
            Layout.fillHeight: true
        }

        ListView {
            id: sourceList
            objectName: "sourceList"
            visible: !sidebar.collapsed
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 2
            clip: true
            boundsBehavior: Flickable.StopAtBounds
            model: backend.sources
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            delegate: Rectangle {
                id: sourceRow
                width: sourceList.width
                height: 40
                radius: 10
                color: modelData.active
                       ? theme.colors.source_selected
                       : (rowMouse.containsMouse ? theme.colors.source_hover : "transparent")

                RowLayout {
                    anchors.fill: parent
                    anchors.leftMargin: 8
                    anchors.rightMargin: 10
                    spacing: 9

                    Rectangle {
                        width: 24
                        height: 24
                        radius: 7
                        color: modelData.active ? theme.colors.accent : theme.colors.soft
                        border.width: 1
                        border.color: modelData.active ? theme.colors.accent : theme.colors.border
                        AppIcon {
                            anchors.centerIn: parent
                            visible: modelData.name === ""
                            name: "grid"
                            size: 13
                            color: modelData.active ? "#ffffff" : theme.colors.faint
                        }
                        Text {
                            anchors.centerIn: parent
                            visible: modelData.name !== ""
                            text: String(modelData.display).charAt(0).toUpperCase()
                            color: modelData.active ? "#ffffff" : theme.colors.faint
                            font.pixelSize: 10
                            font.weight: 500
                        }
                    }

                    Text {
                        Layout.fillWidth: true
                        text: modelData.display
                        color: modelData.active ? theme.colors.source_selected_text : theme.colors.source_text
                        font.pixelSize: 12
                        font.weight: modelData.active ? 500 : 400
                        elide: Text.ElideRight
                    }

                    Rectangle {
                        visible: modelData.local
                        width: 28
                        height: 17
                        radius: 8
                        color: theme.colors.tag_bg
                        border.width: 1
                        border.color: theme.colors.border
                        Text {
                            anchors.centerIn: parent
                            text: "本地"
                            color: theme.colors.tag_text
                            font.pixelSize: 9
                        }
                    }

                    Text {
                        text: sidebar.formatCount(modelData.count)
                        color: modelData.active ? theme.colors.accent_text : theme.colors.faint
                        font.pixelSize: 10
                    }
                }

                MouseArea {
                    id: rowMouse
                    anchors.fill: parent
                    hoverEnabled: true
                    acceptedButtons: Qt.LeftButton | Qt.RightButton
                    onClicked: {
                        if (mouse.button === Qt.RightButton) {
                            sidebar.contextSource = modelData.name
                            backend.select_source(modelData.name)
                            sourceMenu.popup()
                        } else {
                            backend.select_source(modelData.name)
                        }
                    }
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 1
            color: theme.colors.border
            visible: !sidebar.collapsed
        }

        RowLayout {
            visible: !sidebar.collapsed
            spacing: 6
            AppButton {
                Layout.fillWidth: true
                kind: "primary"
                iconName: "plus"
                iconSize: 14
                text: "添加源"
                onClicked: sidebar.addRequested()
            }
            AppButton {
                width: 34
                height: 34
                implicitWidth: 34
                implicitHeight: 34
                kind: "ghost"
                iconName: "folder"
                iconSize: 17
                ToolTip.visible: hovered
                ToolTip.text: "从本地导入 JSON"
                onClicked: sidebar.importRequested()
            }
            AppButton {
                width: 34
                height: 34
                implicitWidth: 34
                implicitHeight: 34
                kind: "ghost"
                iconName: "refresh"
                iconSize: 17
                ToolTip.visible: hovered
                ToolTip.text: "刷新全部源"
                onClicked: sidebar.refreshRequested()
            }
        }
    }

    Menu {
        id: sourceMenu
        objectName: "sourceMenu"
        width: 190
        padding: 6
        font.pixelSize: 12
        palette.text: theme.colors.text
        palette.windowText: theme.colors.text
        palette.light: theme.colors.menu_selection
        palette.midlight: theme.colors.source_hover
        palette.mid: theme.colors.border
        background: Rectangle {
            color: theme.colors.surface
            border.width: 1
            border.color: theme.colors.border_strong
            radius: 10
        }
        MenuItem {
            text: "更新此源…"
            enabled: sidebar.contextSource.length > 0
            onTriggered: sidebar.updateRequested()
        }
        MenuItem {
            text: "复制源地址"
            enabled: backend.activeSourceUrl.length > 0
            onTriggered: backend.copy_text(backend.activeSourceUrl)
        }
        MenuItem {
            text: "删除源"
            enabled: sidebar.contextSource.length > 0
            onTriggered: sidebar.deleteRequested()
        }
        MenuSeparator {}
        MenuItem {
            text: "打开源文件夹"
            onTriggered: backend.open_sources_folder()
        }
        MenuItem {
            text: "从本地导入 JSON…"
            onTriggered: sidebar.importRequested()
        }
    }
}
