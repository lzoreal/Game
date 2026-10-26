import QtQuick
import QtQuick.Controls
import QtQuick.Layouts
import QtQuick.Dialogs
import "components"

ApplicationWindow {
    id: appWindow

    visible: true
    width: 1440
    height: 900
    minimumWidth: 1180
    minimumHeight: 720
    title: "GameMagNet 游戏目录管理器 " + backend.version
    flags: Qt.Window | Qt.FramelessWindowHint
    color: theme.colors.window
    background: Rectangle { color: theme.colors.window }

    property bool detailsOpen: true

    function openUpdateDialog() {
        if (backend.activeSource.length > 0) {
            updateSourceDialog.open()
        } else {
            showMessage("提示", "请先在左侧选择一个源。", false)
        }
    }

    function openDeleteDialog() {
        if (backend.activeSource.length > 0) {
            deleteDialog.open()
        } else {
            showMessage("提示", "请先选择一个源。", false)
        }
    }

    function openBtDialog() {
        btDialog.open()
    }

    function showMessage(title, text, error) {
        messageDialog.heading = title
        messageDialog.body = text
        messageDialog.isError = error
        messageDialog.link = ""
        messageDialog.open()
    }

    Shortcut { sequence: "Ctrl+U"; onActivated: addDialog.open() }
    Shortcut { sequence: "Ctrl+I"; onActivated: importDialog.open() }
    Shortcut { sequence: "F5"; onActivated: backend.refresh_all() }
    Shortcut { sequence: "Ctrl+D"; onActivated: appWindow.detailsOpen = !appWindow.detailsOpen }
    Shortcut { sequence: "Ctrl+K"; onActivated: topBar.focusSearch() }
    Shortcut { sequence: "Ctrl+F"; onActivated: topBar.focusSearch() }

    ColumnLayout {
        anchors.fill: parent
        spacing: 0

        TopBar {
            id: topBar
            Layout.fillWidth: true
            onUpdateRequested: appWindow.openUpdateDialog()
            onDeleteRequested: appWindow.openDeleteDialog()
            onAboutRequested: aboutDialog.open()
            onBtRequested: btDialog.open()
        }

        RowLayout {
            Layout.fillWidth: true
            Layout.fillHeight: true
            spacing: 0

            Sidebar {
                id: sidebar
                Layout.fillHeight: true
                onAddRequested: addDialog.open()
                onImportRequested: importDialog.open()
                onRefreshRequested: backend.refresh_all()
                onUpdateRequested: appWindow.openUpdateDialog()
                onDeleteRequested: appWindow.openDeleteDialog()
            }

            ResultsView {
                Layout.fillWidth: true
                Layout.fillHeight: true
            }

            Item {
                Layout.preferredWidth: appWindow.detailsOpen ? 388 : 0
                Layout.fillHeight: true
                visible: appWindow.detailsOpen

                DetailView {
                    anchors.fill: parent
                    anchors.margins: 14
                }
            }
        }

        Rectangle {
            Layout.fillWidth: true
            height: 32
            color: theme.colors.topbar

            Rectangle {
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                height: 1
                color: theme.colors.border
            }

            RowLayout {
                anchors.fill: parent
                anchors.leftMargin: 18
                anchors.rightMargin: 18
                spacing: 6

                Rectangle {
                    width: 6
                    height: 6
                    radius: 3
                    color: theme.colors.success
                }
                Text {
                    Layout.fillWidth: true
                    text: backend.statusText
                    color: theme.colors.faint
                    font.pixelSize: 10
                    elide: Text.ElideRight
                }
                Text {
                    text: backend.version + " · 本地目录"
                    color: theme.colors.faint
                    font.pixelSize: 10
                }
            }
        }
    }

    MouseArea {
        anchors.left: parent.left
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 6
        z: 1000
        cursorShape: Qt.SizeHorCursor
        onPressed: appWindow.startSystemResize(Qt.LeftEdge)
    }
    MouseArea {
        anchors.right: parent.right
        anchors.top: parent.top
        anchors.bottom: parent.bottom
        width: 6
        z: 1000
        cursorShape: Qt.SizeHorCursor
        onPressed: appWindow.startSystemResize(Qt.RightEdge)
    }
    MouseArea {
        anchors.top: parent.top
        anchors.left: parent.left
        anchors.right: parent.right
        height: 6
        z: 1000
        cursorShape: Qt.SizeVerCursor
        onPressed: appWindow.startSystemResize(Qt.TopEdge)
    }
    MouseArea {
        anchors.bottom: parent.bottom
        anchors.left: parent.left
        anchors.right: parent.right
        height: 6
        z: 1000
        cursorShape: Qt.SizeVerCursor
        onPressed: appWindow.startSystemResize(Qt.BottomEdge)
    }
    MouseArea {
        anchors.left: parent.left
        anchors.top: parent.top
        width: 12
        height: 12
        z: 1001
        cursorShape: Qt.SizeFDiagCursor
        onPressed: appWindow.startSystemResize(Qt.LeftEdge | Qt.TopEdge)
    }
    MouseArea {
        anchors.right: parent.right
        anchors.top: parent.top
        width: 12
        height: 12
        z: 1001
        cursorShape: Qt.SizeBDiagCursor
        onPressed: appWindow.startSystemResize(Qt.RightEdge | Qt.TopEdge)
    }
    MouseArea {
        anchors.left: parent.left
        anchors.bottom: parent.bottom
        width: 12
        height: 12
        z: 1001
        cursorShape: Qt.SizeBDiagCursor
        onPressed: appWindow.startSystemResize(Qt.LeftEdge | Qt.BottomEdge)
    }
    MouseArea {
        anchors.right: parent.right
        anchors.bottom: parent.bottom
        width: 12
        height: 12
        z: 1001
        cursorShape: Qt.SizeFDiagCursor
        onPressed: appWindow.startSystemResize(Qt.RightEdge | Qt.BottomEdge)
    }

    Rectangle {
        id: toast
        anchors.horizontalCenter: parent.horizontalCenter
        anchors.bottom: parent.bottom
        anchors.bottomMargin: 46
        height: 34
        width: toastLabel.implicitWidth + 28
        radius: 17
        color: theme.colors.text
        opacity: 0
        visible: opacity > 0
        z: 2000

        Text {
            id: toastLabel
            anchors.centerIn: parent
            color: theme.colors.window
            font.pixelSize: 11
        }

        Behavior on opacity { NumberAnimation { duration: 140 } }

        Timer {
            id: toastTimer
            interval: 1800
            onTriggered: toast.opacity = 0
        }
    }

    Connections {
        target: backend
        function onToastRequested(message) {
            toastLabel.text = message
            toast.opacity = 1
            toastTimer.restart()
        }
        function onMessageRequested(title, text, error, link) {
            messageDialog.heading = title
            messageDialog.body = text
            messageDialog.isError = error
            messageDialog.link = link || ""
            messageDialog.open()
        }
        function onOverwriteRequested(name) {
            overwriteDialog.sourceName = name
            overwriteDialog.open()
        }
        function onUpdateDialogRequested() {
            updateAvailableDialog.open()
        }
    }

    Dialog {
        id: addDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 520
        height: 360
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "添加源"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            Text {
                text: "添加新源"
                color: theme.colors.text
                font.pixelSize: 18
                font.weight: 500
            }
            Text {
                text: "源地址"
                color: theme.colors.muted
                font.pixelSize: 11
            }
            AppTextField {
                id: addUrl
                Layout.fillWidth: true
                placeholderText: "https://example.com/catalog.json"
            }
            Text {
                text: "保存名称（可选）"
                color: theme.colors.muted
                font.pixelSize: 11
            }
            AppTextField {
                id: addName
                Layout.fillWidth: true
                placeholderText: "留空则自动生成"
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "soft"
                    text: "取消"
                    onClicked: addDialog.close()
                }
                AppButton {
                    kind: "primary"
                    text: "下载并添加"
                    onClicked: {
                        backend.request_add_source(addUrl.text, addName.text)
                        addDialog.close()
                    }
                }
            }
        }
        onOpened: {
            addUrl.text = ""
            addName.text = ""
            addUrl.forceActiveFocus()
        }
    }

    Dialog {
        id: updateSourceDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 520
        height: 300
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "更新源"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            Text {
                text: "更新源：" + backend.activeSourceDisplay
                color: theme.colors.text
                font.pixelSize: 18
                font.weight: 500
                elide: Text.ElideRight
                Layout.fillWidth: true
            }
            Text {
                text: "源地址"
                color: theme.colors.muted
                font.pixelSize: 11
            }
            AppTextField {
                id: updateUrl
                Layout.fillWidth: true
                placeholderText: "https://example.com/catalog.json"
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "soft"
                    text: "取消"
                    onClicked: updateSourceDialog.close()
                }
                AppButton {
                    kind: "primary"
                    text: "开始更新"
                    onClicked: {
                        backend.request_update_source(updateUrl.text)
                        updateSourceDialog.close()
                    }
                }
            }
        }
        onOpened: {
            updateUrl.text = backend.activeSourceUrl
            updateUrl.forceActiveFocus()
        }
    }

    Dialog {
        id: overwriteDialog
        property string sourceName: ""
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 460
        height: 220
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "覆盖确认"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 14
            Text {
                text: "覆盖确认"
                color: theme.colors.text
                font.pixelSize: 18
                font.weight: 500
            }
            Text {
                Layout.fillWidth: true
                text: "源文件 " + overwriteDialog.sourceName + " 已存在，是否覆盖？"
                color: theme.colors.muted
                font.pixelSize: 12
                wrapMode: Text.Wrap
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "soft"
                    text: "取消"
                    onClicked: {
                        backend.cancel_overwrite()
                        overwriteDialog.close()
                    }
                }
                AppButton {
                    kind: "primary"
                    text: "覆盖"
                    onClicked: {
                        backend.confirm_overwrite()
                        overwriteDialog.close()
                    }
                }
            }
        }
    }

    Dialog {
        id: deleteDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 460
        height: 220
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "确认删除"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 14
            Text {
                text: "确认删除"
                color: theme.colors.text
                font.pixelSize: 18
                font.weight: 500
            }
            Text {
                Layout.fillWidth: true
                text: "确定要删除源「" + backend.activeSourceDisplay + "」吗？本地 JSON 与缓存都会被删除。"
                color: theme.colors.muted
                font.pixelSize: 12
                wrapMode: Text.Wrap
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "soft"
                    text: "取消"
                    onClicked: deleteDialog.close()
                }
                AppButton {
                    kind: "danger"
                    text: "删除"
                    onClicked: {
                        backend.delete_active_source()
                        deleteDialog.close()
                    }
                }
            }
        }
    }

    Dialog {
        id: messageDialog
        objectName: "messageDialog"
        property string heading: ""
        property string body: ""
        property bool isError: false
        property string link: ""
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 520
        height: 240
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: heading
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                spacing: 8
                AppIcon {
                    name: messageDialog.isError ? "close" : "check"
                    size: 18
                    color: messageDialog.isError ? "#e5484d" : theme.colors.success
                }
                Text {
                    text: messageDialog.heading
                    color: theme.colors.text
                    font.pixelSize: 18
                    font.weight: 500
                }
            }
            ScrollView {
                id: messageScroll
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                ScrollBar.vertical.policy: ScrollBar.AsNeeded
                TextArea {
                    width: messageScroll.availableWidth
                    readOnly: true
                    selectByMouse: true
                    text: messageDialog.body
                    color: theme.colors.muted
                    font.pixelSize: 11
                    wrapMode: TextEdit.Wrap
                    background: Item {}
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                AppButton {
                    objectName: "messageOpenLink"
                    visible: messageDialog.link.length > 0
                    text: "用浏览器打开"
                    onClicked: backend.open_external_url(messageDialog.link)
                }
                AppButton {
                    kind: "primary"
                    text: "确定"
                    onClicked: messageDialog.close()
                }
            }
        }
    }

    Dialog {
        id: updateAvailableDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 640
        height: 540
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "发现新版本"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            Text {
                text: "发现新版本 " + (backend.updateInfo.latest || "")
                color: theme.colors.text
                font.pixelSize: 20
                font.weight: 500
            }
            Text {
                text: "当前版本 " + (backend.updateInfo.current || "") + "  →  " + (backend.updateInfo.latest || "")
                color: theme.colors.muted
                font.pixelSize: 11
            }
            Text {
                Layout.fillWidth: true
                text: backend.updateInfo.name || ""
                color: theme.colors.text
                font.pixelSize: 12
                font.weight: 500
                elide: Text.ElideRight
            }
            ScrollView {
                id: updateNotesScroll
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                ScrollBar.vertical.policy: ScrollBar.AsNeeded
                TextArea {
                    width: updateNotesScroll.availableWidth
                    readOnly: true
                    text: backend.updateInfo.notes || "该版本没有提供更新说明。"
                    color: theme.colors.muted
                    font.pixelSize: 11
                    wrapMode: TextEdit.Wrap
                    background: Rectangle {
                        color: theme.colors.soft
                        radius: 10
                        border.width: 1
                        border.color: theme.colors.border
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                spacing: 8
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "ghost"
                    text: "打开发布页"
                    visible: (backend.updateInfo.url || "").length > 0
                    onClicked: Qt.openUrlExternally(backend.updateInfo.url)
                }
                AppButton {
                    kind: "primary"
                    text: "下载更新"
                    visible: (backend.updateInfo.downloadUrl || "").length > 0
                    onClicked: Qt.openUrlExternally(backend.updateInfo.downloadUrl)
                }
                AppButton {
                    kind: "soft"
                    text: "关闭"
                    onClicked: updateAvailableDialog.close()
                }
            }
        }
    }

    Dialog {
        id: aboutDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 520
        height: 380
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "关于"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            RowLayout {
                spacing: 12
                Rectangle {
                    width: 44
                    height: 44
                    radius: 12
                    gradient: Gradient {
                        GradientStop { position: 0; color: theme.colors.accent_hover }
                        GradientStop { position: 1; color: theme.colors.accent }
                    }
                    Text {
                        anchors.centerIn: parent
                        text: "G"
                        color: "#ffffff"
                        font.pixelSize: 20
                        font.weight: 500
                    }
                }
                ColumnLayout {
                    spacing: 2
                    Text {
                        text: "GameMagNet 游戏目录管理器"
                        color: theme.colors.text
                        font.pixelSize: 16
                        font.weight: 500
                    }
                    Text {
                        text: "版本 " + backend.version + " · QML / Qt Quick"
                        color: theme.colors.muted
                        font.pixelSize: 11
                    }
                }
                Item { Layout.fillWidth: true }
            }
            Rectangle {
                Layout.fillWidth: true
                height: 1
                color: theme.colors.border
            }
            Text {
                text: "本地游戏目录管理器，支持去重搜索、来源筛选、详情与链接复制。"
                color: theme.colors.muted
                font.pixelSize: 11
                wrapMode: Text.Wrap
                Layout.fillWidth: true
            }
            Text {
                text: "已加载 " + (backend.sources.length > 0 ? (backend.sources[0].count || 0) : 0) + " 个游戏 · " + Math.max(0, backend.sources.length - 1) + " 个源"
                color: theme.colors.muted
                font.pixelSize: 11
            }
            Text {
                Layout.fillWidth: true
                text: backend.sourcesDir
                color: theme.colors.faint
                font.pixelSize: 10
                wrapMode: Text.Wrap
            }
            Item { Layout.fillHeight: true }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "primary"
                    text: "关闭"
                    onClicked: aboutDialog.close()
                }
            }
        }
    }

    Dialog {
        id: btDialog
        parent: Overlay.overlay
        x: Math.round((parent.width - width) / 2)
        y: Math.round((parent.height - height) / 2)
        width: 620
        height: 560
        modal: true
        header: null
        padding: 24
        closePolicy: Popup.CloseOnEscape
        title: "常见 BT 客户端"
        background: Rectangle {
            color: theme.colors.window
            radius: 16
            border.width: 1
            border.color: theme.colors.border_strong
        }
        contentItem: ColumnLayout {
            spacing: 12
            Text {
                text: "常见 BT 客户端"
                color: theme.colors.text
                font.pixelSize: 18
                font.weight: 500
            }
            Text {
                text: "以下链接均指向官方下载页。安装时请留意捆绑组件。"
                color: theme.colors.muted
                font.pixelSize: 11
            }
            ScrollView {
                id: btScroll
                Layout.fillWidth: true
                Layout.fillHeight: true
                clip: true
                ScrollBar.vertical.policy: ScrollBar.AsNeeded
                ColumnLayout {
                    width: btScroll.availableWidth
                    spacing: 8
                    Repeater {
                        model: backend.btCatalog
                        delegate: Rectangle {
                            Layout.fillWidth: true
                            height: 58
                            radius: 10
                            color: theme.colors.soft
                            border.width: 1
                            border.color: theme.colors.border
                            RowLayout {
                                anchors.fill: parent
                                anchors.leftMargin: 12
                                anchors.rightMargin: 10
                                spacing: 10
                                ColumnLayout {
                                    spacing: 2
                                    Text {
                                        text: modelData.name
                                        color: theme.colors.text
                                        font.pixelSize: 12
                                        font.weight: 500
                                    }
                                    Text {
                                        Layout.fillWidth: true
                                        text: modelData.desc
                                        color: theme.colors.muted
                                        font.pixelSize: 10
                                        elide: Text.ElideRight
                                    }
                                }
                                Item { Layout.fillWidth: true }
                                AppButton {
                                    kind: "soft"
                                    text: "下载页"
                                    height: 28
                                    implicitHeight: 28
                                    onClicked: Qt.openUrlExternally(modelData.url)
                                }
                            }
                        }
                    }
                }
            }
            RowLayout {
                Layout.fillWidth: true
                Item { Layout.fillWidth: true }
                AppButton {
                    kind: "primary"
                    text: "关闭"
                    onClicked: btDialog.close()
                }
            }
        }
    }

    FileDialog {
        id: importDialog
        title: "选择 JSON 目录"
        fileMode: FileDialog.OpenFiles
        nameFilters: ["JSON 文件 (*.json *.json.gz)", "所有文件 (*)"]
        onAccepted: backend.import_sources(selectedFiles)
    }
}
