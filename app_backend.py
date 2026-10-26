"""Backend exposed to the QML UI: models, source tasks and detail data."""

import json
import os
import subprocess
import time

from PySide6.QtCore import (
    QAbstractListModel,
    QModelIndex,
    QObject,
    Property,
    QTimer,
    Qt,
    QUrl,
    Signal,
    Slot,
)
from PySide6.QtGui import QDesktopServices, QGuiApplication

import bt_clients as bt
import catalog as core
import catalog_tasks as task_lib
import detail_model as details_lib
import search_controller as search_lib
import source_commands as commands_lib
import source_library as sources_lib
import swarm_stats as swarm
import update_check as updates
from task_runner import TaskRunner

UPDATE_REPO = "lzoreal/Game"


class ResultsModel(QAbstractListModel):
    TitleRole = Qt.UserRole + 1
    SourcesRole = Qt.UserRole + 2
    SizeRole = Qt.UserRole + 3
    DateRole = Qt.UserRole + 4
    LinkCountRole = Qt.UserRole + 5

    def __init__(self, parent=None):
        super().__init__(parent)
        self._groups = []

    def roleNames(self):
        return {
            self.TitleRole: b"title",
            self.SourcesRole: b"sources",
            self.SizeRole: b"size",
            self.DateRole: b"date",
            self.LinkCountRole: b"linkCount",
        }

    def rowCount(self, parent=QModelIndex()):
        return 0 if parent.isValid() else len(self._groups)

    def data(self, index, role=Qt.DisplayRole):
        if not index.isValid() or not (0 <= index.row() < len(self._groups)):
            return None
        group = self._groups[index.row()]
        if role == self.TitleRole:
            return group["title"]
        if role == self.SourcesRole:
            return " + ".join(group["sources"])
        if role == self.SizeRole:
            return str(group["size"])
        if role == self.DateRole:
            return group["date_display"]
        if role == self.LinkCountRole:
            return group["link_count"]
        return None

    def set_groups(self, groups):
        self.beginResetModel()
        self._groups = list(groups)
        self.endResetModel()

    def group_at(self, row):
        if 0 <= row < len(self._groups):
            return self._groups[row]
        return None


