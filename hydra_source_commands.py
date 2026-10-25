"""GUI-free commands that own every write to the sources directory."""

import json
import os
import re
from dataclasses import dataclass

import hydra_core as core


class SourceCommandError(RuntimeError):
    pass


class SourceExists(SourceCommandError):
    def __init__(self, name):
        super().__init__(f"源文件 {name} 已存在")
        self.name = name


@dataclass(frozen=True)
class FetchedSource:
    url: str
    data: dict
    count: int
    default_name: str


@dataclass(frozen=True)
class SourceChange:
    name: str
    count: int | None = None


@dataclass(frozen=True)
class ImportResult:
    added: tuple[SourceChange, ...]
    errors: tuple[str, ...]


@dataclass(frozen=True)
class RefreshResult:
    updated: tuple[SourceChange, ...]
    failed: tuple[tuple[str, str], ...]


def url_to_filename(url):
    from urllib.parse import urlparse

    path = urlparse(url).path
    name = os.path.basename(path) or "source"
    name = os.path.splitext(name)[0]
    name = re.sub(r'[\\/:*?"<>|]', '_', name)
    return name[:50] or "source"


def safe_source_filename(text):
    name = str(text or "").strip()
    if name.lower().endswith(".json"):
        name = name[:-5]
    name = re.sub(r'[\\/:*?"<>|]', '_', name).strip()
    return (name[:80] or "source") + ".json"


def _validate_data(data, name):
    repacks, _ = core.extract_repacks(data, name)
    if not repacks:
        keys = list(data.keys()) if isinstance(data, dict) else type(data).__name__
        raise SourceCommandError(f"不是有效的目录 JSON 格式。JSON 顶层字段: {keys}")
    return data, len(repacks)


def _read_source_file(path, name):
    with open(path, "r", encoding="utf-8") as handle:
        try:
            data = json.load(handle)
        except json.JSONDecodeError as exc:
            raise SourceCommandError(f"不是有效 JSON: {exc}") from exc
    return _validate_data(data, name)


def _parse_content(content, name):
    try:
        data = json.loads(content)
    except json.JSONDecodeError as exc:
        raise SourceCommandError(f"返回内容不是有效 JSON: {exc}") from exc
    return _validate_data(data, name)


class SourceCommands:
    def __init__(self, sources_dir, downloader_factory=None):
        self.sources_dir = sources_dir
        self._downloader_factory = downloader_factory or core.Downloader

    def fetch(self, method, url):
        downloader = self._downloader_factory()
        content = downloader.download_json(method, url)
        default_name = url_to_filename(url)
        data, count = _parse_content(content, default_name)
        return FetchedSource(
            url=url,
            data=data,
            count=count,
            default_name=default_name,
        )

    def add_fetched(self, fetched, filename, overwrite=False):
        name = safe_source_filename(filename)
        path = os.path.join(self.sources_dir, name)
        if os.path.exists(path) and not overwrite:
            raise SourceExists(name)
        self._write(name, fetched.url, fetched.data)
        return SourceChange(name, fetched.count)

    def update(self, method, name, url=None):
        name = self._checked_name(name)
        path = os.path.join(self.sources_dir, name)
        url = (url or "").strip() or core.read_source_url(path)
        if not url:
            raise SourceCommandError(f"{name}: 没有记录源地址")
        try:
            fetched = self.fetch(method, url)
        except Exception as exc:
            raise SourceCommandError(f"{name}: {exc}") from exc
        self._write(name, url, fetched.data)
        return SourceChange(name, fetched.count)

    def import_files(self, paths, overwrite=()):
        approved = set(overwrite)
        added = []
        errors = []
        for path in paths:
            base = os.path.basename(str(path))
            if not base.lower().endswith(".json"):
                errors.append(f"{base}: 不是 .json 文件")
                continue
            name = safe_source_filename(base)
            try:
                data, count = _read_source_file(path, name)
            except (OSError, SourceCommandError) as exc:
                errors.append(f"{name}: {exc}")
                continue
            dest = os.path.join(self.sources_dir, name)
            if os.path.exists(dest) and name not in approved:
                errors.append(f"{name}: 已存在，未覆盖")
                continue
            try:
                core.write_json_atomic(dest, data)
                core.invalidate_cache(self.sources_dir, name)
            except OSError as exc:
                errors.append(f"{name}: {exc}")
                continue
            added.append(SourceChange(name, count))
        return ImportResult(tuple(added), tuple(errors))

    def delete(self, name):
        name = self._checked_name(name)
        path = os.path.join(self.sources_dir, name)
        cache_path = core.cache_path_for(self.sources_dir, name)
        for target in (path, path + ".url", cache_path, cache_path + ".tmp"):
            try:
                os.remove(target)
            except OSError:
                pass
        return SourceChange(name)

    def refresh(self, method, names=None, report=None):
        if names is None:
            try:
                names = sorted(
                    n for n in os.listdir(self.sources_dir) if n.endswith(".json")
                )
            except OSError:
                names = []
        targets = []
        for name in names:
            name = self._checked_name(name)
            url = core.read_source_url(os.path.join(self.sources_dir, name))
            if url:
                targets.append((name, url))

        downloader = self._downloader_factory()
        updated = []
        failed = []
        total = len(targets)
        for index, (name, url) in enumerate(targets, 1):
            if report:
                report(index, total, name)
            try:
                content = downloader.download_json(method, url)
                data, count = _parse_content(content, name)
                self._write(name, url, data)
                updated.append(SourceChange(name, count))
            except Exception as exc:
                failed.append((name, str(exc)))
        return RefreshResult(tuple(updated), tuple(failed))

    def _write(self, name, url, data):
        path = os.path.join(self.sources_dir, name)
        try:
            core.write_json_atomic(path, data)
            core.write_url_file(path, url)
        except OSError as exc:
            raise SourceCommandError(f"{name}: {exc}") from exc
        core.invalidate_cache(self.sources_dir, name)
        return path

    @staticmethod
    def _checked_name(name):
        raw = str(name or "")
        base = os.path.basename(raw)
        if raw != base or not base.lower().endswith(".json"):
            raise SourceCommandError(f"无效的源文件名: {raw!r}")
        return base
