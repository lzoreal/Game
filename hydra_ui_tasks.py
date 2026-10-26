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
        self.cancelled = False

    def run(self):
        def report(event, payload):
            if self.cancelled:
                return
            try:
                self.progress.emit(self.kind, event, payload)
            except RuntimeError:
                # The runner can be destroyed while the app is shutting down.
                pass

        try:
            result = self.task(report)
        except Exception as exc:
            if self.cancelled:
                return
            try:
                self.failed.emit(self.kind, str(exc))
            except RuntimeError:
                pass
            return
        if self.cancelled:
            return
        try:
            self.done.emit(self.kind, result)
        except RuntimeError:
            pass


class TaskRunner(QObject):
    progress = Signal(str, str, object)
    done = Signal(str, object)
    failed = Signal(str, str)
    changed = Signal()

    def __init__(self, parent=None):
        super().__init__(parent)
        self._threads = []
        self._finished = []
        self._active = {}

    def submit(self, kind, task):
        thread = TaskThread(kind, task)
        self._threads.append(thread)
        self._active[kind] = self._active.get(kind, 0) + 1
        # Bound methods, not signal-to-signal connections: when the runner is
        # destroyed, Qt can then drop still-queued deliveries safely.
        thread.progress.connect(self._relay_progress)
        thread.done.connect(self._relay_done)
        thread.failed.connect(self._relay_failed)
        thread.finished.connect(partial(self._on_finished, thread, kind))
        thread.start()

    def _relay_progress(self, kind, event, payload):
        self.progress.emit(kind, event, payload)

    def _relay_done(self, kind, result):
        self.done.emit(kind, result)

    def _relay_failed(self, kind, error):
        self.failed.emit(kind, error)

    def _on_finished(self, thread, kind):
        try:
            if thread in self._threads:
                self._threads.remove(thread)
            remaining = self._active.get(kind, 1) - 1
            if remaining > 0:
                self._active[kind] = remaining
            else:
                self._active.pop(kind, None)
            # Keep finished threads alive: deleting one while queued signal
            # deliveries are still pending produces "signal source deleted"
            # warnings. They are released in shutdown().
            thread.task = None
            self._finished.append(thread)
            self.changed.emit()
        except RuntimeError:
            # The Qt object can already be gone while a worker thread unwinds.
            pass

    def active(self, kind):
        return self._active.get(kind, 0)

    def busy(self):
        return any(
            kind not in ("search", "latest", "swarm", "check_update")
            for kind in self._active
        )

    def shutdown(self):
        # Let workers unwind on their own: QThread.terminate() can deadlock a
        # Python process, especially while the interpreter is exiting.
        for thread in list(self._threads):
            thread.cancelled = True
            try:
                thread.progress.disconnect()
                thread.done.disconnect()
                thread.failed.disconnect()
            except (RuntimeError, TypeError):
                # Already disconnected or the Qt object is gone.
                pass
            if thread.isRunning():
                thread.requestInterruption()
                thread.wait(5000)
        self._finished.clear()
