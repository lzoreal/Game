"""Details pane: renders a DetailModel and emits user intents."""

from functools import partial

from PySide6.QtCore import Qt, QTimer, Signal
from PySide6.QtGui import QFont, QFontMetrics
from PySide6.QtWidgets import (
    QFrame,
    QHBoxLayout,
    QLabel,
    QPlainTextEdit,
    QPushButton,
    QScrollArea,
    QSizePolicy,
    QVBoxLayout,
    QWidget,
)

import hydra_bt as bt
from hydra_details import build_detail
from hydra_ui_widgets import ClickableLabel


class DetailsPane(QFrame):
    copy_requested = Signal(str)
    bt_requested = Signal(str)
    collapse_requested = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self.setObjectName("detailPanel")
        self.setMinimumWidth(320)
        self.current_group = None
        self.expanded_keys = set()
        self.raw_visible = False
        self.bt_targets = []

        layout = QVBoxLayout(self)
        layout.setContentsMargins(18, 14, 12, 10)
        layout.setSpacing(8)

        header = QHBoxLayout()
        section = QLabel("游戏详情")
        section.setObjectName("section")
        header.addWidget(section)
        header.addStretch(1)
        self.detail_hint = QLabel("")
        self.detail_hint.setObjectName("faint")
        header.addWidget(self.detail_hint)
        collapse = QPushButton("收起")
        collapse.setObjectName("ghost")
        collapse.clicked.connect(self.collapse_requested)
        header.addWidget(collapse)
        layout.addLayout(header)

        self.detail_scroll = QScrollArea()
        self.detail_scroll.setWidgetResizable(True)
        self.detail_scroll.setFrameShape(QFrame.NoFrame)
        self.detail_scroll.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.detail_scroll.viewport().setAutoFillBackground(False)
        layout.addWidget(self.detail_scroll, 1)

        self.clear()

    def clear(self):
        self.current_group = None
        self.expanded_keys.clear()
        self.raw_visible = False
        self.detail_hint.setText("")
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setAlignment(Qt.AlignCenter)
        layout.setSpacing(6)
        icon = QLabel("◇")
        icon.setObjectName("emptyIcon")
        icon.setAlignment(Qt.AlignCenter)
        title = QLabel("选择一个游戏")
        title.setObjectName("muted")
        title.setAlignment(Qt.AlignCenter)
        hint = QLabel("点击搜索结果查看来源与下载链接")
        hint.setObjectName("faint")
        hint.setAlignment(Qt.AlignCenter)
        layout.addStretch(1)
        layout.addWidget(icon)
        layout.addWidget(title)
        layout.addWidget(hint)
        layout.addStretch(1)
        self.detail_scroll.setWidget(page)

    def refresh(self):
        if self.current_group:
            self.show_group(self.current_group, bt.detect())

    def show_group(self, group, targets=None):
        self.bt_targets = list(targets or [])
        same_group = group is self.current_group
        if not same_group:
            self.expanded_keys.clear()
            self.raw_visible = False
        self.current_group = group
        scroll_pos = self.detail_scroll.verticalScrollBar().value() if same_group else 0

        model = build_detail(group, self.expanded_keys, self.raw_visible)

        content = QWidget()
        layout = QVBoxLayout(content)
        layout.setContentsMargins(0, 2, 6, 12)
        layout.setSpacing(10)

        title = QLabel(model.title)
        title.setObjectName("detailTitle")
        title.setWordWrap(True)
        title.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
        layout.addWidget(title)

        chips = QHBoxLayout()
        chips.setSpacing(6)
        for source in model.sources:
            chip = QLabel(source)
            chip.setObjectName("chip")
            chips.addWidget(chip)
        chips.addStretch(1)
        layout.addLayout(chips)

        meta = QLabel(model.meta)
        meta.setObjectName("muted")
        layout.addWidget(meta)

        for card in model.cards:
            layout.addWidget(self.build_source_card(card))

        raw_button = QPushButton(model.raw_label)
        raw_button.setObjectName("ghost")
        raw_button.clicked.connect(self.toggle_raw_json)
        layout.addWidget(raw_button)

        if model.raw_visible:
            editor = QPlainTextEdit()
            editor.setReadOnly(True)
            editor.setPlainText(model.raw_text)
            editor.setFont(QFont("Consolas", 9))
            editor.setMaximumHeight(200)
            editor.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
            layout.addWidget(editor)

        layout.addStretch(1)
        self.detail_hint.setText(model.hint)
        self.detail_scroll.setWidget(content)
        if same_group and scroll_pos:
            QTimer.singleShot(
                0, lambda: self.detail_scroll.verticalScrollBar().setValue(scroll_pos)
            )

    def build_source_card(self, card):
        frame = QFrame()
        frame.setObjectName("card")
        layout = QVBoxLayout(frame)
        layout.setContentsMargins(14, 12, 14, 12)
        layout.setSpacing(7)

        head = QHBoxLayout()
        name = QLabel(card.source)
        name.setObjectName("cardTitle")
        head.addWidget(name)
        head.addStretch(1)
        meta = QLabel(f"{card.date_text}   ·   {card.size}")
        meta.setObjectName("faint")
        head.addWidget(meta)
        layout.addLayout(head)

        if card.description:
            label = QLabel(card.description)
            label.setObjectName("descText")
            label.setWordWrap(True)
            label.setTextInteractionFlags(Qt.TextSelectableByMouse)
            label.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
            layout.addWidget(label)

        viewport_width = self.detail_scroll.viewport().width() or 340
        for link_group in card.links:
            group_label = QLabel(f"{link_group.label} · {link_group.total}")
            group_label.setObjectName("linkGroup")
            layout.addWidget(group_label)

            link_width = max(
                120, viewport_width - (200 if link_group.label == "磁力链接" else 120)
            )
            for link in link_group.visible:
                uri = link.uri
                link_row = QHBoxLayout()
                link_row.setSpacing(4)
                label = ClickableLabel(self.elide(uri, link_width))
                label.setObjectName("link")
                label.setToolTip(uri)
                label.setSizePolicy(QSizePolicy.Ignored, QSizePolicy.Preferred)
                label.clicked.connect(partial(self.copy_requested.emit, uri))
                link_row.addWidget(label, 1)
                copy_one = QPushButton("复制")
                copy_one.setObjectName("ghost")
                copy_one.clicked.connect(partial(self.copy_requested.emit, uri))
                link_row.addWidget(copy_one)
                if link.is_magnet:
                    bt_button = QPushButton("BT 下载")
                    bt_button.setObjectName("btLink")
                    bt_button.setToolTip(
                        "用本机 BT 客户端下载此磁力\n已检测: "
                        + self.bt_targets_hint(self.bt_targets)
                    )
                    bt_button.clicked.connect(
                        lambda checked=False, u=uri: self.bt_requested.emit(u)
                    )
                    link_row.addWidget(bt_button)
                layout.addLayout(link_row)

            if link_group.can_expand:
                expand = QPushButton(f"展开其余 {link_group.hidden} 条")
                expand.setObjectName("ghost")
                expand.clicked.connect(partial(self.expand_card, card.key))
                layout.addWidget(expand)

        return frame

    def elide(self, text, width):
        metrics = QFontMetrics(self.font())
        return metrics.elidedText(text, Qt.ElideMiddle, width)

    def expand_card(self, key):
        self.expanded_keys.add(key)
        if self.current_group:
            self.show_group(self.current_group, self.bt_targets)

    def toggle_raw_json(self):
        self.raw_visible = not self.raw_visible
        if self.current_group:
            self.show_group(self.current_group, self.bt_targets)

    @staticmethod
    def bt_targets_hint(targets):
        clients = [target for target in targets if target["kind"] == "client"]
        if clients:
            names = " / ".join(target["name"] for target in clients[:2])
            if len(clients) > 2:
                names += f" 等 {len(clients)} 个"
            return names
        system = next((target for target in targets if target["kind"] == "system"), None)
        if system:
            return f"系统默认：{system['name']}"
        return "未检测到 BT 客户端"
