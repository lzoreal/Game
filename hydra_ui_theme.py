"""Qt theme, app icon, dialogs helpers and native dark chrome."""

import os

from PySide6.QtCore import Qt, QTimer
from PySide6.QtGui import (
    QBrush,
    QColor,
    QFont,
    QFontDatabase,
    QIcon,
    QLinearGradient,
    QPainter,
    QPalette,
    QPixmap,
)
from PySide6.QtWidgets import QApplication, QMessageBox

import hydra_core as core

FONT_FAMILY = "Microsoft YaHei UI"
ACCENT = "#3b82f6"
APP_NAME = "GameMagNet 游戏目录管理器"
STATUS_RIGHT = f"{core.APP_VERSION} · PySide6"

QSS = f"""
* {{
    font-family: "{FONT_FAMILY}";
    font-size: 10pt;
    color: #e6e8eb;
}}
QWidget#root {{ background-color: #0f1115; }}
QFrame#sidebar {{
    background-color: #12151b;
    border-right: 1px solid #1f242c;
}}
QLabel#appTitle {{ font-size: 12pt; font-weight: 700; color: #f2f4f7; }}
QLabel#logo {{
    background-color: {ACCENT};
    color: #ffffff;
    border-radius: 9px;
    font-size: 15pt;
    font-weight: 800;
}}
QLabel#section {{ color: #6f7887; font-size: 9pt; font-weight: 700; }}
QLabel#muted {{ color: #8b93a1; }}
QLabel#faint {{ color: #5f6875; font-size: 9pt; }}
QLabel#warn {{ color: #f39c12; font-size: 9pt; }}
QLabel#tag {{
    background-color: #1c2027;
    color: #7f8896;
    border-radius: 7px;
    padding: 1px 6px;
    font-size: 8pt;
}}
QLabel#emptyIcon {{ color: #333b47; font-size: 40pt; }}
QLabel#emptyTitle {{ font-size: 15pt; font-weight: 700; color: #dfe3e9; }}
QLabel#detailTitle {{ font-size: 14pt; font-weight: 700; color: #f2f4f7; }}
QLabel#dialogTitle {{ font-size: 14pt; font-weight: 700; color: #f2f4f7; }}
QLabel#cardTitle {{ font-size: 10.5pt; font-weight: 700; color: #eef1f5; }}
QLabel#linkGroup {{ font-size: 9.5pt; font-weight: 700; color: #c3cad4; }}
QLabel#chip {{
    background-color: #1c2738;
    color: #9ec1ff;
    border-radius: 9px;
    padding: 2px 9px;
    font-size: 9pt;
}}
QLabel#installedTag {{
    background-color: #12291f;
    color: #6ee7b7;
    border-radius: 9px;
    padding: 2px 9px;
    font-size: 9pt;
}}
QLabel#link {{ color: #6ea8ff; font-size: 9pt; }}
QLabel#link:hover {{ color: #a8c8ff; }}
QLabel#descText {{
    background-color: #11141a;
    border: 1px solid #20262f;
    border-radius: 8px;
    padding: 9px;
    color: #aeb7c3;
    font-size: 9pt;
}}

QLineEdit {{
    background-color: #14171c;
    border: 1px solid #2a303a;
    border-radius: 8px;
    padding: 7px 10px;
    color: #e6e8eb;
    selection-background-color: #2a4a78;
}}
QLineEdit:focus {{ border-color: {ACCENT}; }}
QLineEdit#search {{
    background-color: #171a20;
    border: 1px solid #2a303a;
    border-radius: 12px;
    padding: 10px 16px;
    font-size: 12.5pt;
    color: #eef1f5;
}}

QPushButton {{
    background-color: #1b2028;
    border: 1px solid #2a313c;
    border-radius: 8px;
    padding: 7px 14px;
    color: #d7dce3;
}}
QPushButton:hover {{ background-color: #232a35; border-color: #3a4352; }}
QPushButton:pressed {{ background-color: #171b22; }}
QPushButton:disabled {{ color: #5f6875; background-color: #161a20; border-color: #242a33; }}
QPushButton#accent {{
    background-color: {ACCENT};
    border: 1px solid {ACCENT};
    color: #ffffff;
    font-weight: 600;
}}
QPushButton#accent:hover {{ background-color: #4f8df8; border-color: #4f8df8; }}
QPushButton#ghost {{
    background: transparent;
    border: none;
    color: #8b93a1;
    padding: 2px 6px;
}}
QPushButton#ghost:hover {{ color: #e6e8eb; }}
QPushButton#btLink {{
    background: transparent;
    border: none;
    color: #6ea8ff;
    padding: 2px 4px;
    font-size: 9pt;
}}
QPushButton#btLink:hover {{ color: #a8c8ff; }}
QPushButton#toggle {{ padding: 5px 12px; }}
QPushButton#toggle:checked {{
    background-color: #172338;
    border: 1px solid {ACCENT};
    color: #9ec1ff;
}}

QDoubleSpinBox {{
    background-color: #171a20;
    border: 1px solid #2a303a;
    border-radius: 7px;
    padding: 4px 6px;
    min-width: 56px;
}}
QComboBox {{
    background-color: #14171c;
    border: 1px solid #2a303a;
    border-radius: 7px;
    padding: 5px 10px;
    min-width: 140px;
}}
QComboBox:hover {{ border-color: #3a4352; }}
QComboBox QAbstractItemView {{
    background-color: #171a20;
    border: 1px solid #2a313c;
    selection-background-color: #1e2a3d;
    outline: 0;
}}

QMenuBar {{
    background-color: #0c0e12;
    border-bottom: 1px solid #1f242c;
    padding: 2px 8px;
}}
QMenuBar::item {{ background: transparent; padding: 5px 10px; border-radius: 6px; }}
QMenuBar::item:selected {{ background-color: #1b2028; color: #ffffff; }}
QMenu {{
    background-color: #171a20;
    border: 1px solid #2a313c;
    padding: 6px;
}}
QMenu::item {{ padding: 6px 26px 6px 12px; border-radius: 6px; color: #d7dce3; }}
QMenu::item:selected {{ background-color: #1e2a3d; color: #ffffff; }}
QMenu::item:disabled {{ color: #5f6875; }}
QMenu::separator {{ height: 1px; background-color: #262b34; margin: 4px 8px; }}

QDialog {{ background-color: #12151b; }}
QMessageBox {{ background-color: #12151b; }}

QFrame#detailPanel {{
    background-color: #12151b;
    border-left: 1px solid #1f242c;
}}
QFrame#card {{
    background-color: #161a20;
    border: 1px solid #242a33;
    border-radius: 10px;
}}

QListWidget#sourceList {{ background: transparent; border: none; outline: 0; }}
QListWidget#sourceList::item {{ border-radius: 8px; margin: 1px 0; color: #aeb6c2; }}
QListWidget#sourceList::item:hover {{ background-color: #191e26; }}
QListWidget#sourceList::item:selected {{ background-color: #1b2a3f; color: #ffffff; }}

QTableView {{
    background-color: #14171c;
    alternate-background-color: #171b21;
    border: none;
    gridline-color: transparent;
    selection-background-color: #1e2f4a;
    selection-color: #ffffff;
    outline: 0;
}}
QTableView::item {{ padding: 2px 10px; border-bottom: 1px solid #1c2027; }}
QHeaderView::section {{
    background-color: #14171c;
    color: #7f8896;
    border: none;
    border-bottom: 1px solid #262b34;
    padding: 9px 10px;
    font-size: 9pt;
    font-weight: 600;
}}
QTableCornerButton::section {{ background-color: #14171c; border: none; }}

QScrollArea {{ background: transparent; border: none; }}
QScrollArea > QWidget > QWidget {{ background: transparent; }}
QScrollBar:vertical {{ background: transparent; width: 9px; margin: 0; }}
QScrollBar::handle:vertical {{ background: #2c333f; border-radius: 4px; min-height: 28px; }}
QScrollBar::handle:vertical:hover {{ background: #3b4553; }}
QScrollBar::add-line:vertical, QScrollBar::sub-line:vertical {{ height: 0; }}
QScrollBar:horizontal {{ background: transparent; height: 9px; margin: 0; }}
QScrollBar::handle:horizontal {{ background: #2c333f; border-radius: 4px; min-width: 28px; }}
QScrollBar::add-line:horizontal, QScrollBar::sub-line:horizontal {{ width: 0; }}

QProgressBar#lineBar {{ background-color: #171b21; border: none; border-radius: 1px; }}
QProgressBar#lineBar::chunk {{ background-color: {ACCENT}; border-radius: 1px; }}

QFrame#statusBar {{ background-color: #0c0e12; border-top: 1px solid #1c2027; }}
QSplitter::handle {{ background-color: #1f242c; }}
QToolTip {{
    background-color: #1b2028;
    color: #e6e8eb;
    border: 1px solid #303845;
    padding: 4px 6px;
}}
"""


