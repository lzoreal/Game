"""Source, about and BT-client dialogs."""

import os

from PySide6.QtCore import Qt, QUrl, qVersion
from PySide6.QtGui import QDesktopServices
from PySide6.QtWidgets import (
    QApplication,
    QComboBox,
    QDialog,
    QDialogButtonBox,
    QFormLayout,
    QFrame,
    QHBoxLayout,
    QLabel,
    QLineEdit,
    QPushButton,
    QScrollArea,
    QVBoxLayout,
    QWidget,
)

import hydra_bt as bt
import hydra_core as core
from hydra_ui_theme import (
    APP_NAME,
    apply_dark_titlebar,
    show_error,
)


class SourceDialog(QDialog):
    def __init__(self, parent, mode, url="", method="cloudscraper", source_label=""):
        super().__init__(parent)
        self.mode = mode
        self._url = ""
        self._method = method
        self.setWindowTitle("添加源" if mode == "add" else "更新源")
        self.setModal(True)
        self.setWindowIcon(QApplication.windowIcon())
        self.resize(700, 340)

        layout = QVBoxLayout(self)
        layout.setContentsMargins(22, 20, 22, 18)
        layout.setSpacing(14)

        title = QLabel("添加新源" if mode == "add" else f"更新源：{source_label}")
        title.setObjectName("dialogTitle")
        title.setWordWrap(True)
        layout.addWidget(title)

        form = QFormLayout()
        form.setSpacing(10)

        self.url_edit = QLineEdit(url)
        self.url_edit.setPlaceholderText("https://example.com/catalog.json")
        form.addRow("源地址", self.url_edit)

        self.method_combo = QComboBox()
        method_options = []
        if core.CLOUDSCRAPER_AVAILABLE:
            method_options.append(("cloudscraper", "cloudscraper（推荐）"))
        method_options.extend([
            ("curl", "系统 curl"),
            ("urllib", "Python urllib"),
            ("manual", "手动导入（不支持自动下载）"),
        ])
        for key, label in method_options:
            self.method_combo.addItem(label, key)
        for index in range(self.method_combo.count()):
            if self.method_combo.itemData(index) == method:
                self.method_combo.setCurrentIndex(index)
                break
        self.method_combo.currentIndexChanged.connect(self.on_method_changed)
        form.addRow("下载方式", self.method_combo)

        self.warn_label = QLabel("手动模式无法自动下载，请使用「源 → 从本地导入 JSON…」。")
        self.warn_label.setObjectName("warn")
        self.warn_label.hide()
        form.addRow("", self.warn_label)

        layout.addLayout(form)
        layout.addStretch(1)

        footer = QHBoxLayout()
        footer.addStretch(1)

        self.buttons = QDialogButtonBox(
            QDialogButtonBox.StandardButton.Cancel | QDialogButtonBox.StandardButton.Ok
        )
        self.ok_button = self.buttons.button(QDialogButtonBox.StandardButton.Ok)
        self.ok_button.setText("下载并添加" if mode == "add" else "开始更新")
        self.ok_button.setObjectName("accent")
        self.buttons.button(QDialogButtonBox.StandardButton.Cancel).setText("取消")
        self.buttons.accepted.connect(self.accept)
        self.buttons.rejected.connect(self.reject)
        footer.addWidget(self.buttons)
        layout.addLayout(footer)

        self.on_method_changed()

    @property
    def url(self):
        return self._url

    @property
    def method(self):
        return self._method

    def on_method_changed(self, *args):
        manual = self.method_combo.currentData() == "manual"
        self.warn_label.setVisible(manual)
        self.ok_button.setEnabled(not manual)

    def accept(self):
        url = self.url_edit.text().strip()
        if not url:
            show_error(self, "提示", "请输入源地址。")
            return
        if not url.startswith(("http://", "https://")):
            url = "https://" + url
            self.url_edit.setText(url)
        method = self.method_combo.currentData()
        if method == "manual":
            show_error(self, "提示", "手动模式无法自动下载，请改用「从本地导入 JSON」。")
            return
        self._url = url
        self._method = method
        super().accept()

    def showEvent(self, event):
        super().showEvent(event)
        apply_dark_titlebar(self)


