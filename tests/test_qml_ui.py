import os
import shutil
import sys
import tempfile
import time
import unittest

os.environ.setdefault("QT_QPA_PLATFORM", "offscreen")
sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

try:
    from PySide6.QtCore import QObject, QPoint, QPointF, Qt, QUrl
    from PySide6.QtGui import QGuiApplication
    from PySide6.QtQml import QQmlApplicationEngine, QQmlExpression
    from PySide6.QtQuickControls2 import QQuickStyle
    from PySide6.QtTest import QTest

    import hydra_core as core
    import hydra_qml_backend as backend_module
    import hydra_qml_theme as theme_module
    PYSIDE_AVAILABLE = True
except ImportError:
    PYSIDE_AVAILABLE = False


def relative_luminance(hex_color):
    value = hex_color.lstrip("#")
    channels = [int(value[i:i + 2], 16) / 255 for i in (0, 2, 4)]

    def linear(channel):
        return channel / 12.92 if channel <= 0.03928 else ((channel + 0.055) / 1.055) ** 2.4

    red, green, blue = (linear(channel) for channel in channels)
    return 0.2126 * red + 0.7152 * green + 0.0722 * blue


def contrast_ratio(background, foreground):
    first = relative_luminance(background)
    second = relative_luminance(foreground)
    lighter, darker = max(first, second), min(first, second)
    return (lighter + 0.05) / (darker + 0.05)