def show_info(parent, title, text):
    box = QMessageBox(parent)
    box.setWindowTitle(title)
    box.setText(text)
    box.setIcon(QMessageBox.Icon.Information)
    box.addButton("确定", QMessageBox.ButtonRole.AcceptRole)
    prepare_dialog(box)
    box.exec()


def show_error(parent, title, text):
    box = QMessageBox(parent)
    box.setWindowTitle(title)
    box.setText(text)
    box.setIcon(QMessageBox.Icon.Critical)
    box.addButton("确定", QMessageBox.ButtonRole.AcceptRole)
    prepare_dialog(box)
    box.exec()


def ask_confirm(parent, title, text, ok_text="确认", cancel_text="取消"):
    box = QMessageBox(parent)
    box.setWindowTitle(title)
    box.setText(text)
    box.setIcon(QMessageBox.Icon.Question)
    ok_button = box.addButton(ok_text, QMessageBox.ButtonRole.AcceptRole)
    box.addButton(cancel_text, QMessageBox.ButtonRole.RejectRole)
    prepare_dialog(box)
    box.exec()
    return box.clickedButton() is ok_button


def create_app_icon():
    icon = QIcon()
    for size in (16, 24, 32, 48, 64, 128, 256):
        pixmap = QPixmap(size, size)
        pixmap.fill(Qt.transparent)
        painter = QPainter(pixmap)
        painter.setRenderHint(QPainter.Antialiasing)
        gradient = QLinearGradient(0, 0, 0, size)
        gradient.setColorAt(0.0, QColor("#4f8df8"))
        gradient.setColorAt(1.0, QColor("#2f6fd8"))
        painter.setBrush(QBrush(gradient))
        painter.setPen(Qt.NoPen)
        radius = size * 0.22
        painter.drawRoundedRect(0, 0, size, size, radius, radius)
        painter.setPen(QColor("#ffffff"))
        painter.setFont(QFont("Segoe UI", max(6, int(size * 0.52)), QFont.Bold))
        painter.drawText(pixmap.rect(), Qt.AlignCenter, "G")
        painter.end()
        icon.addPixmap(pixmap)
    return icon