class AboutDialog(QDialog):
    def __init__(self, parent, item_count, source_count, sources_dir, thunder_path):
        super().__init__(parent)
        self.setWindowTitle("关于")
        self.setModal(True)
        self.resize(600, 420)
        self.setWindowIcon(QApplication.windowIcon())

        layout = QVBoxLayout(self)
        layout.setContentsMargins(24, 22, 24, 20)
        layout.setSpacing(14)

        head = QHBoxLayout()
        head.setSpacing(14)
        logo = QLabel()
        logo.setPixmap(QApplication.windowIcon().pixmap(56, 56))
        head.addWidget(logo)
        titles = QVBoxLayout()
        titles.setSpacing(2)
        title = QLabel(APP_NAME)
        title.setObjectName("dialogTitle")
        version = QLabel(f"版本 {core.APP_VERSION} · PySide6")
        version.setObjectName("muted")
        titles.addWidget(title)
        titles.addWidget(version)
        head.addLayout(titles)
        head.addStretch(1)
        layout.addLayout(head)

        desc = QLabel("管理本地导入的 JSON 游戏目录，支持去重搜索、来源筛选与一键复制链接。")
        desc.setObjectName("muted")
        desc.setWordWrap(True)
        layout.addWidget(desc)

        card = QFrame()
        card.setObjectName("card")
        card_layout = QVBoxLayout(card)
        card_layout.setContentsMargins(16, 14, 16, 14)
        card_layout.setSpacing(10)

        def add_row(key, value):
            row = QHBoxLayout()
            row.setSpacing(12)
            key_label = QLabel(key)
            key_label.setObjectName("muted")
            key_label.setFixedWidth(72)
            row.addWidget(key_label, 0, Qt.AlignTop)
            value_label = QLabel(value)
            value_label.setWordWrap(True)
            value_label.setTextInteractionFlags(Qt.TextSelectableByMouse)
            row.addWidget(value_label, 1)
            card_layout.addLayout(row)

        add_row("源目录", sources_dir)
        add_row("缓存目录", os.path.join(sources_dir, ".cache"))
        add_row("已加载", f"{item_count:,} 个游戏 · {source_count} 个源")
        add_row("迅雷", thunder_path or "未检测到")
        layout.addWidget(card)

        layout.addStretch(1)
        footer = QHBoxLayout()
        qt_label = QLabel(f"Qt {qVersion()} · PySide6")
        qt_label.setObjectName("faint")
        footer.addWidget(qt_label)
        footer.addStretch(1)
        close_btn = QPushButton("关闭")
        close_btn.setObjectName("accent")
        close_btn.clicked.connect(self.accept)
        footer.addWidget(close_btn)
        layout.addLayout(footer)

    def showEvent(self, event):
        super().showEvent(event)
        apply_dark_titlebar(self)


class BtClientsDialog(QDialog):
    def __init__(self, parent):
        super().__init__(parent)
        self.setWindowTitle("常见 BT 客户端")
        self.setModal(True)
        self.resize(640, 640)
        self.setWindowIcon(QApplication.windowIcon())

        layout = QVBoxLayout(self)
        layout.setContentsMargins(22, 20, 22, 18)
        layout.setSpacing(12)

        title = QLabel("常见 BT 客户端")
        title.setObjectName("dialogTitle")
        layout.addWidget(title)

        intro = QLabel(
            "用 BT 客户端打开磁力链接。以下链接均指向官方下载页，"
            "安装时请留意捆绑组件；如官网访问缓慢，请通过可信渠道获取安装包。"
        )
        intro.setObjectName("muted")
        intro.setWordWrap(True)
        layout.addWidget(intro)

        scroll = QScrollArea()
        scroll.setWidgetResizable(True)
        scroll.setFrameShape(QFrame.NoFrame)
        scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        scroll.viewport().setAutoFillBackground(False)
        content = QWidget()
        content_layout = QVBoxLayout(content)
        content_layout.setContentsMargins(0, 0, 6, 0)
        content_layout.setSpacing(8)
        installed = {
            target["key"] for target in bt.detect() if target["kind"] == "client"
        }
        for client in bt.CATALOG:
            content_layout.addWidget(
                self.build_client_card(client, client["key"] in installed)
            )
        content_layout.addStretch(1)
        scroll.setWidget(content)
        layout.addWidget(scroll, 1)

        footer = QHBoxLayout()
        footer.addStretch(1)
        close_btn = QPushButton("关闭")
        close_btn.setObjectName("accent")
        close_btn.clicked.connect(self.accept)
        footer.addWidget(close_btn)
        layout.addLayout(footer)

    def build_client_card(self, client, installed=False):
        card = QFrame()
        card.setObjectName("card")
        layout = QHBoxLayout(card)
        layout.setContentsMargins(14, 12, 14, 12)
        layout.setSpacing(12)

        badge = QLabel(client["badge"])
        badge.setFixedSize(38, 38)
        badge.setAlignment(Qt.AlignCenter)
        badge.setStyleSheet(
            f"background-color: {client['color']}; color: #ffffff; border-radius: 9px;"
            "font-weight: 800; font-size: 9pt;"
        )
        layout.addWidget(badge)

        info = QVBoxLayout()
        info.setSpacing(2)
        name_row = QHBoxLayout()
        name_row.setSpacing(8)
        name = QLabel(client["name"])
        name.setObjectName("cardTitle")
        name_row.addWidget(name)
        if client.get("recommended"):
            chip = QLabel("推荐")
            chip.setObjectName("chip")
            name_row.addWidget(chip)
        if installed:
            chip = QLabel("已安装")
            chip.setObjectName("installedTag")
            name_row.addWidget(chip)
        name_row.addStretch(1)
        info.addLayout(name_row)
        desc = QLabel(client["desc"])
        desc.setObjectName("muted")
        desc.setWordWrap(True)
        info.addWidget(desc)
        layout.addLayout(info, 1)

        open_btn = QPushButton("打开下载页")
        open_btn.setToolTip(client["url"])
        open_btn.clicked.connect(lambda: QDesktopServices.openUrl(QUrl(client["url"])))
        layout.addWidget(open_btn, 0, Qt.AlignVCenter)
        return card

    def showEvent(self, event):
        super().showEvent(event)
        apply_dark_titlebar(self)
