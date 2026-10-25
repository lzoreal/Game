"""本地游戏目录管理器 v4.0 — PySide6 桌面版。"""

import os
import subprocess
import sys
import time
from PySide6.QtCore import QSize, Qt, QTimer, QUrl
from PySide6.QtGui import (
    QAction,
    QActionGroup,
    QColor,
    QDesktopServices,
    QGuiApplication,
    QKeySequence,
    QPalette,
)
from PySide6.QtWidgets import (
    QAbstractItemView,
    QApplication,
    QDialog,
    QDoubleSpinBox,
    QFileDialog,
    QFrame,
    QHBoxLayout,
    QHeaderView,
    QInputDialog,
    QLabel,
    QLineEdit,
    QListWidget,
    QListWidgetItem,
    QMenu,
    QMenuBar,
    QProgressBar,
    QPushButton,
    QSplitter,
    QStackedWidget,
    QTableView,
    QVBoxLayout,
    QWidget,
)

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import hydra_core as core  # noqa: E402
import hydra_bt as bt  # noqa: E402
import hydra_tasks as task_lib  # noqa: E402
from hydra_sources import SourceLibrary  # noqa: E402
from hydra_source_commands import (  # noqa: E402
    SourceCommands,
    safe_source_filename,
)
from hydra_search import SearchController  # noqa: E402
from hydra_ui_details import DetailsPane  # noqa: E402
from hydra_ui_dialogs import AboutDialog, BtClientsDialog, SourceDialog  # noqa: E402
from hydra_ui_tasks import TaskRunner  # noqa: E402
from hydra_ui_theme import (  # noqa: E402
    APP_NAME,
    STATUS_RIGHT,
    apply_dark_titlebar,
    ask_confirm,
    setup_app,
    show_error,
    show_info,
)
from hydra_ui_widgets import GameTableModel, SourceRow  # noqa: E402