class Backend(QObject):
    latestLoadingChanged = Signal()
    sourcesChanged = Signal()
    sourcesCountChanged = Signal()
    pageChanged = Signal()
    detailChanged = Signal()
    selectedResultIndexChanged = Signal()
    statusChanged = Signal()
    busyChanged = Signal()
    queryChanged = Signal()
    fuzzyChanged = Signal()
    thresholdChanged = Signal()
    activeSourceChanged = Signal()
    sortDescChanged = Signal()
    swarmTextsChanged = Signal()
    btTargetsChanged = Signal()
    updateInfoChanged = Signal()

    messageRequested = Signal(str, str, bool, str)
    overwriteRequested = Signal(str)
    updateDialogRequested = Signal()
    toastRequested = Signal(str)

    def __init__(self, sources_dir, parent=None):
        super().__init__(parent)
        self.sources_dir = sources_dir
        self.commands = commands_lib.SourceCommands(sources_dir)
        self.library = sources_lib.SourceLibrary()
        self.results = ResultsModel(self)
        self.search_ctl = search_lib.SearchController()
        self.tasks = TaskRunner(self)
        self.tasks.progress.connect(self.on_progress)
        self.tasks.done.connect(self.on_done)
        self.tasks.failed.connect(self.on_failed)
        self.tasks.changed.connect(self.on_tasks_changed)

        self._query = ""
        self._latest_wanted = None
        self._latest_inflight = set()
        self._latest_cache = {}
        self._latest_gen = 0
        self._latest_loading = False
        self._fuzzy = True
        self._threshold = 0.3
        self._active_source = ""
        self._selected_index = -1
        self._groups = []
        self._selected_group = None
        self._status = "正在加载源…"
        self._count_text = ""
        self._elapsed_text = ""
        self._swarm_texts = {}
        self._swarm_pending = set()
        self._bt_targets = []
        self._update_info = {}
        self._pending_name = ""
        self._pending_fetched = None
        self._download_url = ""
        self._detail_title = ""
        self._detail_sources = []
        self._detail_meta = ""
        self._detail_hint = ""
        self._detail_cards = []
        self._detail_raw = ""
        self._has_detail = False

        self.search_timer = QTimer(self)
        self.search_timer.setSingleShot(True)
        self.search_timer.setInterval(250)
        self.search_timer.timeout.connect(self.run_search)

        self.refresh_bt_targets()
        QTimer.singleShot(0, self.load_sources)

    # ------------------------------------------------------------ properties
    @Property(str, constant=True)
    def version(self):
        return core.APP_VERSION

    @Property(str, constant=True)
    def sourcesDir(self):
        return self.sources_dir

    @Property(QObject, constant=True)
    def resultsModel(self):
        return self.results

    @Property("QVariantList", notify=sourcesChanged)
    def sources(self):
        rows = [
            {
                "name": "",
                "display": "全部游戏",
                "count": len(self.library.items),
                "local": False,
                "active": not self._active_source,
            }
        ]
        for name, meta in sorted(
            self.library.metas.items(), key=lambda item: item[1]["display"]
        ):
            rows.append(
                {
                    "name": name,
                    "display": meta["display"],
                    "count": meta["count"],
                    "local": not bool(meta.get("url")),
                    "active": name == self._active_source,
                }
            )
        return rows

    @Property(str, notify=pageChanged)
    def pageTitle(self):
        meta = self.library.metas.get(self._active_source)
        return meta["display"] if meta else "全部游戏"

    @Property(str, notify=pageChanged)
    def pageSub(self):
        meta = self.library.metas.get(self._active_source)
        if meta:
            return f"{meta['count']:,} 个游戏 · 最后更新 {self.source_updated_text(meta)}"
        if self.library.metas:
            return (
                f"{len(self.library.items):,} 个游戏 · "
                f"去重后 {self.library.total_groups:,} 组"
            )
        return "正在加载源…"

    @Property(str, notify=pageChanged)
    def scopeText(self):
        meta = self.library.metas.get(self._active_source)
        return meta["display"] if meta else "全部游戏"

    @Property(str, notify=pageChanged)
    def countText(self):
        return self._count_text

    @Property(str, notify=pageChanged)
    def elapsedText(self):
        return self._elapsed_text

    @Property(str, notify=statusChanged)
    def statusText(self):
        return self._status

    @Property(bool, notify=busyChanged)
    def busy(self):
        return self.tasks.busy()

    @Property(bool, notify=latestLoadingChanged)
    def latestLoading(self):
        return self._latest_loading

    @Property(str, notify=queryChanged)
    def query(self):
        return self._query

    @Property(bool, notify=fuzzyChanged)
    def fuzzy(self):
        return self._fuzzy

    @Property(float, notify=thresholdChanged)
    def threshold(self):
        return self._threshold

    @Property(str, notify=activeSourceChanged)
    def activeSource(self):
        return self._active_source

    @Property(str, notify=activeSourceChanged)
    def activeSourceDisplay(self):
        meta = self.library.metas.get(self._active_source)
        return meta["display"] if meta else ""

    @Property(str, notify=activeSourceChanged)
    def activeSourceUrl(self):
        meta = self.library.metas.get(self._active_source)
        return (meta or {}).get("url") or ""

    @Property(bool, notify=sortDescChanged)
    def sortDesc(self):
        return self.search_ctl.sort_desc

    @Property(int, notify=selectedResultIndexChanged)
    def selectedResultIndex(self):
        return self._selected_index

    @Property(str, notify=detailChanged)
    def detailTitle(self):
        return self._detail_title

    @Property("QVariantList", notify=detailChanged)
    def detailSources(self):
        return list(self._detail_sources)

    @Property(str, notify=detailChanged)
    def detailMeta(self):
        return self._detail_meta

    @Property(str, notify=detailChanged)
    def detailHint(self):
        return self._detail_hint

    @Property("QVariantList", notify=detailChanged)
    def detailCards(self):
        return list(self._detail_cards)

    @Property(str, notify=detailChanged)
    def detailRawJson(self):
        return self._detail_raw

    @Property(bool, notify=detailChanged)
    def hasDetail(self):
        return self._has_detail

    @Property("QVariantMap", notify=swarmTextsChanged)
    def swarmTexts(self):
        return dict(self._swarm_texts)

    @Property("QVariantList", notify=btTargetsChanged)
    def btTargets(self):
        return list(self._bt_targets)

    @Property("QVariantList", constant=True)
    def btCatalog(self):
        return [
            {
                "key": entry["key"],
                "name": entry["name"],
                "desc": entry["desc"],
                "url": entry["url"],
            }
            for entry in bt.CATALOG
        ]

    @Property("QVariantMap", notify=updateInfoChanged)
    def updateInfo(self):
        return dict(self._update_info)

    # ------------------------------------------------------------- loading
    @Slot()
    def load_sources(self):
        if self.tasks.active("load"):
            return
        preserve = self._active_source
        self.library.reset()
        self._latest_gen += 1
        self._latest_cache.clear()
        self._latest_inflight.clear()
        self._latest_wanted = None
        self._set_latest_loading(False)
        self.results.set_groups([])
        self._groups = []
        self._selected_group = None
        self._selected_index = -1
        self._clear_detail()
        self._count_text = ""
        self._elapsed_text = ""
        self._active_source = preserve
        self.sourcesChanged.emit()
        self.sourcesCountChanged.emit()
        self.pageChanged.emit()
        self.selectedResultIndexChanged.emit()
        self.status = "正在加载源…"

        def task(report):
            return task_lib.load_sources(self.sources_dir, report)

        self.tasks.submit("load", task)
        self.update_busy()

    def on_progress(self, kind, event, payload):
        if kind == "load" and event == "status":
            self.status = (
                f"加载源 ({payload['index']}/{payload['total']}): {payload['name']}"
            )
        elif kind == "load" and event == "source":
            name, items, meta = payload
            self.library.add_source(name, items, meta)
            if self._active_source and self._active_source not in self.library.metas:
                self._active_source = ""
                self.activeSourceChanged.emit()
            self.sourcesChanged.emit()
            self.sourcesCountChanged.emit()
            self.pageChanged.emit()
        elif kind == "refresh" and event == "progress":
            self.status = (
                f"刷新源 ({payload['index']}/{payload['total']}): {payload['name']}"
            )

    def on_done(self, kind, result):
        if kind == "load":
            self.on_loaded(result[0] if result else [])
        elif kind == "search":
            self.on_search_done(*result)
        elif kind == "latest":
            self.on_latest_done(*result)
        elif kind == "fetch_add":
            self.on_fetched(result)
        elif kind == "add":
            self.on_added(result)
        elif kind == "update":
            self.on_updated(result)
        elif kind == "delete":
            self.on_deleted(result)
        elif kind == "import":
            self.on_imported(result)
        elif kind == "refresh":
            self.on_refreshed(result)
        elif kind == "check_update":
            self.on_checked_for_updates(result)
        elif kind == "swarm":
            self.on_swarm_done(result)
        self.update_busy()

    def on_failed(self, kind, error):
        self.update_busy()
        self.status = self.ready_status()
        if kind == "latest":
            self._latest_inflight.clear()
            self._latest_wanted = None
            self._set_latest_loading(False)
            self.toastRequested.emit(f"读取最新游戏失败：{error}")
            return
        if kind in ("fetch_add", "add"):
            self._pending_fetched = None
        if kind in ("search", "swarm", "check_update"):
            if kind == "swarm":
                self.toastRequested.emit(f"热度查询失败：{error}")
            else:
                self.show_message("操作失败", error, True)
            return
        self.show_message(
            "操作失败", error, True, self._cloudflare_link(error, self._download_url)
        )

    @staticmethod
    def _cloudflare_link(error, url):
        return url if "Cloudflare" in str(error or "") else ""

    def on_tasks_changed(self):
        self.update_busy()
        self.status = self.status if self.tasks.busy() else self.ready_status()

    def update_busy(self):
        self.busyChanged.emit()

    def on_loaded(self, errors):
        errors = errors or []
        self.sourcesChanged.emit()
        self.sourcesCountChanged.emit()
        self.pageChanged.emit()
        self.status = self.ready_status()
        if errors:
            self.status = f"{self.status} · {len(errors)} 个源加载失败"
            if not self.library.metas:
                self.show_message("加载源失败", "\n".join(errors[:20]), True)
        if self._query:
            self.run_search()
        else:
            self.run_latest()
        self.update_busy()

    # -------------------------------------------------------------- search
    @Slot(str)
    def set_query(self, text):
        self._query = str(text or "").strip()
        self.queryChanged.emit()
        if not self._query:
            self.search_timer.stop()
            self.run_latest()
            return
        self.status = "正在搜索…"
        self.search_timer.start()

    @Slot(bool)
    def set_fuzzy(self, enabled):
        self._fuzzy = bool(enabled)
        self.fuzzyChanged.emit()
        if self._query:
            self.search_timer.start()

    @Slot(float)
    def set_threshold(self, value):
        self._threshold = float(value)
        self.thresholdChanged.emit()
        if self._query:
            self.search_timer.start()

    @Slot(str)
    def select_source(self, name):
        name = str(name or "")
        if name and name not in self.library.metas:
            return
        if name == self._active_source:
            return
        self._active_source = name
        self.activeSourceChanged.emit()
        self.sourcesChanged.emit()
        self.pageChanged.emit()
        if self._query:
            self.search_timer.start()
        else:
            self.run_latest()

    def run_search(self):
        self._latest_wanted = None
        self._set_latest_loading(False)
        self.search_timer.stop()
        if not self._query or not self.library.items:
            self._clear_results()
            return
        ticket = self.search_ctl.begin(
            self._query,
            self.scopeText if self._active_source else "",
            self._fuzzy,
            self._threshold,
        )
        if ticket is None:
            return

        def task(report):
            return (
                ticket,
                *task_lib.search_sources(
                    self.library.items,
                    ticket.query,
                    ticket.fuzzy,
                    ticket.threshold,
                    ticket.source_filter,
                ),
            )

        self.tasks.submit("search", task)

    def on_search_done(self, ticket, groups, total):
        if not self.search_ctl.accepts(ticket.token):
            return
        elapsed = self.search_ctl.elapsed_ms(ticket)
        groups = self.search_ctl.apply_sort(groups)
        self._groups = groups
        self.results.set_groups(groups)
        self._count_text = f"找到 {total:,} 组"
        self._elapsed_text = f"{elapsed / 1000:.2f} 秒"
        self.pageChanged.emit()
        self.status = self.ready_status()
        if groups:
            self._selected_index = -1
            self.select_result(0)
        else:
            self._selected_index = -1
            self.selectedResultIndexChanged.emit()
            self._clear_detail()
        self.update_busy()

    def _set_latest_loading(self, value):
        value = bool(value)
        if value != self._latest_loading:
            self._latest_loading = value
            self.latestLoadingChanged.emit()

    def run_latest(self):
        self.search_timer.stop()
        self.search_ctl.invalidate()
        if not self.library.items:
            self._clear_results()
            return
        source_filter = self.scopeText if self._active_source else ""
        cache_key = self._active_source
        cached = self._latest_cache.get(cache_key)
        if cached is not None:
            self._latest_wanted = cache_key
            self._set_latest_loading(False)
            self._apply_latest(*cached)
            return
        if cache_key in self._latest_inflight:
            self._latest_wanted = cache_key
            self._set_latest_loading(True)
            return

        self._clear_results()
        self._latest_wanted = cache_key
        gen = self._latest_gen
        items = self.library.items

        def task(report):
            return (
                gen,
                cache_key,
                *task_lib.latest_sources(items, source_filter, core.LATEST_LIMIT),
            )

        self._latest_inflight.add(cache_key)
        self._set_latest_loading(True)
        self.tasks.submit("latest", task)

    def on_latest_done(self, gen, cache_key, groups, total):
        if gen != self._latest_gen:
            return
        self._latest_inflight.discard(cache_key)
        self._latest_cache[cache_key] = (groups, total)
        if self._query or cache_key != self._latest_wanted:
            return
        self._set_latest_loading(False)
        self._apply_latest(groups, total)

    def _apply_latest(self, groups, total):
        self._groups = groups
        self.results.set_groups(groups)
        shown = len(groups)
        if total > shown:
            self._count_text = f"显示最新 {shown:,} 组 · 共 {total:,} 组"
        else:
            self._count_text = f"共 {total:,} 组"
        self._elapsed_text = ""
        self._selected_index = -1
        self._selected_group = None
        self.selectedResultIndexChanged.emit()
        self._clear_detail()
        self.pageChanged.emit()
        self.status = self.ready_status()
        self.update_busy()

    def _clear_results(self):
        self._latest_wanted = None
        self._set_latest_loading(False)
        self.search_ctl.invalidate()
        self.results.set_groups([])
        self._groups = []
        self._selected_index = -1
        self._count_text = ""
        self._elapsed_text = ""
        self.pageChanged.emit()
        self.selectedResultIndexChanged.emit()
        self._clear_detail()
        self.status = self.ready_status()
        self.update_busy()

    @Slot()
    def toggle_sort(self):
        self.search_ctl.toggle_date_sort()
        if self._groups:
            groups = self.search_ctl.apply_sort(self._groups)
            self._groups = groups
            self.results.set_groups(groups)
            if self._selected_index >= 0:
                self.select_result(0)
        self.sortDescChanged.emit()

    # ------------------------------------------------------------- details
    @Slot(int)
    def select_result(self, index):
        group = self.results.group_at(index)
        if group is None:
            self._selected_index = -1
            self.selectedResultIndexChanged.emit()
            self._clear_detail()
            return
        self._selected_group = group
        self._selected_index = index
        self.selectedResultIndexChanged.emit()
        model = details_lib.build_detail(
            group, expanded=set(), raw_visible=False, link_limit=100000
        )
        self._detail_title = model.title
        self._detail_sources = list(model.sources)
        self._detail_meta = model.meta
        self._detail_hint = model.hint
        self._detail_cards = self._cards_for_qml(model)
        raw = json.dumps(
            [item.get("raw") for item in group["items"]],
            ensure_ascii=False,
            indent=2,
        )
        self._detail_raw = raw if len(raw) <= 100000 else raw[:100000] + "\n\n…"
        self._has_detail = True
        self.detailChanged.emit()

    @staticmethod
    def _cards_for_qml(model):
        cards = []
        for card in model.cards:
            groups = []
            for link_group in card.links:
                groups.append(
                    {
                        "label": link_group.label,
                        "total": link_group.total,
                        "links": [
                            {"uri": link.uri, "isMagnet": link.is_magnet}
                            for link in link_group.visible
                        ],
                    }
                )
            cards.append(
                {
                    "key": card.key,
                    "source": card.source,
                    "date": card.date_text,
                    "size": card.size,
                    "description": card.description,
                    "links": groups,
                }
            )
        return cards

    def _clear_detail(self):
        self._selected_group = None
        self._detail_title = ""
        self._detail_sources = []
        self._detail_meta = ""
        self._detail_hint = ""
        self._detail_cards = []
        self._detail_raw = ""
        self._has_detail = False
        self.detailChanged.emit()

    @Slot()
    def copy_current_links(self):
        if not self._selected_group:
            return
        self.copy_text("\n".join(self._selected_group.get("uris") or []))

    @Slot(str)
    def copy_text(self, text):
        if not text:
            return
        QGuiApplication.clipboard().setText(text)
        self.toastRequested.emit("已复制到剪贴板")

    @Slot()
    def open_sources_folder(self):
        QDesktopServices.openUrl(QUrl.fromLocalFile(self.sources_dir))

    # ------------------------------------------------------ source actions
    @Slot(str, str)
    def request_add_source(self, url, filename):
        url = str(url or "").strip()
        if not url:
            self.show_message("提示", "请输入源地址。", False)
            return
        if not url.startswith(("http://", "https://")):
            url = "https://" + url
        name = commands_lib.safe_source_filename(
            str(filename or "").strip() or commands_lib.url_to_filename(url)
        )
        self._pending_name = name
        self._pending_fetched = None
        self._download_url = url
        self.status = "正在下载源…"

        def task(report):
            return self.commands.fetch(url)

        self.tasks.submit("fetch_add", task)

    def on_fetched(self, fetched):
        self._pending_fetched = fetched
        dest = os.path.join(self.sources_dir, self._pending_name)
        if os.path.exists(dest):
            self.overwriteRequested.emit(self._pending_name)
            return
        self._save_pending(False)

    @Slot()
    def confirm_overwrite(self):
        if self._pending_fetched is not None:
            self._save_pending(True)

    @Slot()
    def cancel_overwrite(self):
        self._pending_fetched = None
        self._pending_name = ""

    def _save_pending(self, overwrite):
        fetched = self._pending_fetched
        if fetched is None:
            return
        name = self._pending_name
        self.status = f"正在保存 {name}…"

        def task(report):
            return self.commands.add_fetched(fetched, name, overwrite)

        self.tasks.submit("add", task)

    def on_added(self, change):
        self._pending_fetched = None
        self.toastRequested.emit(f"已保存 {change.name}，正在重新加载…")
        self.load_sources()

    @Slot(str)
    def request_update_source(self, url):
        name = self._active_source
        if not name:
            self.show_message("提示", "请先在左侧选择一个源。", False)
            return
        self._download_url = (
            str(url or "").strip()
            or core.read_source_url(os.path.join(self.sources_dir, name))
            or ""
        )
        self.status = "正在更新源…"

        def task(report):
            return self.commands.update(name, url)

        self.tasks.submit("update", task)

    def on_updated(self, change):
        self.toastRequested.emit(f"已更新 {change.name}")
        self.load_sources()

    @Slot()
    def delete_active_source(self):
        name = self._active_source
        if not name:
            self.show_message("提示", "请先选择一个源。", False)
            return
        self.status = "正在删除源…"

        def task(report):
            return self.commands.delete(name)

        self.tasks.submit("delete", task)

    def on_deleted(self, change):
        self._active_source = ""
        self.activeSourceChanged.emit()
        self.toastRequested.emit(f"已删除 {change.name}")
        self.load_sources()

    @Slot("QVariantList")
    def import_sources(self, files):
        paths = []
        for item in files:
            if isinstance(item, QUrl):
                paths.append(item.toLocalFile())
            else:
                text = str(item)
                if text.startswith("file:"):
                    paths.append(QUrl(text).toLocalFile())
                else:
                    paths.append(text)
        paths = [path for path in paths if path]
        if not paths:
            return
        self.status = "正在导入源…"

        def task(report):
            return self.commands.import_files(tuple(paths))

        self.tasks.submit("import", task)

    def on_imported(self, result):
        if result.added:
            message = f"成功导入 {len(result.added)} 个源"
            if result.errors:
                message += f"，失败 {len(result.errors)} 个\n\n" + "\n".join(
                    result.errors[:10]
                )
            self.show_message("导入完成", message, False)
            self.load_sources()
        elif result.errors:
            self.show_message("导入失败", "\n".join(result.errors[:20]), True)
        self.update_busy()

    @Slot()
    def refresh_all(self):
        count = sum(1 for meta in self.library.metas.values() if meta.get("url"))
        if not count:
            self.show_message("提示", "没有记录了网址的源可以刷新。", False)
            return
        self.status = "正在刷新全部源…"

        def task(report):
            return self.commands.refresh(
                report=lambda index, total, name: report(
                    "progress", {"index": index, "total": total, "name": name}
                )
            )

        self.tasks.submit("refresh", task)

    def on_refreshed(self, result):
        lines = [f"成功更新 {len(result.updated)} 个源，失败 {len(result.failed)} 个"]
        lines.extend(f"{name}: {error}" for name, error in result.failed[:5])
        blocked = []
        for name, error in result.failed:
            if "Cloudflare" not in str(error or ""):
                continue
            url = core.read_source_url(os.path.join(self.sources_dir, name))
            if url:
                blocked.append(url)
        if blocked:
            lines.append("")
            lines.append("受 Cloudflare 拦截的地址：")
            lines.extend(blocked[:5])
        self.show_message(
            "刷新完成", "\n".join(lines), False, blocked[0] if len(blocked) == 1 else ""
        )
        self.load_sources()

    # -------------------------------------------------------------- update
    @Slot()
    def check_updates(self):
        self.status = "正在检查更新…"

        def task(report):
            try:
                return updates.check_latest(UPDATE_REPO, core.APP_VERSION), None
            except Exception as exc:
                return None, str(exc)

        self.tasks.submit("check_update", task)

    def on_checked_for_updates(self, result):
        outcome, error = result
        self.status = self.ready_status()
        if error:
            self.show_message("检查更新失败", error, True)
            return
        if not outcome.update_available:
            self.show_message(
                "检查更新", f"当前已是最新版本（{outcome.current}）。", False
            )
            return
        release = outcome.release
        self._update_info = {
            "current": outcome.current,
            "latest": outcome.latest,
            "name": release.name,
            "url": release.url,
            "downloadUrl": release.download_url or "",
            "notes": release.notes,
            "published": release.published_at,
        }
        self.updateInfoChanged.emit()
        self.updateDialogRequested.emit()

    # -------------------------------------------------------------- swarm
    @Slot(str)
    def query_swarm(self, uri):
        if uri in self._swarm_pending:
            return
        self._swarm_pending.add(uri)
        self._swarm_texts[uri] = "正在查询…"
        self.swarmTextsChanged.emit()

        def task(report):
            try:
                return uri, swarm.query_swarm(uri), None
            except Exception as exc:
                return uri, None, str(exc)

        self.tasks.submit("swarm", task)

    def on_swarm_done(self, result):
        uri, stats, error = result
        self._swarm_pending.discard(uri)
        if error or stats is None:
            self._swarm_texts[uri] = "查询失败"
        elif not stats.trackers_ok:
            self._swarm_texts[uri] = "暂无热度数据"
        else:
            self._swarm_texts[uri] = swarm.format_stats(stats)
        self.swarmTextsChanged.emit()

    # ----------------------------------------------------------------- bt
    def refresh_bt_targets(self):
        self._bt_targets = [
            {"key": target["key"], "name": target["name"], "kind": target["kind"]}
            for target in bt.detect()
        ]
        self.btTargetsChanged.emit()

    @Slot(str, str)
    def send_to_bt(self, uri, key):
        target = next(
            (item for item in bt.detect() if item["key"] == key), None
        )
        if target is None or not uri:
            self.show_message("提示", "没有找到 BT 客户端。", True)
            return
        plan = bt.plan(uri, target)
        if not plan:
            return
        QGuiApplication.clipboard().setText(uri)
        try:
            if plan.get("kind") == "shell":
                os.startfile(uri)
            else:
                exe = plan.get("exe", "")
                if exe and os.path.exists(exe):
                    subprocess.Popen([exe] + list(plan.get("args", [])), shell=False)
                else:
                    os.startfile(uri)
            self.toastRequested.emit(f"已发送到 {target['name']}（链接已复制）")
        except Exception as exc:
            self.show_message("启动失败", str(exc), True)

    # ------------------------------------------------------------- helpers
    def show_message(self, title, text, error, link=""):
        self.messageRequested.emit(title, text, bool(error), link or "")

    @Slot(str)
    def open_external_url(self, url):
        text = str(url or "").strip()
        if text.startswith(("http://", "https://")):
            QDesktopServices.openUrl(QUrl(text))

    def ready_status(self):
        if not self.library.metas:
            return "就绪"
        return (
            f"就绪 · {len(self.library.metas)} 个源 · "
            f"{len(self.library.items):,} 个游戏"
        )

    @staticmethod
    def source_updated_text(meta):
        updated = meta.get("updated") if meta else None
        if not updated:
            return "未知"
        return time.strftime("%Y-%m-%d %H:%M", time.localtime(updated))

    @property
    def status(self):
        return self._status

    @status.setter
    def status(self, value):
        if value == self._status:
            return
        self._status = value
        self.statusChanged.emit()

    @Slot()
    def shutdown(self):
        self.tasks.shutdown()
