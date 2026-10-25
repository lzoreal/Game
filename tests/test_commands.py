import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import hydra_core
from hydra_source_commands import (
    SourceCommandError,
    SourceCommands,
    SourceExists,
    safe_source_filename,
    url_to_filename,
)


def payload(title="Alpha", uri="magnet:a"):
    return {"downloads": [{"title": title, "uris": [uri]}]}


def write_source(path, title="Old", uri="magnet:old"):
    data = payload(title, uri)
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle)
    return data


def read_source(path):
    with open(path, encoding="utf-8") as handle:
        return json.load(handle)


class FakeDownloader:
    def __init__(self, payloads=None, fail_urls=()):
        self.payloads = payloads or {}
        self.fail_urls = set(fail_urls)

    def download_json(self, method, url, timeout=30):
        if url in self.fail_urls:
            raise RuntimeError("boom")
        return self.payloads[url]


class CommandTestBase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="hydra_command_test_")
        self.sources = os.path.join(self.tmp, "sources")
        os.makedirs(self.sources)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def commands(self, payloads=None, fail_urls=()):
        downloader = FakeDownloader(payloads, fail_urls)
        return SourceCommands(self.sources, lambda: downloader)

    def path(self, name):
        return os.path.join(self.sources, name)


class NamingTests(unittest.TestCase):
    def test_filename_rules(self):
        self.assertEqual(
            url_to_filename("https://example.com/sources/catalog.json"), "catalog"
        )
        self.assertEqual(safe_source_filename(" My:Game "), "My_Game.json")
        self.assertEqual(safe_source_filename("already.json"), "already.json")


class FetchTests(CommandTestBase):
    def test_fetch_returns_validated_source(self):
        url = "https://example.com/a.json"
        commands = self.commands({url: json.dumps(payload())})
        fetched = commands.fetch("urllib", url)
        self.assertEqual(fetched.url, url)
        self.assertEqual(fetched.count, 1)
        self.assertEqual(fetched.default_name, "a")
        self.assertEqual(fetched.data["downloads"][0]["title"], "Alpha")

    def test_fetch_rejects_bad_payloads(self):
        url = "https://example.com/a.json"
        commands = self.commands(
            {url: "{not json", "https://example.com/b.json": '{"foo": []}'}
        )
        with self.assertRaises(SourceCommandError):
            commands.fetch("urllib", url)
        with self.assertRaises(SourceCommandError) as ctx:
            commands.fetch("urllib", "https://example.com/b.json")
        self.assertIn("目录 JSON", str(ctx.exception))


class AddTests(CommandTestBase):
    def test_add_writes_json_url_and_clears_cache(self):
        url = "https://example.com/a.json"
        commands = self.commands({url: json.dumps(payload())})
        cache = os.path.join(self.sources, ".cache", "a.json.cache")
        os.makedirs(os.path.dirname(cache))
        with open(cache, "w", encoding="utf-8") as handle:
            handle.write("stale")

        change = commands.add_fetched(commands.fetch("urllib", url), "a")

        self.assertEqual(change.name, "a.json")
        self.assertEqual(change.count, 1)
        self.assertEqual(read_source(self.path("a.json"))["downloads"][0]["title"], "Alpha")
        with open(self.path("a.json.url"), encoding="utf-8") as handle:
            self.assertEqual(handle.read(), url)
        self.assertFalse(os.path.exists(cache))

    def test_add_refuses_existing_unless_overwritten(self):
        url = "https://example.com/a.json"
        commands = self.commands({url: json.dumps(payload("New"))})
        write_source(self.path("a.json"), "Old")
        fetched = commands.fetch("urllib", url)

        with self.assertRaises(SourceExists):
            commands.add_fetched(fetched, "a.json")
        self.assertEqual(read_source(self.path("a.json"))["downloads"][0]["title"], "Old")

        change = commands.add_fetched(fetched, "a.json", overwrite=True)
        self.assertEqual(change.name, "a.json")
        self.assertEqual(read_source(self.path("a.json"))["downloads"][0]["title"], "New")


class UpdateTests(CommandTestBase):
    def test_update_reads_url_sidecar_and_writes(self):
        path = self.path("a.json")
        write_source(path, "Old")
        hydra_core.write_url_file(path, "https://example.com/a.json")
        commands = self.commands(
            {"https://example.com/a.json": json.dumps(payload("New"))}
        )

        change = commands.update("urllib", "a.json")

        self.assertEqual((change.name, change.count), ("a.json", 1))
        self.assertEqual(read_source(path)["downloads"][0]["title"], "New")

    def test_update_uses_explicit_url_and_updates_sidecar(self):
        path = self.path("a.json")
        write_source(path, "Old")
        hydra_core.write_url_file(path, "https://example.com/old.json")
        commands = self.commands(
            {"https://example.com/new.json": json.dumps(payload("New"))}
        )

        commands.update("urllib", "a.json", "https://example.com/new.json")

        with open(path + ".url", encoding="utf-8") as handle:
            self.assertEqual(handle.read(), "https://example.com/new.json")

    def test_update_failure_includes_filename(self):
        path = self.path("a.json")
        write_source(path, "Old")
        hydra_core.write_url_file(path, "https://example.com/bad.json")
        commands = self.commands({}, {"https://example.com/bad.json"})

        with self.assertRaises(SourceCommandError) as ctx:
            commands.update("urllib", "a.json")
        self.assertIn("a.json", str(ctx.exception))