@unittest.skipUnless(PYSIDE_AVAILABLE, "PySide6 QML not installed")
class QmlUiTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.app = QGuiApplication.instance() or QGuiApplication([])
        QQuickStyle.setStyle("Basic")

    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="gamemagnet_qml_")
        sources = os.path.join(self.tmp, "sources")
        os.makedirs(sources)
        core.write_source_file(
            os.path.join(sources, "sample.json.gz"),
            {
                "downloads": [
                    {
                        "title": "Sample Game",
                        "uris": ["https://example.com/sample"],
                        "fileSize": "1 GB",
                        "uploadDate": "2026-01-01",
                    }
                ]
            },
        )
        self.theme = theme_module.ThemeBridge()
        self.backend = backend_module.Backend(sources)
        self.engine = QQmlApplicationEngine()
        self.engine.rootContext().setContextProperty("backend", self.backend)
        self.engine.rootContext().setContextProperty("theme", self.theme)
        qml = os.path.join(os.path.dirname(os.path.dirname(__file__)), "qml", "Main.qml")
        self.engine.load(QUrl.fromLocalFile(qml))
        self.assertTrue(self.engine.rootObjects(), "Main.qml failed to load")
        self.window = self.engine.rootObjects()[0]

    def tearDown(self):
        self.window.close()
        self.backend.shutdown()
        self.engine.deleteLater()
        self.app.processEvents()
        shutil.rmtree(self.tmp, ignore_errors=True)

    def wait_until(self, predicate, timeout=15):
        deadline = time.time() + timeout
        while time.time() < deadline:
            self.app.processEvents()
            if predicate():
                return True
            time.sleep(0.02)
        return False

    def test_qml_search_and_detail_flow(self):
        self.assertTrue(
            self.wait_until(
                lambda: len(self.backend.library.metas) == 1
                and not self.backend.tasks.busy()
            ),
            "sources did not load",
        )
        self.assertEqual(self.backend.sources[0]["count"], 1)
        self.assertEqual(self.backend.sources[1]["display"], "sample")

        self.backend.set_query("sample")
        self.assertTrue(
            self.wait_until(lambda: self.backend.resultsModel.rowCount() > 0),
            "search returned no rows",
        )

        self.backend.select_result(0)
        self.assertTrue(self.backend.hasDetail)
        self.assertEqual(self.backend.detailTitle, "Sample Game")

    def test_qml_source_selection_updates_header(self):
        self.assertTrue(
            self.wait_until(
                lambda: len(self.backend.library.metas) == 1
                and not self.backend.tasks.busy()
            )
        )
        self.backend.select_source("sample.json.gz")
        self.assertEqual(self.backend.pageTitle, "sample")
        self.assertEqual(self.backend.scopeText, "sample")

    def test_empty_query_shows_latest_games_by_default(self):
        self.assertTrue(
            self.wait_until(
                lambda: len(self.backend.library.metas) == 1
                and not self.backend.tasks.busy()
            )
        )
        self.assertEqual(self.backend.query, "")
        self.assertTrue(
            self.wait_until(lambda: self.backend.resultsModel.rowCount() == 1),
            "default latest listing did not populate",
        )
        self.assertEqual(self.backend.countText, "共 1 组")
        self.assertFalse(self.backend.hasDetail)

        self.backend.select_result(0)
        self.assertTrue(self.backend.hasDetail)

        self.backend.set_query("")
        self.assertTrue(
            self.wait_until(lambda: not self.backend.hasDetail),
            "clearing the query did not return to the latest listing",
        )
        self.assertEqual(self.backend.resultsModel.rowCount(), 1)
        self.assertEqual(self.backend.countText, "共 1 组")

    def test_update_task_kinds_dispatch_separately(self):
        calls = []
        self.backend.on_updated = lambda change: calls.append(("source", change))
        self.backend.on_checked_for_updates = lambda outcome: calls.append(
            ("check", outcome)
        )

        self.backend.on_done("update", "change")
        self.backend.on_done("check_update", "outcome")

        self.assertEqual(calls, [("source", "change"), ("check", "outcome")])

    def palette_color(self, menu, role):
        expr = QQmlExpression(self.engine.rootContext(), menu, "palette." + role)
        value, undefined = expr.evaluate()
        self.assertFalse(undefined, "palette.%s is undefined" % role)
        return value

    def test_overflow_menu_drops_actions_that_have_buttons(self):
        menu = self.window.findChild(QObject, "overflowMenu")
        self.assertIsNotNone(menu)
        texts = [
            item.property("text")
            for item in menu.findChildren(QObject)
            if item.metaObject().className().startswith("MenuItem_QMLTYPE")
        ]
        for removed in (
            "通过 URL 添加源…",
            "从本地导入 JSON…",
            "刷新全部源",
            "显示详情面板",
            "切换明暗主题",
            "重置布局",
            "退出",
        ):
            self.assertNotIn(removed, texts)
        for kept in (
            "更新选中源…",
            "删除选中源",
            "打开源文件夹",
            "聚焦搜索框",
            "常见 BT 客户端…",
            "检查更新…",
            "关于",
        ):
            self.assertIn(kept, texts)

    def test_menu_highlight_text_is_readable(self):
        tokens = (
            theme_module.DARK_THEME if self.theme.dark else theme_module.LIGHT_THEME
        )
        menu = self.window.findChild(QObject, "overflowMenu")
        light = self.palette_color(menu, "light")
        window_text = self.palette_color(menu, "windowText")
        self.assertEqual(light.name(), tokens["menu_selection"])
        self.assertEqual(window_text.name(), tokens["text"])
        # Basic 样式用 palette.light 作选中背景、palette.windowText 作文字色，
        # 两者对比度必须足够，否则深色主题下选中项文字会看不见。
        self.assertGreater(contrast_ratio(light.name(), window_text.name()), 4.5)

    def test_sidebar_library_collapses(self):
        sidebar = self.window.findChild(QObject, "sidebar")
        self.assertIsNotNone(sidebar)
        source_list = sidebar.findChild(QObject, "sourceList")
        self.assertIsNotNone(source_list)
        self.assertFalse(sidebar.property("collapsed"))
        self.assertTrue(source_list.property("visible"))

        sidebar.setProperty("collapsed", True)
        self.app.processEvents()
        self.assertTrue(sidebar.property("collapsed"))
        self.assertFalse(source_list.property("visible"))

        sidebar.setProperty("collapsed", False)
        self.app.processEvents()
        self.assertTrue(source_list.property("visible"))

    def test_double_click_result_opens_detail(self):
        self.assertTrue(
            self.wait_until(
                lambda: len(self.backend.library.metas) == 1
                and not self.backend.tasks.busy()
            )
        )
        self.backend.set_query("sample")
        self.assertTrue(
            self.wait_until(lambda: self.backend.resultsModel.rowCount() > 0)
        )
        self.window.setProperty("detailsOpen", False)
        self.assertTrue(self.wait_until(lambda: not self.window.property("detailsOpen")))

        def result_row():
            view = self.window.findChild(QObject, "resultList")
            if view is None:
                return None
            content = view.property("contentItem")
            if content is None:
                return None
            for child in content.childItems():
                if child.objectName() == "resultRow":
                    return child
            return None

        self.assertTrue(
            self.wait_until(lambda: result_row() is not None),
            "result row was not created",
        )
        row = result_row()
        self.assertTrue(
            self.wait_until(lambda: row.property("visible") and row.width() > 0)
        )
        point = row.mapToScene(QPointF(row.width() / 2, row.height() / 2))
        QTest.mouseDClick(
            self.window,
            Qt.LeftButton,
            Qt.NoModifier,
            QPoint(round(point.x()), round(point.y())),
        )
        self.assertTrue(
            self.wait_until(
                lambda: self.window.property("detailsOpen") and self.backend.hasDetail
            ),
            "double click did not open the detail panel",
        )


if __name__ == "__main__":
    unittest.main()
