import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: bar

    signal updateRequested()
    signal deleteRequested()
    signal aboutRequested()
    signal btRequested()

    height: 62
    color: theme.colors.window

    function focusSearch() {
        searchField.forceActiveFocus()
        searchField.selectAll()
    }

    Rectangle {
        anchors.left: parent.left
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        height: 1
        color: theme.colors.border
    }

    MouseArea {
        anchors.fill: parent
        acceptedButtons: Qt.LeftButton
        onPressed: Window.window.startSystemMove()
        onDoubleClicked: {
            if (Window.window.visibility === Window.Maximized)
                Window.window.showNormal()
            else
                Window.window.showMaximized()
        }
    }

    RowLayout {
        anchors.fill: parent
        anchors.leftMargin: 18
        anchors.rightMargin: 18
        spacing: 18

        RowLayout {
            spacing: 10
            Layout.alignment: Qt.AlignVCenter

            Rectangle {
                width: 34
                height: 34
                radius: 10
                gradient: Gradient {
                    GradientStop { position: 0; color: theme.colors.accent_hover }
                    GradientStop { position: 1; color: theme.colors.accent }
                }
                Text {
                    anchors.centerIn: parent
                    text: "G"
                    color: "#ffffff"
                    font.pixelSize: 16
                    font.weight: 500
                }
            }

            ColumnLayout {
                spacing: 1
                Text {
                    text: "GameMagNet"
                    color: theme.colors.text
                    font.pixelSize: 15
                    font.weight: 500
                }
                Text {
                    text: "游戏目录"
                    color: theme.colors.muted
                    font.pixelSize: 11
                }
            }
        }

        Item { Layout.fillWidth: true }

        Rectangle {
            id: searchBox
            Layout.preferredWidth: 520
            Layout.minimumWidth: 280
            Layout.maximumWidth: 560
            height: 38
            radius: 10
            color: theme.colors.surface
            border.width: 1
            border.color: searchField.activeFocus ? theme.colors.accent : theme.colors.border_strong
            Behavior on border.color { ColorAnimation { duration: 110 } }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 11
                anchors.rightMargin: 8
                spacing: 8

                AppIcon {
                    name: "search"
                    size: 18
                    color: theme.colors.muted
                }

                TextField {
                    id: searchField
                    Layout.fillWidth: true
                    Layout.minimumWidth: 0
                    text: backend.query
                    placeholderText: "搜索游戏名称、来源…"
                    color: theme.colors.text
                    placeholderTextColor: theme.colors.placeholder
                    font.pixelSize: 12
                    leftPadding: 0
                    rightPadding: 0
                    topPadding: 0
                    bottomPadding: 0
                    verticalAlignment: TextInput.AlignVCenter
                    selectByMouse: true
                    background: Item {}
                    onTextEdited: backend.set_query(text)
                }

                Connections {
                    target: backend
                    function onQueryChanged() {
                        if (searchField.text !== backend.query)
                            searchField.text = backend.query
                    }
                }

                AppButton {
                    visible: backend.query.length > 0
                    width: 24
                    height: 24
                    implicitWidth: 24
                    implicitHeight: 24
                    kind: "ghost"
                    iconName: "close"
                    iconSize: 13
                    ToolTip.visible: hovered
                    ToolTip.text: "清空"
                    onClicked: backend.set_query("")
                }

                Rectangle {
                    visible: searchBox.width > 420
                    height: 20
                    width: 40
                    radius: 5
                    color: theme.colors.soft
                    border.width: 1
                    border.color: theme.colors.border
                    Text {
                        anchors.centerIn: parent
                        text: "Ctrl K"
                        color: theme.colors.faint
                        font.pixelSize: 10
                    }
                }
            }
        }

        Item { Layout.fillWidth: true }

        RowLayout {
            spacing: 4
            Layout.alignment: Qt.AlignVCenter

            AppButton {
                width: 34
                height: 34
                implicitWidth: 34
                implicitHeight: 34
                kind: "ghost"
                iconName: theme.dark ? "moon" : "sun"
                iconSize: 18
                ToolTip.visible: hovered
                ToolTip.text: theme.dark ? "切换为浅色主题" : "切换为深色主题"
                onClicked: theme.dark = !theme.dark
            }

            AppButton {
                id: moreButton
                objectName: "moreButton"
                width: 34
                height: 34
                implicitWidth: 34
                implicitHeight: 34
                kind: "ghost"
                iconName: "more"
                iconSize: 18
                ToolTip.visible: hovered
                ToolTip.text: "更多操作"
                onClicked: overflowMenu.popup()
            }

            Rectangle {
                width: 1
                height: 20
                color: theme.colors.border
                Layout.leftMargin: 4
                Layout.rightMargin: 4
            }

            AppButton {
                width: 30
                height: 30
                implicitWidth: 30
                implicitHeight: 30
                kind: "ghost"
                iconName: "minimize"
                iconSize: 15
                ToolTip.visible: hovered
                ToolTip.text: "最小化"
                onClicked: Window.window.showMinimized()
            }

            AppButton {
                width: 30
                height: 30
                implicitWidth: 30
                implicitHeight: 30
                kind: "ghost"
                iconName: Window.window.visibility === Window.Maximized ? "restore" : "maximize"
                iconSize: 14
                ToolTip.visible: hovered
                ToolTip.text: Window.window.visibility === Window.Maximized ? "还原" : "最大化"
                onClicked: {
                    if (Window.window.visibility === Window.Maximized)
                        Window.window.showNormal()
                    else
                        Window.window.showMaximized()
                }
            }

            AppButton {
                width: 30
                height: 30
                implicitWidth: 30
                implicitHeight: 30
                kind: "ghost"
                danger: true
                iconName: "close"
                iconSize: 15
                ToolTip.visible: hovered
                ToolTip.text: "关闭"
                onClicked: Window.window.close()
            }
        }
    }

    Menu {
        id: overflowMenu
        objectName: "overflowMenu"
        width: 236
        padding: 6
        font.pixelSize: 12
        palette.text: theme.colors.text
        palette.windowText: theme.colors.text
        palette.buttonText: theme.colors.text
        // Basic 样式用 palette.light 画选中项背景、palette.mid 画分隔线，
        // 不覆盖的话深色主题下会出现浅底浅字（文字不可见）。
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
            text: "源"
            enabled: false
            font.weight: 500
        }
        MenuItem {
            text: "更新选中源…"
            enabled: backend.activeSource.length > 0
            onTriggered: bar.updateRequested()
        }
        MenuItem {
            text: "删除选中源"
            enabled: backend.activeSource.length > 0
            onTriggered: bar.deleteRequested()
        }
        MenuItem {
            text: "打开源文件夹"
            onTriggered: backend.open_sources_folder()
        }
        MenuSeparator {}
        MenuItem {
            text: "聚焦搜索框"
            onTriggered: bar.focusSearch()
        }
        MenuSeparator {}
        MenuItem {
            text: "常见 BT 客户端…"
            onTriggered: bar.btRequested()
        }
        MenuItem {
            text: "检查更新…"
            onTriggered: backend.check_updates()
        }
        MenuItem {
            text: "关于"
            onTriggered: bar.aboutRequested()
        }
    }
}
