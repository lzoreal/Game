"""QML entry point for the GameMagNet desktop UI."""

import os
import sys

from PySide6.QtCore import Qt, QUrl
from PySide6.QtGui import QFont, QGuiApplication, QIcon
from PySide6.QtQml import QQmlApplicationEngine
from PySide6.QtQuickControls2 import QQuickStyle

import app_backend as backend_module
import theme_tokens as theme_module


def app_base_dir():
    if getattr(sys, "frozen", False):
        return os.path.dirname(os.path.abspath(sys.executable))
    return os.path.dirname(os.path.abspath(__file__))


def resource_dir(name):
    if getattr(sys, "frozen", False):
        base = getattr(sys, "_MEIPASS", app_base_dir())
    else:
        base = app_base_dir()
    return os.path.join(base, name)


def apply_window_style(window, theme):
    if os.name != "nt":
        return
    try:
        import ctypes

        hwnd = int(window.winId())
        rounded = ctypes.c_int(2)
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 33, ctypes.byref(rounded), ctypes.sizeof(rounded)
        )

        def colorref(hex_color):
            value = hex_color.lstrip("#")
            red = int(value[0:2], 16)
            green = int(value[2:4], 16)
            blue = int(value[4:6], 16)
            return red | (green << 8) | (blue << 16)

        border = ctypes.c_int(colorref(theme.colors["border"]))
        ctypes.windll.dwmapi.DwmSetWindowAttribute(
            hwnd, 34, ctypes.byref(border), ctypes.sizeof(border)
        )
    except Exception:
        pass


def main():
    try:
        QGuiApplication.setHighDpiScaleFactorRoundingPolicy(
            Qt.HighDpiScaleFactorRoundingPolicy.PassThrough
        )
    except Exception:
        pass

    QQuickStyle.setStyle("Basic")
    app = QGuiApplication(sys.argv)
    app.setApplicationName("GameMagNet")
    app.setOrganizationName("GameMagNet")
    app.setFont(QFont("Microsoft YaHei UI", 10))

    icon_path = os.path.join(resource_dir("assets"), "gamemagnet.ico")
    if os.path.exists(icon_path):
        app.setWindowIcon(QIcon(icon_path))

    sources_dir = os.path.join(app_base_dir(), "sources")
    os.makedirs(sources_dir, exist_ok=True)

    theme = theme_module.ThemeBridge()
    backend = backend_module.Backend(sources_dir)

    engine = QQmlApplicationEngine()
    engine.rootContext().setContextProperty("backend", backend)
    engine.rootContext().setContextProperty("theme", theme)
    engine.load(QUrl.fromLocalFile(os.path.join(resource_dir("qml"), "Main.qml")))

    if not engine.rootObjects():
        sys.exit(1)

    window = engine.rootObjects()[0]
    apply_window_style(window, theme)
    theme.darkChanged.connect(lambda: apply_window_style(window, theme))
    app.aboutToQuit.connect(backend.shutdown)
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
