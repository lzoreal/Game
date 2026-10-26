import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Rectangle {
    id: detailView

    property bool rawVisible: false

    color: theme.colors.surface
    radius: 16
    border.width: 1
    border.color: theme.colors.border
    clip: true

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        RowLayout {
            Layout.fillWidth: true
            Layout.leftMargin: 18
            Layout.rightMargin: 14
            Layout.topMargin: 15
            spacing: 6

            Text {
                text: "游戏详情"
                color: theme.colors.muted
                font.pixelSize: 11
                font.weight: 500
            }
            Text {
                Layout.fillWidth: true
                text: backend.detailHint
                color: theme.colors.faint
                font.pixelSize: 10
                horizontalAlignment: Text.AlignRight
                elide: Text.ElideRight
            }
            AppButton {
                width: 28
                height: 28
                implicitWidth: 28
                implicitHeight: 28
                kind: "ghost"
                iconName: "close"
                iconSize: 13
                ToolTip.visible: hovered
                ToolTip.text: "收起详情"
                onClicked: Window.window.detailsOpen = false
            }
        }

        Item {
            Layout.fillWidth: true
            Layout.fillHeight: true

        ColumnLayout {
            anchors.centerIn: parent
            spacing: 7
            visible: !backend.hasDetail

            AppIcon {
                Layout.alignment: Qt.AlignHCenter
                name: "grid"
                size: 30
                color: theme.colors.empty_icon
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "选择一个游戏"
                color: theme.colors.muted
                font.pixelSize: 13
            }
            Text {
                Layout.alignment: Qt.AlignHCenter
                text: "点击搜索结果查看来源与下载链接"
                color: theme.colors.faint
                font.pixelSize: 10
            }
        }

        Flickable {
            id: detailFlick
            anchors.fill: parent
            clip: true
            visible: backend.hasDetail
            contentWidth: width
            contentHeight: detailColumn.implicitHeight + 32
            boundsBehavior: Flickable.StopAtBounds
            ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

            ColumnLayout {
                id: detailColumn
                x: 18
                y: 14
                width: detailFlick.width - 32
                spacing: 12

                Text {
                    Layout.fillWidth: true
                    text: backend.detailTitle
                    color: theme.colors.text
                    font.pixelSize: 18
                    font.weight: 500
                    wrapMode: Text.Wrap
                    lineHeight: 1.2
                }

                Flow {
                    Layout.fillWidth: true
                    spacing: 6
                    Repeater {
                        model: backend.detailSources
                        delegate: Rectangle {
                            height: 22
                            width: sourceText.implicitWidth + 16
                            radius: 11
                            color: theme.colors.chip_bg
                            Text {
                                id: sourceText
                                anchors.centerIn: parent
                                text: modelData
                                color: theme.colors.chip_text
                                font.pixelSize: 10
                            }
                        }
                    }
                }

                Text {
                    Layout.fillWidth: true
                    text: backend.detailMeta
                    color: theme.colors.faint
                    font.pixelSize: 11
                }

                RowLayout {
                    spacing: 8
                    AppButton {
                        kind: "soft"
                        text: "复制全部"
                        iconName: "copy"
                        iconSize: 14
                        textSize: 11
                        hPadding: 10
                        cornerRadius: 8
                        height: 28
                        implicitHeight: 28
                        onClicked: backend.copy_current_links()
                    }
                    Item { Layout.fillWidth: true }
                }

                Repeater {
                    model: backend.detailCards
                    delegate: Rectangle {
                        id: card
                        property bool expanded: false
                        property var cardData: modelData

                        Layout.fillWidth: true
                        Layout.preferredHeight: cardColumn.implicitHeight + 24
                        radius: 12
                        color: theme.colors.soft
                        border.width: 1
                        border.color: theme.colors.border

                        ColumnLayout {
                            id: cardColumn
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.top: parent.top
                            anchors.margins: 12
                            spacing: 8

                            RowLayout {
                                Layout.fillWidth: true
                                Text {
                                    text: card.cardData.source
                                    color: theme.colors.text
                                    font.pixelSize: 12
                                    font.weight: 500
                                }
                                Item { Layout.fillWidth: true }
                                Text {
                                    text: card.cardData.date + "  ·  " + card.cardData.size
                                    color: theme.colors.faint
                                    font.pixelSize: 10
                                }
                            }

                            Text {
                                Layout.fillWidth: true
                                visible: card.cardData.description.length > 0
                                text: card.cardData.description
                                color: theme.colors.muted
                                font.pixelSize: 11
                                wrapMode: Text.Wrap
                            }

                            Repeater {
                                model: card.cardData.links
                                delegate: ColumnLayout {
                                    id: linkGroup
                                    property var groupData: modelData

                                    Layout.fillWidth: true
                                    spacing: 6

                                    RowLayout {
                                        Layout.fillWidth: true
                                        Text {
                                            text: linkGroup.groupData.label + " · " + linkGroup.groupData.total
                                            color: theme.colors.muted
                                            font.pixelSize: 10
                                            font.weight: 500
                                        }
                                        Item { Layout.fillWidth: true }
                                        Text {
                                            text: linkGroup.groupData.total
                                            color: theme.colors.faint
                                            font.pixelSize: 10
                                        }
                                    }

                                    Repeater {
                                        model: card.expanded
                                               ? linkGroup.groupData.links
                                               : linkGroup.groupData.links.slice(0, 6)

                                        delegate: ColumnLayout {
                                            id: linkRow
                                            property var linkData: modelData
                                            property string heatText: backend.swarmTexts[linkData.uri] || ""

                                            Layout.fillWidth: true
                                            spacing: 4

                                            RowLayout {
                                                Layout.fillWidth: true
                                                spacing: 5
                                                AppIcon {
                                                    name: linkData.isMagnet ? "magnet" : "link"
                                                    size: 12
                                                    color: theme.colors.muted
                                                }
                                                Text {
                                                    Layout.fillWidth: true
                                                    text: linkData.uri
                                                    color: theme.colors.link
                                                    font.family: "Cascadia Mono"
                                                    font.pixelSize: 10
                                                    elide: Text.ElideMiddle
                                                }
                                                AppButton {
                                                    width: 24
                                                    height: 24
                                                    implicitWidth: 24
                                                    implicitHeight: 24
                                                    kind: "ghost"
                                                    iconName: "copy"
                                                    iconSize: 12
                                                    ToolTip.visible: hovered
                                                    ToolTip.text: "复制链接"
                                                    onClicked: backend.copy_text(linkData.uri)
                                                }
                                                AppButton {
                                                    visible: !linkData.isMagnet
                                                    width: 24
                                                    height: 24
                                                    implicitWidth: 24
                                                    implicitHeight: 24
                                                    kind: "ghost"
                                                    iconName: "external"
                                                    iconSize: 12
                                                    ToolTip.visible: hovered
                                                    ToolTip.text: "在浏览器打开"
                                                    onClicked: Qt.openUrlExternally(linkData.uri)
                                                }
                                            }

                                            Flow {
                                                visible: linkData.isMagnet
                                                Layout.fillWidth: true
                                                spacing: 8

                                                AppButton {
                                                    kind: "primary"
                                                    height: 28
                                                    implicitHeight: 28
                                                    text: "BT 下载"
                                                    iconName: "download"
                                                    iconSize: 14
                                                    textSize: 11
                                                    hPadding: 10
                                                    cornerRadius: 8
                                                    onClicked: {
                                                        btMenu.uri = linkData.uri
                                                        btMenu.popup()
                                                    }
                                                }

                                                Item {
                                                    visible: linkRow.heatText.length > 0
                                                    width: heatPill.width
                                                    height: 28
                                                    Rectangle {
                                                        id: heatPill
                                                        anchors.verticalCenter: parent.verticalCenter
                                                        height: 22
                                                        width: heatContent.implicitWidth + 16
                                                        radius: 11
                                                        color: theme.colors.success_soft
                                                        RowLayout {
                                                            id: heatContent
                                                            anchors.centerIn: parent
                                                            spacing: 5
                                                            Rectangle {
                                                                width: 5
                                                                height: 5
                                                                radius: 2.5
                                                                color: theme.colors.success
                                                                Layout.alignment: Qt.AlignVCenter
                                                            }
                                                            Text {
                                                                text: linkRow.heatText
                                                                color: theme.colors.success
                                                                font.pixelSize: 10
                                                                font.weight: 500
                                                                Layout.alignment: Qt.AlignVCenter
                                                            }
                                                        }
                                                    }
                                                }

                                                AppButton {
                                                    kind: "ghost"
                                                    height: 28
                                                    implicitHeight: 28
                                                    text: "查热度"
                                                    iconName: "pulse"
                                                    iconSize: 14
                                                    textSize: 11
                                                    hPadding: 10
                                                    cornerRadius: 8
                                                    onClicked: backend.query_swarm(linkData.uri)
                                                }
                                            }
                                        }
                                    }

                                    AppButton {
                                        kind: "ghost"
                                        height: 26
                                        implicitHeight: 26
                                        text: "显示其余 " + (linkGroup.groupData.links.length - 6) + " 条"
                                        visible: !card.expanded && linkGroup.groupData.links.length > 6
                                        onClicked: card.expanded = true
                                    }
                                }
                            }
                        }
                    }
                }

                AppButton {
                    kind: "ghost"
                    text: detailView.rawVisible ? "收起原始 JSON" : "查看原始 JSON"
                    iconName: "chevron"
                    iconSize: 14
                    iconRight: true
                    contentSpacing: 4
                    textSize: 11
                    hPadding: 0
                    rotation: 0
                    onClicked: detailView.rawVisible = !detailView.rawVisible
                }

                Rectangle {
                    visible: detailView.rawVisible
                    Layout.fillWidth: true
                    height: 220
                    radius: 10
                    color: theme.colors.soft
                    border.width: 1
                    border.color: theme.colors.border

                    TextArea {
                        anchors.fill: parent
                        anchors.margins: 8
                        readOnly: true
                        text: backend.detailRawJson
                        color: theme.colors.text
                        font.family: "Cascadia Mono"
                        font.pixelSize: 10
                        wrapMode: TextEdit.WrapAnywhere
                        background: Item {}
                    }
                }
            }
        }
        }
    }

    Menu {
        id: btMenu
        property string uri: ""
        width: 200
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
            text: "未检测到 BT 客户端"
            enabled: false
            visible: backend.btTargets.length === 0
        }
        Repeater {
            model: backend.btTargets
            delegate: MenuItem {
                text: modelData.name
                onTriggered: backend.send_to_bt(btMenu.uri, modelData.key)
            }
        }
        MenuSeparator { visible: backend.btTargets.length > 0 }
        MenuItem {
            text: "常见 BT 客户端…"
            onTriggered: Window.window.openBtDialog()
        }
    }
}