class ImportTests(CommandTestBase):
    def test_import_validates_files_without_url_sidecar(self):
        valid = os.path.join(self.tmp, "catalog-import.json")
        write_source(valid, "Imported")
        invalid = os.path.join(self.tmp, "bad.json")
        with open(invalid, "w", encoding="utf-8") as handle:
            handle.write("{not json")
        not_json = os.path.join(self.tmp, "note.txt")
        with open(not_json, "w", encoding="utf-8") as handle:
            handle.write("x")
        cache = os.path.join(self.sources, ".cache", "catalog-import.json.cache")
        os.makedirs(os.path.dirname(cache))
        with open(cache, "w", encoding="utf-8") as handle:
            handle.write("stale")

        result = self.commands().import_files([valid, invalid, not_json])

        self.assertEqual([change.name for change in result.added], ["catalog-import.json"])
        self.assertEqual(len(result.errors), 2)
        self.assertEqual(
            read_source(self.path("catalog-import.json"))["downloads"][0]["title"],
            "Imported",
        )
        self.assertFalse(os.path.exists(self.path("catalog-import.json.url")))
        self.assertFalse(os.path.exists(cache))

    def test_import_requires_approved_overwrite(self):
        dest = self.path("dup.json")
        write_source(dest, "Old")
        incoming = os.path.join(self.tmp, "dup.json")
        write_source(incoming, "New")
        commands = self.commands()

        result = commands.import_files([incoming])
        self.assertEqual(result.added, ())
        self.assertEqual(len(result.errors), 1)
        self.assertEqual(read_source(dest)["downloads"][0]["title"], "Old")

        result = commands.import_files([incoming], overwrite=("dup.json",))
        self.assertEqual([change.name for change in result.added], ["dup.json"])
        self.assertEqual(read_source(dest)["downloads"][0]["title"], "New")


class DeleteTests(CommandTestBase):
    def test_delete_removes_source_url_and_caches(self):
        path = self.path("a.json")
        write_source(path)
        hydra_core.write_url_file(path, "https://example.com/a.json")
        cache = os.path.join(self.sources, ".cache", "a.json.cache")
        os.makedirs(os.path.dirname(cache))
        for target in (cache, cache + ".tmp"):
            with open(target, "w", encoding="utf-8") as handle:
                handle.write("x")

        change = self.commands().delete("a.json")

        self.assertEqual(change.name, "a.json")
        for target in (path, path + ".url", cache, cache + ".tmp"):
            self.assertFalse(os.path.exists(target))

    def test_delete_rejects_paths_outside_sources(self):
        with self.assertRaises(SourceCommandError):
            self.commands().delete(os.path.join("..", "a.json"))


class RefreshTests(CommandTestBase):
    def test_refresh_reports_progress_and_failures(self):
        good = self.path("a.json")
        bad = self.path("b.json")
        no_url = self.path("c.json")
        for target in (good, bad, no_url):
            write_source(target, "Old")
        hydra_core.write_url_file(good, "https://example.com/a.json")
        hydra_core.write_url_file(bad, "https://example.com/b.json")
        commands = self.commands(
            {"https://example.com/a.json": json.dumps(payload("New"))},
            {"https://example.com/b.json"},
        )
        events = []

        result = commands.refresh(
            "urllib", report=lambda index, total, name: events.append((index, total, name))
        )

        self.assertEqual([change.name for change in result.updated], ["a.json"])
        self.assertEqual(result.failed, (("b.json", "boom"),))
        self.assertEqual(events, [(1, 2, "a.json"), (2, 2, "b.json")])
        self.assertEqual(read_source(good)["downloads"][0]["title"], "New")
        self.assertEqual(read_source(bad)["downloads"][0]["title"], "Old")

    def test_refresh_accepts_name_subset(self):
        first = self.path("a.json")
        second = self.path("b.json")
        write_source(first)
        write_source(second)
        hydra_core.write_url_file(first, "https://example.com/a.json")
        hydra_core.write_url_file(second, "https://example.com/b.json")
        commands = self.commands(
            {"https://example.com/a.json": json.dumps(payload("New"))}
        )

        result = commands.refresh("urllib", names=["a.json"])

        self.assertEqual([change.name for change in result.updated], ["a.json"])
        self.assertEqual(read_source(second)["downloads"][0]["title"], "Old")


if __name__ == "__main__":
    unittest.main()
