"""Generic Qt background task thread and runner."""

from functools import partial

from PySide6.QtCore import QObject, QThread, Signal


class TaskThread(QThread):
    progress = Signal(str, str, object)
    done = Signal(str, object)
    failed = Signal(str, str)

    def __init__(self, kind, task):
        super().__init__()
        self.kind = kind
        self.task = task

    def run(self):
        def report(event, payload):
            self.progress.emit(self.kind, event, payload)

        try:
            result = self.task(report)
        except Exception as exc:
            self.failed.emit(self.kind, str(exc))
            return
        self.done.emit(self.kind, result)


class TaskRunner(QObject):
    progress = Signal(str, str, object)
    done = Signal(str, object)
    failed = Signal(str, str)
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._threads = []
        self._active = {}

    def submit(self, kind, task):
        thread = TaskThread(kind, task)
        self._threads.append(thread)
        self._active[kind] = self._active.get(kind, 0) + 1
        thread.progress.connect(self.progress.emit)
        thread.done.connect(self.done.emit)
        thread.failed.connect(self.failed.emit)
        thread.finished.connect(partial(self._on_finished, thread, kind))
        thread.start()

    def _on_finished(self, thread, kind):
        if thread in self._threads:
            self._threads.remove(thread)
        remaining = self._active.get(kind, 1) - 1
        if remaining > 0:
            self._active[kind] = remaining
        else:
            self._active.pop(kind, None)
        thread.deleteLater()
        self.changed.emit()

    def active(self, kind):
        return self._active.get(kind, 0)

    def busy(self):
        return any(kind not in ("search", "swarm", "check_update") for kind in self._active)

    def shutdown(self):
        for thread in list(self._threads):
            if thread.isRunning():
                thread.terminate()
                thread.wait(500)
