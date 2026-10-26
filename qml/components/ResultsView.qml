import QtQuick
import QtQuick.Controls
import QtQuick.Layouts

Item {
    id: view

    property int sourceWidth: 120
    property int sizeWidth: 105
    property int dateWidth: 120
    property int linkWidth: 60

    readonly property string resultMeta: backend.scopeText
                                         + (backend.countText.length > 0 ? "  ·  " + backend.countText : "")
                                         + (backend.elapsedText.length > 0 ? "  ·  " + backend.elapsedText : "")

    ColumnLayout {
        anchors.fill: parent
        anchors.leftMargin: 22
        anchors.rightMargin: 24
        anchors.topMargin: 22
        anchors.bottomMargin: 18
        spacing: 14

        RowLayout {
            Layout.fillWidth: true
            spacing: 16

            ColumnLayout {
                spacing: 4
                Text {
                    text: backend.pageTitle
                    color: theme.colors.text
                    font.pixelSize: 24
                    font.weight: 500
                }
                Text {
                    text: backend.pageSub
                    color: theme.colors.muted
                    font.pixelSize: 12
                }
            }

            Item { Layout.fillWidth: true }

            AppButton {
                visible: !Window.window.detailsOpen
                kind: "soft"
                text: "展开详情"
                iconName: "grid"
                iconSize: 14
                onClicked: Window.window.detailsOpen = true
            }
        }

        RowLayout {
            Layout.fillWidth: true
            spacing: 8

            AppButton {
                id: fuzzyButton
                kind: "filter"
                checkable: true
                text: "模糊匹配"
                iconName: "check"
                iconSize: 14
                Component.onCompleted: checked = backend.fuzzy
                onToggled: backend.set_fuzzy(checked)
                Connections {
                    target: backend
                    function onFuzzyChanged() {
                        if (fuzzyButton.checked !== backend.fuzzy)
                            fuzzyButton.checked = backend.fuzzy
                    }
                }
            }

            Text {
                text: "阈值"
                color: theme.colors.muted
                font.pixelSize: 12
            }

            Rectangle {
                height: 32
                implicitWidth: thresholdRow.implicitWidth + 12
                radius: 8
                color: theme.colors.surface
                border.width: 1
                border.color: theme.colors.border

                RowLayout {
                    id: thresholdRow
                    anchors.centerIn: parent
                    spacing: 1

                    AppButton {
                        width: 22
                        height: 24
                        implicitWidth: 22
                        implicitHeight: 24
                        kind: "ghost"
                        text: "−"
                        onClicked: backend.set_threshold(Math.max(0.1, Math.round((backend.threshold - 0.1) * 10) / 10))
                    }
                    Text {
                        width: 24
                        text: backend.threshold.toFixed(1)
                        color: theme.colors.text
                        font.pixelSize: 11
                        horizontalAlignment: Text.AlignHCenter
                    }
                    AppButton {
                        width: 22
                        height: 24
                        implicitWidth: 22
                        implicitHeight: 24
                        kind: "ghost"
                        text: "+"
                        onClicked: backend.set_threshold(Math.min(1.0, Math.round((backend.threshold + 0.1) * 10) / 10))
                    }
                }
            }

            Item { Layout.fillWidth: true }

            AppButton {
                kind: "ghost"
                height: 30
                implicitHeight: 30
                text: backend.sortDesc ? "最新在前 ↓" : "最早在前 ↑"
                ToolTip.visible: hovered
                ToolTip.text: "切换上传日期排序"
                onClicked: backend.toggle_sort()
            }
        }

        Rectangle {
            Layout.fillWidth: true
            Layout.fillHeight: true
            radius: 16
            color: theme.colors.surface
            border.width: 1
            border.color: theme.colors.border
            clip: true

            ColumnLayout {
                anchors.fill: parent
                anchors.margins: 6
                spacing: 0

                Rectangle {
                    Layout.fillWidth: true
                    height: 52
                    color: "transparent"
                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: 1
                        color: theme.colors.border
                    }
                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 18
                        anchors.rightMargin: 18
                        Text {
                            text: "搜索结果"
                            color: theme.colors.text
                            font.pixelSize: 13
                            font.weight: 500
                        }
                        Item { Layout.fillWidth: true }
                        Text {
                            text: view.resultMeta
                            color: theme.colors.faint
                            font.pixelSize: 11
                        }
                    }
                }

                Rectangle {
                    Layout.fillWidth: true
                    height: 36
                    color: theme.colors.soft

                    Rectangle {
                        anchors.left: parent.left
                        anchors.right: parent.right
                        anchors.bottom: parent.bottom
                        height: 1
                        color: theme.colors.border
                    }

                    RowLayout {
                        anchors.fill: parent
                        anchors.leftMargin: 12
                        anchors.rightMargin: 12
                        spacing: 0

                        Text {
                            Layout.fillWidth: true
                            text: "游戏名称"
                            color: theme.colors.header_text
                            font.pixelSize: 10
                            font.weight: 500
                        }
                        Text {
                            Layout.preferredWidth: view.sourceWidth
                            text: "来源"
                            color: theme.colors.header_text
                            font.pixelSize: 10
                            font.weight: 500
                        }
                        Text {
                            Layout.preferredWidth: view.sizeWidth
                            text: "大小"
                            color: theme.colors.header_text
                            font.pixelSize: 10
                            font.weight: 500
                        }
                        Item {
                            Layout.preferredWidth: view.dateWidth
                            height: 20
                            Text {
                                text: "上传日期"
                                color: theme.colors.text
                                font.pixelSize: 10
                                font.weight: 500
                            }
                            AppIcon {
                                anchors.left: parent.left
                                anchors.leftMargin: 48
                                anchors.verticalCenter: parent.verticalCenter
                                name: "chevron"
                                size: 11
                                rotation: backend.sortDesc ? 0 : 180
                                color: theme.colors.text
                            }
                            MouseArea {
                                anchors.fill: parent
                                cursorShape: Qt.PointingHandCursor
                                onClicked: backend.toggle_sort()
                            }
                        }
                        Text {
                            Layout.preferredWidth: view.linkWidth
                            text: "链接"
                            color: theme.colors.header_text
                            font.pixelSize: 10
                            font.weight: 500
                        }
                    }
                }

                Item {
                    Layout.fillWidth: true
                    Layout.fillHeight: true

                ListView {
                    id: resultList
                    objectName: "resultList"
                    anchors.fill: parent
                    clip: true
                    spacing: 0
                    model: backend.resultsModel
                    currentIndex: backend.selectedResultIndex
                    boundsBehavior: Flickable.StopAtBounds
                    ScrollBar.vertical: ScrollBar { policy: ScrollBar.AsNeeded }

                    delegate: Rectangle {
                        id: resultRow
                        objectName: "resultRow"
                        width: resultList.width
                        height: 46
                        color: index === backend.selectedResultIndex
                               ? theme.colors.table_selection
                               : (rowMouse.containsMouse
                                  ? theme.colors.hover
                                  : (index % 2 === 1 ? theme.colors.table_alt : "transparent"))

                        Rectangle {
                            width: 3
                            height: parent.height
                            color: theme.colors.accent
                            visible: index === backend.selectedResultIndex
                        }

                        Rectangle {
                            anchors.left: parent.left
                            anchors.right: parent.right
                            anchors.bottom: parent.bottom
                            height: 1
                            color: theme.colors.border
                        }

                        RowLayout {
                            anchors.fill: parent
                            anchors.leftMargin: 12
                            anchors.rightMargin: 12
                            spacing: 0

                            Text {
                                Layout.fillWidth: true
                                text: model.title
                                color: theme.colors.text
                                font.pixelSize: 13
                                font.weight: 500
                                elide: Text.ElideRight
                            }
                            Item {
                                Layout.preferredWidth: view.sourceWidth
                                height: 22
                                Rectangle {
                                    anchors.verticalCenter: parent.verticalCenter
                                    height: 22
                                    width: Math.min(sourceLabel.implicitWidth + 16, view.sourceWidth - 4)
                                    radius: 11
                                    color: theme.colors.soft
                                    border.width: 1
                                    border.color: theme.colors.border
                                    Text {
                                        id: sourceLabel
                                        anchors.centerIn: parent
                                        width: parent.width - 16
                                        text: model.sources
                                        color: theme.colors.muted
                                        font.pixelSize: 10
                                        elide: Text.ElideRight
                                    }
                                }
                            }
                            Text {
                                Layout.preferredWidth: view.sizeWidth
                                text: model.size
                                color: theme.colors.muted
                                font.pixelSize: 12
                                elide: Text.ElideRight
                            }
                            Text {
                                Layout.preferredWidth: view.dateWidth
                                text: model.date
                                color: theme.colors.muted
                                font.pixelSize: 12
                            }
                            RowLayout {
                                Layout.preferredWidth: view.linkWidth
                                spacing: 4
                                AppIcon {
                                    name: "link"
                                    size: 13
                                    color: theme.colors.muted
                                }
                                Text {
                                    text: model.linkCount
                                    color: theme.colors.muted
                                    font.pixelSize: 11
                                }
                            }
                        }

                        MouseArea {
                            id: rowMouse
                            anchors.fill: parent
                            hoverEnabled: true
                            onClicked: backend.select_result(index)
                            onDoubleClicked: {
                                backend.select_result(index)
                                Window.window.detailsOpen = true
                            }
                        }
                    }
                }

                Item {
                    anchors.fill: parent
                    visible: resultList.count === 0

                    ColumnLayout {
                        anchors.centerIn: parent
                        spacing: 8
                        width: Math.min(parent.width - 40, 360)

                        AppIcon {
                            Layout.alignment: Qt.AlignHCenter
                            name: "search"
                            size: 34
                            color: theme.colors.empty_icon
                        }
                        Text {
                            Layout.alignment: Qt.AlignHCenter
                            text: backend.query.length > 0 ? "没有找到结果" : "搜索游戏"
                            color: theme.colors.empty_title
                            font.pixelSize: 16
                            font.weight: 500
                        }
                        Text {
                            Layout.alignment: Qt.AlignHCenter
                            text: backend.query.length > 0
                                  ? "试试更短的关键词，或切换来源"
                                  : "输入关键词，从 " + (backend.sources.length > 0 ? backend.sources[0].count.toLocaleString() : "0") + " 个游戏中查找"
                            color: theme.colors.muted
                            font.pixelSize: 11
                            horizontalAlignment: Text.AlignHCenter
                            wrapMode: Text.Wrap
                        }
                    }
                }
                }
            }
        }
    }
}