class MainWindow(QWidget):
    def __init__(self):
        super().__init__()
        self.setObjectName("root")
        self.setWindowTitle(f"{APP_NAME} {core.APP_VERSION}")
        self.setWindowIcon(QApplication.windowIcon())
        self.resize(1400, 840)
        self.setMinimumSize(1140, 700)
        self.setAttribute(Qt.WA_StyledBackground, True)

        self.sources_dir = os.path.join(os.path.dirname(os.path.abspath(__file__)), "sources")
        os.makedirs(self.sources_dir, exist_ok=True)
        self.commands = SourceCommands(self.sources_dir)

        self.library = SourceLibrary()
        self.selected_name = ""
        self.source_filter = ""
        self.search_ctl = SearchController()
        self.details_visible = True
        self.download_method = "cloudscraper" if core.CLOUDSCRAPER_AVAILABLE else "manual"
        self._preserve_source = ""

        self.debounce = QTimer(self)
        self.debounce.setSingleShot(True)
        self.debounce.setInterval(self.search_ctl.debounce_ms)
        self.debounce.timeout.connect(self.start_search)

        self.tasks = TaskRunner(self)
        self.tasks.progress.connect(self.on_task_progress)
        self.tasks.done.connect(self.on_task_done)
        self.tasks.failed.connect(self.on_task_failed)
        self.tasks.changed.connect(self.on_tasks_changed)

        self.model = GameTableModel()
        self.build_ui()
        self.build_menus()
        self.update_actions_state()
        self.start_loading("正在加载源…")

    # ---------------------------------------------------------------- UI
    def build_ui(self):
        root = QVBoxLayout(self)
        root.setContentsMargins(0, 0, 0, 0)
        root.setSpacing(0)

        self.menu_bar = QMenuBar(self)
        self.menu_bar.setNativeMenuBar(False)
        root.addWidget(self.menu_bar)

        content = QWidget()
        row = QHBoxLayout(content)
        row.setContentsMargins(0, 0, 0, 0)
        row.setSpacing(0)
        row.addWidget(self.build_sidebar())

        self.splitter = QSplitter(Qt.Horizontal)
        self.splitter.setHandleWidth(1)
        self.splitter.addWidget(self.build_main())
        self.splitter.addWidget(self.build_details())
        self.splitter.setStretchFactor(0, 1)
        self.splitter.setStretchFactor(1, 0)
        self.splitter.setSizes([800, 390])
        row.addWidget(self.splitter, 1)

        root.addWidget(content, 1)
        root.addWidget(self.build_statusbar())

    def build_sidebar(self):
        side = QFrame()
        side.setObjectName("sidebar")
        side.setFixedWidth(252)
        layout = QVBoxLayout(side)
        layout.setContentsMargins(16, 18, 14, 14)
        layout.setSpacing(10)

        brand = QHBoxLayout()
        brand.setSpacing(10)
        logo = QLabel("G")
        logo.setObjectName("logo")
        logo.setFixedSize(34, 34)
        logo.setAlignment(Qt.AlignCenter)
        brand.addWidget(logo)
        titles = QVBoxLayout()
        titles.setSpacing(1)
        title = QLabel(APP_NAME)
        title.setObjectName("appTitle")
        subtitle = QLabel(f"{core.APP_VERSION} · 本地游戏目录")
        subtitle.setObjectName("faint")
        titles.addWidget(title)
        titles.addWidget(subtitle)
        brand.addLayout(titles)
        brand.addStretch(1)
        layout.addLayout(brand)

        self.source_section = QLabel("源")
        self.source_section.setObjectName("section")
        layout.addWidget(self.source_section)

        self.source_list = QListWidget()
        self.source_list.setObjectName("sourceList")
        self.source_list.setSpacing(1)
        self.source_list.setHorizontalScrollBarPolicy(Qt.ScrollBarAlwaysOff)
        self.source_list.setSelectionMode(QAbstractItemView.SingleSelection)
        self.source_list.setContextMenuPolicy(Qt.CustomContextMenu)
        self.source_list.customContextMenuRequested.connect(self.on_source_context_menu)
        self.source_list.currentRowChanged.connect(self.on_source_changed)
        layout.addWidget(self.source_list, 1)

        self.source_info = QLabel("全部源")
        self.source_info.setObjectName("muted")
        self.source_info.setWordWrap(True)
        self.source_info.setMinimumHeight(30)
        layout.addWidget(self.source_info)

        self.side_stats = QLabel("正在加载…")
        self.side_stats.setObjectName("faint")
        self.side_stats.setWordWrap(True)
        layout.addWidget(self.side_stats)

        button_row = QHBoxLayout()
        button_row.setSpacing(6)
        self.add_button = QPushButton("添加源")
        self.add_button.setObjectName("accent")
        self.add_button.clicked.connect(lambda: self.open_source_dialog("add"))
        button_row.addWidget(self.add_button, 1)
        self.refresh_button = QPushButton("刷新")
        self.refresh_button.clicked.connect(self.refresh_all)
        button_row.addWidget(self.refresh_button)
        layout.addLayout(button_row)

        note = QLabel("缓存: sources/.cache")
        note.setObjectName("faint")
        layout.addWidget(note)
        return side

    def build_main(self):
        main = QWidget()
        layout = QVBoxLayout(main)
        layout.setContentsMargins(22, 18, 22, 10)
        layout.setSpacing(10)

        self.search = QLineEdit()
        self.search.setObjectName("search")
        self.search.setPlaceholderText("搜索游戏名称…")
        self.search.setClearButtonEnabled(True)
        self.search.setMinimumHeight(46)
        self.search.textChanged.connect(self.on_query_changed)
        self.search.returnPressed.connect(self.start_search)
        palette = self.search.palette()
        palette.setColor(QPalette.PlaceholderText, QColor("#626b78"))
        self.search.setPalette(palette)
        layout.addWidget(self.search)

        options = QHBoxLayout()
        options.setSpacing(10)
        self.fuzzy_btn = QPushButton("模糊匹配")
        self.fuzzy_btn.setObjectName("toggle")
        self.fuzzy_btn.setCheckable(True)
        self.fuzzy_btn.setChecked(True)
        self.fuzzy_btn.toggled.connect(lambda _: self.start_search())
        options.addWidget(self.fuzzy_btn)

        threshold_label = QLabel("阈值")
        threshold_label.setObjectName("muted")
        options.addWidget(threshold_label)
        self.threshold = QDoubleSpinBox()
        self.threshold.setRange(0.1, 1.0)
        self.threshold.setSingleStep(0.1)
        self.threshold.setDecimals(1)
        self.threshold.setValue(0.3)
        self.threshold.valueChanged.connect(lambda _: self.start_search())
        options.addWidget(self.threshold)

        self.scope_label = QLabel("范围：全部源")
        self.scope_label.setObjectName("muted")
        options.addWidget(self.scope_label)
        options.addStretch(1)

        self.result_meta = QLabel("")
        self.result_meta.setObjectName("muted")
        options.addWidget(self.result_meta)

        self.details_btn = QPushButton("收起详情")
        self.details_btn.setObjectName("toggle")
        self.details_btn.setCheckable(True)
        self.details_btn.setChecked(True)
        self.details_btn.setToolTip("显示/隐藏游戏详情面板 (Ctrl+D)")
        self.details_btn.toggled.connect(self.toggle_details)
        options.addWidget(self.details_btn)
        layout.addLayout(options)

        self.progress = QProgressBar()
        self.progress.setObjectName("lineBar")
        self.progress.setRange(0, 0)
        self.progress.setTextVisible(False)
        self.progress.setFixedHeight(3)
        self.progress.hide()
        layout.addWidget(self.progress)

        header = QHBoxLayout()
        section = QLabel("搜索结果")
        section.setObjectName("section")
        header.addWidget(section)
        header.addStretch(1)
        self.count_label = QLabel("")
        self.count_label.setObjectName("muted")
        header.addWidget(self.count_label)
        layout.addLayout(header)

        self.table = QTableView()
        self.table.setModel(self.model)
        self.table.setSelectionBehavior(QAbstractItemView.SelectRows)
        self.table.setSelectionMode(QAbstractItemView.SingleSelection)
        self.table.setEditTriggers(QAbstractItemView.NoEditTriggers)
        self.table.setShowGrid(False)
        self.table.setAlternatingRowColors(True)
        self.table.setWordWrap(False)
        self.table.setTextElideMode(Qt.ElideRight)
        self.table.verticalHeader().setVisible(False)
        self.table.verticalHeader().setDefaultSectionSize(38)
        self.table.horizontalHeader().setHighlightSections(False)
        self.table.horizontalHeader().setStretchLastSection(False)
        self.table.horizontalHeader().setSectionResizeMode(0, QHeaderView.Stretch)
        for column in (1, 2, 3, 4):
            self.table.horizontalHeader().setSectionResizeMode(column, QHeaderView.Fixed)
        self.table.setColumnWidth(1, 150)
        self.table.setColumnWidth(2, 100)
        self.table.setColumnWidth(3, 110)
        self.table.setColumnWidth(4, 70)
        self.table.selectionModel().selectionChanged.connect(self.on_table_selection)
        self.table.horizontalHeader().sectionClicked.connect(self.on_header_clicked)
        self.table.setContextMenuPolicy(Qt.CustomContextMenu)
        self.table.customContextMenuRequested.connect(self.on_table_context_menu)

        self.empty_page = self.build_empty_page()
        self.stack = QStackedWidget()
        self.stack.addWidget(self.empty_page)
        self.stack.addWidget(self.table)
        layout.addWidget(self.stack, 1)
        return main

    def build_empty_page(self):
        page = QWidget()
        layout = QVBoxLayout(page)
        layout.setAlignment(Qt.AlignCenter)
        layout.setSpacing(8)
        layout.addStretch(1)

        self.empty_icon = QLabel("⌕")
        self.empty_icon.setObjectName("emptyIcon")
        self.empty_icon.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.empty_icon)

        self.empty_title = QLabel("正在加载源…")
        self.empty_title.setObjectName("emptyTitle")
        self.empty_title.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.empty_title)

        self.empty_hint = QLabel("从 sources/.cache 读取游戏数据")
        self.empty_hint.setObjectName("muted")
        self.empty_hint.setAlignment(Qt.AlignCenter)
        layout.addWidget(self.empty_hint)

        layout.addStretch(1)
        return page

    def build_details(self):
        self.details = DetailsPane()
        self.details.copy_requested.connect(self.copy_text)
        self.details.bt_requested.connect(self.open_bt_download_menu)
        self.details.collapse_requested.connect(
            lambda: self.act_details.setChecked(False)
        )
        return self.details

    def build_statusbar(self):
        bar = QFrame()
        bar.setObjectName("statusBar")
        bar.setFixedHeight(34)
        layout = QHBoxLayout(bar)
        layout.setContentsMargins(18, 0, 18, 0)
        self.status_left = QLabel("正在加载源…")
        self.status_left.setObjectName("muted")
        self.status_right = QLabel(STATUS_RIGHT)
        self.status_right.setObjectName("faint")
        layout.addWidget(self.status_left, 1)
        layout.addWidget(self.status_right)
        return bar

    def build_menus(self):
        source_menu = self.menu_bar.addMenu("源")

        self.act_add = QAction("通过 URL 添加源…", self)
        self.act_add.setShortcut(QKeySequence("Ctrl+U"))
        self.act_add.triggered.connect(lambda: self.open_source_dialog("add"))
        source_menu.addAction(self.act_add)

        self.act_update = QAction("更新选中源…", self)
        self.act_update.triggered.connect(self.update_selected_source)
        source_menu.addAction(self.act_update)

        self.act_import = QAction("从本地导入 JSON…", self)
        self.act_import.setShortcut(QKeySequence("Ctrl+I"))
        self.act_import.triggered.connect(self.import_sources)
        source_menu.addAction(self.act_import)

        self.act_refresh = QAction("刷新全部源", self)
        self.act_refresh.setShortcut(QKeySequence("F5"))
        self.act_refresh.triggered.connect(self.refresh_all)
        source_menu.addAction(self.act_refresh)

        self.act_delete = QAction("删除选中源", self)
        self.act_delete.triggered.connect(self.delete_selected_source)
        source_menu.addAction(self.act_delete)

        self.act_open_folder = QAction("打开源文件夹", self)
        self.act_open_folder.triggered.connect(self.open_sources_folder)
        source_menu.addAction(self.act_open_folder)

        source_menu.addSeparator()
        method_menu = source_menu.addMenu("下载方式")
        self.method_group = QActionGroup(self)
        self.method_group.setExclusive(True)
        method_options = [
            ("cloudscraper", "cloudscraper（推荐）"),
            ("curl", "系统 curl"),
            ("urllib", "Python urllib"),
            ("manual", "手动导入"),
        ]
        for key, label in method_options:
            action = QAction(label, self)
            action.setCheckable(True)
            action.setData(key)
            if key == "cloudscraper" and not core.CLOUDSCRAPER_AVAILABLE:
                action.setEnabled(False)
                action.setText("cloudscraper（未安装）")
            action.setChecked(key == self.download_method)
            action.triggered.connect(lambda checked=False, k=key: self.set_download_method(k))
            self.method_group.addAction(action)
            method_menu.addAction(action)

        source_menu.addSeparator()
        quit_action = QAction("退出", self)
        quit_action.setShortcut(QKeySequence.StandardKey.Quit)
        quit_action.triggered.connect(self.close)
        source_menu.addAction(quit_action)

        view_menu = self.menu_bar.addMenu("视图")
        self.act_details = QAction("显示详情面板", self)
        self.act_details.setCheckable(True)
        self.act_details.setChecked(True)
        self.act_details.setShortcut(QKeySequence("Ctrl+D"))
        self.act_details.toggled.connect(self.toggle_details)
        view_menu.addAction(self.act_details)

        reset_action = QAction("重置布局", self)
        reset_action.triggered.connect(self.reset_layout)
        view_menu.addAction(reset_action)

        focus_action = QAction("聚焦搜索框", self)
        focus_action.setShortcut(QKeySequence("Ctrl+F"))
        focus_action.triggered.connect(self.focus_search)
        view_menu.addAction(focus_action)

        tools_menu = self.menu_bar.addMenu("工具")
        bt_action = QAction("常见 BT 客户端…", self)
        bt_action.triggered.connect(self.show_bt_clients)
        tools_menu.addAction(bt_action)

        help_menu = self.menu_bar.addMenu("帮助")
        about_action = QAction("关于", self)
        about_action.triggered.connect(self.show_about)
        help_menu.addAction(about_action)

        self.search.setFocus()

    @property
    def items(self):
        return self.library.items

    @property
    def metas(self):
        return self.library.metas

    @property
    def total_groups(self):
        return self.library.total_groups

    # ------------------------------------------------------------ helpers
    def ready_status(self):
        if not self.metas:
            return "就绪"
        return f"就绪 · {len(self.metas)} 个源 · {len(self.items):,} 个游戏"

    def set_download_method(self, key):
        self.download_method = key
        for action in self.method_group.actions():
            action.setChecked(action.data() == key)

    def guard_busy(self):
        if self.tasks.busy():
            show_info(self, "请稍候", "当前有任务正在执行，请等待完成。")
            return False
        return True

    def submit_task(self, kind, task, text, total=None):
        self.status_left.setText(text)
        self.progress.show()
        if total:
            self.progress.setRange(0, total)
            self.progress.setValue(0)
        else:
            self.progress.setRange(0, 0)
        self.tasks.submit(kind, task)
        self.update_actions_state()

    def on_tasks_changed(self):
        self.update_progress()
        self.update_actions_state()

    def on_task_progress(self, kind, event, payload):
        if kind == "load":
            if event == "status":
                self.progress.setRange(0, payload["total"])
                self.progress.setValue(payload["index"] - 1)
                self.status_left.setText(
                    f"加载源 ({payload['index']}/{payload['total']}): {payload['name']}"
                )
            elif event == "source":
                name, source_items, meta = payload
                self.on_source_loaded(name, source_items, meta)
        elif kind == "refresh" and event == "progress":
            self.progress.setRange(0, payload["total"])
            self.progress.setValue(payload["index"])
            self.status_left.setText(
                f"刷新源 ({payload['index']}/{payload['total']}): {payload['name']}"
            )

    def on_task_done(self, kind, result):
        if kind == "load":
            self.on_loaded(*result)
        elif kind == "search":
            self.on_search_done(*result)
        elif kind == "fetch":
            self.on_url_fetched(result)
        elif kind == "add":
            self.on_source_added(result)
        elif kind == "update":
            self.on_updated(result)
        elif kind == "delete":
            self.on_deleted(result)
        elif kind == "import":
            self.on_imported(result)
        elif kind == "refresh":
            self.on_refreshed(result)

    def on_task_failed(self, kind, error):
        titles = {
            "load": "加载失败",
            "refresh": "刷新失败",
            "fetch": "下载失败",
            "add": "保存失败",
            "update": "更新失败",
            "delete": "删除失败",
            "import": "导入失败",
            "search": "搜索失败",
        }
        self.status_left.setText(self.ready_status())
        show_error(self, titles.get(kind, "操作失败"), error)

    def update_progress(self):
        if self.tasks.busy():
            self.progress.show()
            return
        if self.tasks.active("search"):
            self.progress.setRange(0, 0)
            self.progress.show()
        else:
            self.progress.hide()

    def update_actions_state(self):
        meta = self.metas.get(self.selected_name) if self.selected_name else None
        busy = self.tasks.busy()
        self.act_add.setEnabled(not busy)
        self.act_import.setEnabled(not busy)
        self.act_update.setEnabled(not busy and bool(meta and meta.get("url")))
        self.act_delete.setEnabled(not busy and bool(meta))
        self.act_refresh.setEnabled(not busy and any(m.get("url") for m in self.metas.values()))
        self.add_button.setEnabled(not busy)
        self.refresh_button.setEnabled(not busy and any(m.get("url") for m in self.metas.values()))

    def flash(self, message):
        self.status_right.setText(message)
        QTimer.singleShot(1800, lambda: self.status_right.setText(STATUS_RIGHT))

    def copy_text(self, text):
        if not text:
            return
        QGuiApplication.clipboard().setText(text)
        count = text.count("\n") + 1
        self.flash(f"已复制 {count} 条链接" if "\n" in text else "已复制到剪贴板")

    # ------------------------------------------------------------- loading
    def start_loading(self, text="正在加载源…"):
        if self.tasks.active("load"):
            return
        self.search_ctl.invalidate()
        self._preserve_source = self.selected_name

        self.library.reset()
        self.model.set_groups([])
        self.result_data_clear()
        self.count_label.setText("")
        self.result_meta.setText("")
        self.render_sources_reset()
        self.side_stats.setText("正在加载…")

        self.progress.setRange(0, 1)
        self.progress.setValue(0)
        self.progress.show()
        self.status_left.setText(text)
        self.empty_title.setText("正在加载源…")
        self.empty_hint.setText("从 sources/.cache 读取游戏数据")
        self.stack.setCurrentWidget(self.empty_page)

        self.tasks.submit(
            "load",
            lambda report: task_lib.load_sources(self.sources_dir, report),
        )
        self.update_actions_state()

    def on_source_loaded(self, name, source_items, meta):
        delta = self.library.add_source(name, source_items, meta)
        self.render_source(delta)
        self.side_stats.setText(
            f"{delta.items_count:,} 个游戏 · 去重后 {delta.total_groups:,} 组"
        )
        if self.tasks.active("load") and self.progress.maximum() > 0:
            self.progress.setValue(min(delta.sources_count, self.progress.maximum()))
        self.empty_hint.setText(
            f"输入关键词，从 {delta.items_count:,} 个游戏中查找"
        )
        if self.search.text().strip():
            self.debounce.start()

    def on_loaded(self, errors):
        errors = errors or []
        self.update_source_section()
        self.side_stats.setText(
            f"{len(self.items):,} 个游戏 · 去重后 {self.total_groups:,} 组"
        )
        self.empty_title.setText("搜索游戏")
        self.empty_hint.setText(f"输入关键词，从 {len(self.items):,} 个游戏中查找")

        if self.search.text().strip():
            self.start_search()
        else:
            self.stack.setCurrentWidget(self.empty_page)
            self.progress.hide()
            self.progress.setRange(0, 0)

        self.status_left.setText(self.ready_status())
        if errors:
            self.status_left.setText(
                f"{self.ready_status()} · {len(errors)} 个源加载失败"
            )
            if not self.metas:
                show_error(self, "加载源失败", "\n".join(errors[:20]))
        self.update_actions_state()

    def result_data_clear(self):
        self.details.clear()

    def render_sources_reset(self):
        self.source_list.blockSignals(True)
        self.source_list.clear()
        total_row = QListWidgetItem()
        total_row.setSizeHint(QSize(0, 36))
        total_row.setData(Qt.UserRole, "")
        self.source_list.addItem(total_row)
        self.source_list.setItemWidget(total_row, SourceRow("全部源", 0))
        self.source_list.setCurrentRow(0)
        self.source_list.blockSignals(False)

        self.selected_name = ""
        self.source_filter = ""
        self.scope_label.setText("范围：全部源")
        self.update_source_section()
        self.update_source_info()
        self.update_actions_state()

    def render_source(self, delta):
        meta = delta.meta
        total_row = self.source_list.item(0)

        self.source_list.blockSignals(True)
        self.source_list.setItemWidget(total_row, SourceRow("全部源", delta.items_count))
        row = self.source_row_by_name(delta.name)
        if row < 0:
            row = self.insert_row_for(meta["display"])
            item = QListWidgetItem()
            item.setSizeHint(QSize(0, 36))
            item.setData(Qt.UserRole, delta.name)
            self.source_list.insertItem(row, item)
        else:
            item = self.source_list.item(row)
        item.setToolTip(self.source_tooltip(delta.name, meta))
        self.source_list.setItemWidget(
            item, SourceRow(meta["display"], meta["count"], not meta.get("url"))
        )

        target = self.selected_name or self._preserve_source
        if target:
            target_row = self.source_row_by_name(target)
            if target_row >= 0:
                self.source_list.setCurrentRow(target_row)
                target_meta = self.library.source_meta(target)
                self.selected_name = target
                self.source_filter = target_meta["display"] if target_meta else ""
                self.scope_label.setText(
                    "范围：全部源"
                    if not target_meta
                    else f"范围：{target_meta['display']}"
                )
                self._preserve_source = ""
        self.source_list.blockSignals(False)

        self.update_source_section()
        self.update_source_info()
        self.update_actions_state()

    def source_row_by_name(self, name):
        for row in range(self.source_list.count()):
            item = self.source_list.item(row)
            if (item.data(Qt.UserRole) or "") == name:
                return row
        return -1

    def insert_row_for(self, display):
        for row in range(1, self.source_list.count()):
            meta = self.library.source_meta(self.source_list.item(row).data(Qt.UserRole))
            if meta and meta["display"] > display:
                return row
        return self.source_list.count()

    def source_tooltip(self, name, meta):
        tooltip = f"{name}\n{meta['path']}\n最后更新: {self.source_updated_text(meta)}"
        if meta.get("url"):
            tooltip += f"\n{meta['url']}"
        return tooltip

    def update_source_section(self):
        self.source_section.setText(f"源 · {len(self.metas)}")
        stamps = [m.get("updated") for m in self.metas.values() if m.get("updated")]
        latest = max(stamps) if stamps else None
        latest_text = (
            time.strftime("%Y-%m-%d %H:%M", time.localtime(latest)) if latest else "未知"
        )
        self.source_section.setToolTip(
            f"{len(self.metas)} 个源 · 最近更新 {latest_text}"
        )

    def on_source_changed(self, row):
        if row < 0:
            return
        item = self.source_list.item(row)
        if item is None:
            return
        self.selected_name = item.data(Qt.UserRole) or ""
        self._preserve_source = self.selected_name
        meta = self.metas.get(self.selected_name) if self.selected_name else None
        self.source_filter = meta["display"] if meta else ""
        self.scope_label.setText(
            "范围：全部源" if not meta else f"范围：{meta['display']}"
        )
        self.update_source_info()
        self.update_actions_state()
        if self.search.text().strip():
            self.start_search()

    def source_updated_text(self, meta):
        updated = meta.get("updated") if meta else None
        if not updated:
            return "未知"
        return time.strftime("%Y-%m-%d %H:%M", time.localtime(updated))

    def update_source_info(self):
        if not self.selected_name:
            self.source_info.setText("全部源")
            self.source_info.setToolTip(self.sources_dir)
            return
        meta = self.metas.get(self.selected_name)
        if not meta:
            self.source_info.setText("全部源")
            return
        stamp = self.source_updated_text(meta)
        prefix = "" if meta.get("url") else "本地源 · "
        self.source_info.setText(
            f"{meta['display']} · {meta['count']:,} 个游戏\n{prefix}最后更新: {stamp}"
        )
        tooltip = meta["path"]
        if meta.get("url"):
            tooltip += f"\n{meta['url']}"
        tooltip += f"\n最后更新: {stamp}"
        if meta.get("detected_key"):
            tooltip += f"\n检测字段: {meta['detected_key']}"
        if meta.get("invalid_count"):
            tooltip += f"\n跳过条目: {meta['invalid_count']}"
        self.source_info.setToolTip(tooltip)

    # -------------------------------------------------------- source tasks
    def open_source_dialog(self, mode):
        if not self.guard_busy():
            return
        if mode == "update":
            meta = self.metas.get(self.selected_name) if self.selected_name else None
            if not meta or not meta.get("url"):
                show_info(self, "提示", "请先在左侧选择一个记录了网址的源。")
                return
            dialog = SourceDialog(
                self, "update", meta["url"], self.download_method, meta["display"]
            )
            if dialog.exec() != QDialog.DialogCode.Accepted:
                return
            self.set_download_method(dialog.method)
            self._start_update(dialog.url, self.selected_name, meta)
        else:
            dialog = SourceDialog(self, "add", "", self.download_method)
            if dialog.exec() != QDialog.DialogCode.Accepted:
                return
            self.set_download_method(dialog.method)
            self._start_fetch(dialog.url)

    def on_source_context_menu(self, pos):
        item = self.source_list.itemAt(pos)
        if item is None:
            return
        self.source_list.setCurrentRow(self.source_list.row(item))
        meta = self.metas.get(self.selected_name) if self.selected_name else None
        menu = QMenu(self)
        update_action = menu.addAction("更新此源…")
        update_action.setEnabled(bool(meta and meta.get("url")))
        update_action.triggered.connect(self.update_selected_source)
        delete_action = menu.addAction("删除源")
        delete_action.setEnabled(bool(meta))
        delete_action.triggered.connect(self.delete_selected_source)
        copy_path_action = menu.addAction("复制源路径")
        copy_path_action.setEnabled(bool(meta))
        copy_path_action.triggered.connect(self.copy_source_path)
        menu.addSeparator()
        menu.addAction("打开源文件夹", self.open_sources_folder)
        menu.addAction("从本地导入 JSON…", self.import_sources)
        menu.exec(self.source_list.viewport().mapToGlobal(pos))

    def update_selected_source(self):
        self.open_source_dialog("update")

    def _start_fetch(self, url):
        self.submit_task(
            "fetch",
            lambda report: self.commands.fetch(self.download_method, url),
            "正在下载源…",
        )

    def on_url_fetched(self, fetched):
        dialog = QInputDialog(self)
        dialog.setWindowTitle("保存源")
        dialog.setLabelText("保存文件名（不含扩展名）:")
        dialog.setTextValue(fetched.default_name)
        dialog.setOkButtonText("保存")
        dialog.setCancelButtonText("取消")
        prepare_dialog(dialog)
        if dialog.exec() != QDialog.DialogCode.Accepted:
            self.status_left.setText(self.ready_status())
            return
        filename = safe_source_filename(dialog.textValue())
        dest = os.path.join(self.sources_dir, filename)
        overwrite = False
        if os.path.exists(dest):
            if not ask_confirm(
                self, "覆盖确认", f"源文件 {filename} 已存在，是否覆盖？"
            ):
                self.status_left.setText(self.ready_status())
                return
            overwrite = True
        self.submit_task(
            "add",
            lambda report: self.commands.add_fetched(
                fetched, filename, overwrite
            ),
            f"正在保存 {filename}（{fetched.count} 个游戏）…",
        )

    def on_source_added(self, change):
        self.flash(f"已保存 {change.name}，正在重新加载…")
        self.finish_source_change()

    def _start_update(self, url, name, meta):
        self.submit_task(
            "update",
            lambda report: self.commands.update(self.download_method, name, url),
            f"正在更新 {meta['display']}…",
        )

    def on_updated(self, change):
        show_info(
            self, "更新成功",
            f"已更新: {change.name}\n当前共 {change.count} 个游戏",
        )
        self.finish_source_change()

    def on_deleted(self, change):
        self.selected_name = ""
        self._preserve_source = ""
        self.flash(f"已删除 {change.name}，正在重新加载…")
        self.finish_source_change()

    def on_imported(self, result):
        if result.added:
            if result.errors:
                show_info(
                    self, "导入完成",
                    f"成功导入 {len(result.added)} 个源，"
                    f"失败 {len(result.errors)} 个。\n\n"
                    + "\n".join(result.errors[:10]),
                )
            else:
                self.flash(f"已导入 {len(result.added)} 个源，正在重新加载…")
            self.finish_source_change()
        elif result.errors:
            self.status_left.setText(self.ready_status())
            show_error(self, "导入失败", "\n".join(result.errors[:20]))
        else:
            self.status_left.setText(self.ready_status())

    def on_refreshed(self, result):
        lines = [
            f"成功更新 {len(result.updated)} 个源，失败 {len(result.failed)} 个"
        ]
        lines.extend(f"{name}: {error}" for name, error in result.failed[:5])
        show_info(self, "刷新完成", "\n".join(lines))
        self.finish_source_change()

    def refresh_all(self):
        if not self.guard_busy():
            return
        count = sum(1 for meta in self.metas.values() if meta.get("url"))
        if not count:
            show_info(self, "提示", "没有记录了网址的源可以刷新。")
            return
        if self.download_method == "manual":
            show_info(
                self, "提示",
                "当前为手动导入模式。\n请先在「源 → 下载方式」中选择可用方式，或手动导入 JSON。",
            )
            return

        def task(report):
            return self.commands.refresh(
                self.download_method,
                report=lambda index, total, name: report(
                    "progress", {"index": index, "total": total, "name": name}
                ),
            )

        self.submit_task("refresh", task, "正在刷新全部源…", total=count)

    def finish_source_change(self):
        self.reload_sources()

    def reload_sources(self):
        self.start_loading("正在重新加载源…")

    def import_sources(self):
        if not self.guard_busy():
            return
        files, _ = QFileDialog.getOpenFileNames(
            self,
            "选择游戏目录 JSON 文件",
            "",
            "JSON 文件 (*.json);;所有文件 (*)",
        )
        if not files:
            return

        approved = []
        overwrite = set()
        for source_file in files:
            filename = safe_source_filename(os.path.basename(source_file))
            dest = os.path.join(self.sources_dir, filename)
            if os.path.exists(dest):
                if not ask_confirm(
                    self, "覆盖确认", f"源文件 {filename} 已存在，是否覆盖？"
                ):
                    continue
                overwrite.add(filename)
            approved.append(source_file)

        if not approved:
            return
        self.submit_task(
            "import",
            lambda report: self.commands.import_files(
                tuple(approved), tuple(sorted(overwrite))
            ),
            f"正在导入 {len(approved)} 个源…",
        )

    def delete_selected_source(self):
        if not self.guard_busy():
            return
        name = self.selected_name
        meta = self.metas.get(name) if name else None
        if not meta:
            show_info(self, "提示", "请先选择一个源。")
            return
        if not ask_confirm(
            self, "确认删除",
            f"确定要删除源「{meta['display']}」吗？\n本地 JSON 与缓存都会被删除。",
            "删除", "取消",
        ):
            return
        self.submit_task(
            "delete",
            lambda report: self.commands.delete(name),
            f"正在删除 {meta['display']}…",
        )

    def copy_source_path(self):
        meta = self.metas.get(self.selected_name) if self.selected_name else None
        if not meta:
            return
        self.copy_text(meta["path"])

    def open_sources_folder(self):
        QDesktopServices.openUrl(QUrl.fromLocalFile(self.sources_dir))

    def show_about(self):
        thunder = next(
            (target for target in bt.detect() if target["key"] == "thunder"), None
        )
        dialog = AboutDialog(
            self,
            len(self.items),
            len(self.metas),
            self.sources_dir,
            thunder["exe"] if thunder else None,
        )
        dialog.exec()

    def show_bt_clients(self):
        dialog = BtClientsDialog(self)
        dialog.exec()
        self.details.refresh()

    # -------------------------------------------------------------- search
    def on_query_changed(self, text):
        if not self.search_ctl.observe(text):
            self.debounce.stop()
            self.model.set_groups([])
            self.result_data_clear()
            self.count_label.setText("")
            self.result_meta.setText("")
            self.stack.setCurrentWidget(self.empty_page)
            self.empty_title.setText("搜索游戏")
            self.empty_hint.setText(f"输入关键词，从 {len(self.items):,} 个游戏中查找")
            self.update_progress()
            return
        self.count_label.setText("搜索中…")
        self.debounce.start()

    def start_search(self):
        query = self.search.text().strip()
        if not query or not self.items:
            return
        self.debounce.stop()
        ticket = self.search_ctl.begin(
            query,
            self.source_filter,
            self.fuzzy_btn.isChecked(),
            self.threshold.value(),
        )
        self.count_label.setText("搜索中…")
        self.result_meta.setText("")
        self.update_progress()
        self.tasks.submit(
            "search",
            lambda report: (
                ticket,
                *task_lib.search_sources(
                    self.items,
                    ticket.query,
                    ticket.fuzzy,
                    ticket.threshold,
                    ticket.source_filter,
                ),
            ),
        )

    def on_search_done(self, ticket, groups, total):
        if not self.search_ctl.accepts(ticket.token):
            return
        elapsed = self.search_ctl.elapsed_ms(ticket)
        groups = self.search_ctl.apply_sort(groups)
        self.update_progress()
        self.model.set_groups(groups)
        if groups:
            self.stack.setCurrentWidget(self.table)
            self.table.selectRow(0)
            self.count_label.setText(f"找到 {total:,} 组")
            if total > len(groups):
                self.count_label.setText(f"找到 {total:,} 组 · 显示前 {len(groups):,} 组")
        else:
            self.stack.setCurrentWidget(self.empty_page)
            self.empty_title.setText("没有找到结果")
            self.empty_hint.setText(
                f"没有匹配「{ticket.query}」的游戏，试试更短的关键词或切换来源"
            )
            self.count_label.setText("0 组")
            self.details.clear()
        self.result_meta.setText(f"{elapsed / 1000:.2f} 秒")

    def on_table_selection(self, *args):
        index = self.table.currentIndex()
        if not index.isValid():
            self.details.clear()
            return
        group = self.model.group_at(index.row())
        if group:
            self.show_group(group)

    def show_group(self, group):
        self.details.show_group(group, bt.detect())

    def on_header_clicked(self, section):
        if section != 3 or not self.model.groups:
            return
        desc = self.search_ctl.toggle_date_sort()
        current = self.details.current_group
        self.model.set_groups(self.search_ctl.apply_sort(self.model.groups))
        header = self.table.horizontalHeader()
        header.setSortIndicatorShown(True)
        header.setSortIndicator(
            3, Qt.DescendingOrder if desc else Qt.AscendingOrder
        )
        if current is not None:
            row = self.model.row_for_group(current)
            if row >= 0:
                self.table.selectRow(row)
        self.result_meta.setText("按日期排序")

    # ------------------------------------------------------------- details
    def build_bt_menu(self, uri=None):
        if not uri:
            group = self.details.current_group or self.selected_group()
            if not group:
                return None
            magnets = [link for link in group["uris"] if link.startswith("magnet")]
            if not magnets:
                show_info(self, "提示", "该游戏没有 Magnet 链接，无法发送到 BT 工具。")
                return None
            uri = magnets[0]

        targets = bt.detect()
        client_targets = [target for target in targets if target["kind"] == "client"]
        system_target = next(
            (target for target in targets if target["kind"] == "system"), None
        )

        menu = QMenu(self)
        for target in client_targets:
            action = menu.addAction(target["name"])
            action.triggered.connect(
                lambda checked=False, t=target, u=uri: self.execute_handoff(
                    bt.plan(u, t)
                )
            )

        if system_target:
            if client_targets:
                menu.addSeparator()
            action = menu.addAction(
                f"系统默认磁力程序（{system_target['name']}）"
            )
            action.triggered.connect(
                lambda checked=False, t=system_target, u=uri: self.execute_handoff(
                    bt.plan(u, t)
                )
            )

        if not client_targets and not system_target:
            action = menu.addAction("未检测到本机 BT 客户端")
            action.setEnabled(False)

        if client_targets or system_target:
            menu.addSeparator()
        menu.addAction(
            "查看常见 BT 客户端…", lambda checked=False: self.show_bt_clients()
        )
        return menu

    def open_bt_download_menu(self, uri=None):
        menu = self.build_bt_menu(uri)
        if menu is None:
            return
        menu.exec(self.cursor().pos())

    def execute_handoff(self, plan):
        if not plan:
            return
        uri = plan.get("clipboard", "")
        target = plan.get("target", {})
        name = target.get("name", "BT 客户端")
        if uri:
            QGuiApplication.clipboard().setText(uri)

        if plan.get("kind") == "shell":
            try:
                os.startfile(uri)
                self.flash("已交给系统默认 BT 程序（磁力已复制）")
            except Exception as exc:
                show_error(
                    self, "启动失败",
                    f"无法打开磁力链接:\n{exc}\n\n磁力已复制到剪贴板。",
                )
            return

        exe = plan.get("exe", "")
        if not exe or not os.path.exists(exe):
            fallback = plan.get("fallback")
            if fallback:
                try:
                    os.startfile(fallback.get("clipboard", uri))
                    self.flash("已复制磁力链接并启动默认程序")
                except Exception:
                    show_info(
                        self, "提示",
                        f"磁力链接已复制到剪贴板:\n{uri[:80]}...\n\n"
                        "请手动粘贴到下载工具。",
                    )
            else:
                show_error(
                    self, "启动失败",
                    f"找不到可执行文件:\n{exe}\n\n磁力已复制到剪贴板。",
                )
            return

        try:
            subprocess.Popen([exe] + list(plan.get("args", [])), shell=False)
            self.flash(f"已发送到 {name}（磁力已复制）")
        except Exception as exc:
            show_error(
                self, "启动失败",
                f"无法启动 {name}:\n{exc}\n\n磁力已复制到剪贴板。",
            )

    # ------------------------------------------------------- table actions
    def selected_group(self):
        index = self.table.currentIndex()
        if not index.isValid():
            return None
        return self.model.group_at(index.row())

    def on_table_context_menu(self, pos):
        index = self.table.indexAt(pos)
        if not index.isValid():
            return
        self.table.selectRow(index.row())
        group = self.model.group_at(index.row())
        if not group:
            return

        menu = QMenu(self)
        menu.addAction("在浏览器打开", lambda: self.open_in_browser(group))
        if any(target["key"] == "thunder" for target in bt.detect()):
            menu.addAction("使用迅雷下载", lambda: self.download_with_thunder(group))
        menu.addSeparator()
        menu.addAction("复制源 URL", lambda: self.copy_source_url(group))
        menu.exec(self.table.viewport().mapToGlobal(pos))

    def open_in_browser(self, group):
        for uri in group["uris"]:
            if uri.startswith("http"):
                QDesktopServices.openUrl(QUrl(uri))
                return
        show_info(self, "提示", "该游戏没有 HTTP 链接")

    def copy_source_url(self, group):
        urls = []
        for item in group["items"]:
            url = item.get("url")
            if url and url not in urls:
                urls.append(url)
        if not urls:
            show_info(self, "提示", "该源没有保存网址信息")
            return
        self.copy_text("\n".join(urls))

    def download_with_thunder(self, group):
        magnet = next((uri for uri in group["uris"] if uri.startswith("magnet")), None)
        if not magnet:
            show_info(self, "提示", "该游戏没有 Magnet 链接")
            return
        thunder = next(
            (target for target in bt.detect() if target["key"] == "thunder"), None
        )
        if thunder:
            self.execute_handoff(bt.plan(magnet, thunder))
            return
        QGuiApplication.clipboard().setText(magnet)
        try:
            os.startfile(magnet)
            self.flash("已尝试用默认程序打开磁力链接")
        except Exception:
            show_info(
                self, "提示",
                f"磁力链接已复制到剪贴板:\n{magnet[:80]}...\n\n"
                "请手动粘贴到下载工具。",
            )

    # ------------------------------------------------------------- layout
    def toggle_details(self, visible):
        self.details_visible = visible
        self.details.setVisible(visible)
        if self.act_details.isChecked() != visible:
            self.act_details.setChecked(visible)
        if self.details_btn.isChecked() != visible:
            self.details_btn.setChecked(visible)
        self.details_btn.setText("收起详情" if visible else "展开详情")
        if visible:
            self.splitter.setSizes([800, 390])

    def reset_layout(self):
        self.toggle_details(True)
        self.splitter.setSizes([800, 390])

    def focus_search(self):
        self.search.setFocus()
        self.search.selectAll()

    def closeEvent(self, event):
        self.debounce.stop()
        self.tasks.shutdown()
        super().closeEvent(event)


def main():
    try:
        QGuiApplication.setHighDpiScaleFactorRoundingPolicy(
            Qt.HighDpiScaleFactorRoundingPolicy.PassThrough
        )
    except Exception:
        pass
    app = QApplication(sys.argv)
    setup_app(app)
    window = MainWindow()
    window.show()
    apply_dark_titlebar(window)
    sys.exit(app.exec())


if __name__ == "__main__":
    main()
