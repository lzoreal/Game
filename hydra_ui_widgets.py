"""Reusable result-table and sidebar row widgets."""

from PySide6.QtCore import QAbstractTableModel, QModelIndex, Qt, Signal
from PySide6.QtGui import QColor
from PySide6.QtWidgets import QHBoxLayout, QLabel, QWidget


class GameTableModel(QAbstractTableModel):
    HEADERS = ("游戏名称", "来源", "大小", "上传日期", "链接数")

    def __init__(self):
        super().__init__()
        self.groups = []

    def set_groups(self, groups):
        self.beginResetModel()
        self.groups = groups
        self.endResetModel()

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.groups)

    def columnCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self.HEADERS)

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid():
            return None
        group = self.groups[index.row()]
        col = index.column()
        if role == Qt.DisplayRole:
            values = (
                group["title"],
                " + ".join(group["sources"]),
                str(group["size"]),
                group["date_display"],
                str(group["link_count"]),
            )
            return values[col]
        if role == Qt.UserRole:
            return group
        if role == Qt.ToolTipRole:
            return group["title"]
        if role == Qt.TextAlignmentRole:
            if col in (2, 3, 4):
                return Qt.AlignCenter
            return Qt.AlignVCenter | Qt.AlignLeft
        if role == Qt.ForegroundRole and col in (2, 3, 4):
            return QColor("#98a1ae")
        return None

    def headerData(self, section, orientation, role=Qt.DisplayRole):
        if orientation == Qt.Horizontal:
            if role == Qt.DisplayRole:
                return self.HEADERS[section]
            if role == Qt.TextAlignmentRole:
                return Qt.AlignCenter if section in (2, 3, 4) else Qt.AlignVCenter | Qt.AlignLeft
            if role == Qt.ToolTipRole and section == 3:
                return "点击切换 新→旧 / 旧→新"
        return None

    def group_at(self, row):
        if 0 <= row < len(self.groups):
            return self.groups[row]
        return None

    def row_for_group(self, group):
        for row, candidate in enumerate(self.groups):
            if candidate is group:
                return row
        return -1


class ClickableLabel(QLabel):
    clicked = Signal()

    def __init__(self, text="", parent=None):
        super().__init__(text, parent)
        self.setCursor(Qt.PointingHandCursor)

    def mouseReleaseEvent(self, event):
        if event.button() == Qt.LeftButton:
            self.clicked.emit()
        super().mouseReleaseEvent(event)


class SourceRow(QWidget):
    def __init__(self, name, count, local=False):
        super().__init__()
        self.setAttribute(Qt.WA_TransparentForMouseEvents)
        layout = QHBoxLayout(self)
        layout.setContentsMargins(10, 4, 10, 4)
        layout.setSpacing(6)
        label = QLabel(name)
        label.setAttribute(Qt.WA_TransparentForMouseEvents)
        layout.addWidget(label)
        if local:
            tag = QLabel("本地")
            tag.setObjectName("tag")
            tag.setAttribute(Qt.WA_TransparentForMouseEvents)
            layout.addWidget(tag)
        layout.addStretch(1)
        count_label = QLabel(f"{count:,}")
        count_label.setObjectName("faint")
        count_label.setAttribute(Qt.WA_TransparentForMouseEvents)
        layout.addWidget(count_label)