def setup_app(app):
    app.setStyle("Fusion")
    app.setWindowIcon(create_app_icon())
    family = FONT_FAMILY
    try:
        families = set(QFontDatabase.families())
        for candidate in ("Microsoft YaHei UI", "微软雅黑", "Microsoft YaHei"):
            if candidate in families:
                family = candidate
                break
    except Exception:
        pass
    app.setFont(QFont(family, 10))

    palette = QPalette()
    palette.setColor(QPalette.Window, QColor("#0f1115"))
    palette.setColor(QPalette.WindowText, QColor("#e6e8eb"))
    palette.setColor(QPalette.Base, QColor("#14171c"))
    palette.setColor(QPalette.AlternateBase, QColor("#171b21"))
    palette.setColor(QPalette.Text, QColor("#e6e8eb"))
    palette.setColor(QPalette.Button, QColor("#1b2028"))
    palette.setColor(QPalette.ButtonText, QColor("#d7dce3"))
    palette.setColor(QPalette.Highlight, QColor("#2a4a78"))
    palette.setColor(QPalette.HighlightedText, QColor("#ffffff"))
    palette.setColor(QPalette.ToolTipBase, QColor("#1b2028"))
    palette.setColor(QPalette.ToolTipText, QColor("#e6e8eb"))
    palette.setColor(QPalette.PlaceholderText, QColor("#626b78"))
    app.setPalette(palette)
    app.setStyleSheet(QSS)


def apply_dark_titlebar(window):
    if os.name != "nt":
        return
    try:
        import ctypes

        hwnd = int(window.winId())
        enabled = ctypes.c_int(1)
        for attribute in (20, 19):
            try:
                result = ctypes.windll.dwmapi.DwmSetWindowAttribute(
                    hwnd, attribute, ctypes.byref(enabled), ctypes.sizeof(enabled)
                )
                if result == 0:
                    break
            except Exception:
                continue

        def colorref(hex_color):
            value = hex_color.lstrip("#")
            red = int(value[0:2], 16)
            green = int(value[2:4], 16)
            blue = int(value[4:6], 16)
            return red | (green << 8) | (blue << 16)

        for attribute, color in ((35, "#0c0e12"), (36, "#e6e8eb"), (34, "#1f242c")):
            value = ctypes.c_int(colorref(color))
            try:
                ctypes.windll.dwmapi.DwmSetWindowAttribute(
                    hwnd, attribute, ctypes.byref(value), ctypes.sizeof(value)
                )
            except Exception:
                pass
    except Exception:
        pass


def prepare_dialog(widget):
    widget.setWindowIcon(QApplication.windowIcon())
    QTimer.singleShot(0, lambda: apply_dark_titlebar(widget))
