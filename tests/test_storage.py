import gzip
import json
import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import catalog as core


def payload(title="Alpha", uri="magnet:a"):
    return {"downloads": [{"title": title, "uris": [uri]}]}


def write_plain(path, data):
    with open(path, "w", encoding="utf-8") as handle:
        json.dump(data, handle, indent=2)


class StorageTestBase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="gamemagnet_storage_")
        self.sources = os.path.join(self.tmp, "sources")
        os.makedirs(self.sources)

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)

    def path(self, name):
        return os.path.join(self.sources, name)


class WriteReadTests(StorageTestBase):
    def test_gzip_round_trip_and_compact_payload(self):
        path = self.path("a.json.gz")
        core.write_source_file(path, payload())

        with open(path, "rb") as handle:
            raw = handle.read()
        self.assertEqual(raw[:2], b"\x1f\x8b")
        body = gzip.decompress(raw)
        self.assertNotIn(b"\n", body)
        self.assertEqual(core.read_source_file(path), payload())

    def test_display_name_strips_gzip_suffix(self):
        self.assertEqual(core.display_name("fitgirl.json.gz"), "fitgirl")
        self.assertEqual(core.display_name("skidrow_cleaned.json.gz"), "skidrow")
        self.assertEqual(core.display_name("plain.json"), "plain")

    def test_source_filenames_prefers_gzip_over_plain(self):
        for name in ("a.json", "a.json.gz", "b.json", "b.json.gz", "notes.txt"):
            with open(self.path(name), "w", encoding="utf-8") as handle:
                handle.write("x")
        self.assertEqual(
            core.source_filenames(self.sources), ["a.json.gz", "b.json.gz"]
        )


class LoadTests(StorageTestBase):
    def test_load_source_reads_gzip_without_writing_cache(self):
        path = self.path("a.json.gz")
        core.write_source_file(path, payload())
        core.write_url_file(path, "https://example.com/a.json")

        items, meta = core.load_source(self.sources, "a.json.gz")

        self.assertEqual(len(items), 1)
        self.assertEqual(items[0]["title"], "Alpha")
        self.assertEqual(items[0]["source_display"], "a")
        self.assertEqual(meta["url"], "https://example.com/a.json")
        self.assertFalse(os.path.exists(os.path.join(self.sources, ".cache")))

    def test_load_source_still_reads_plain_json(self):
        write_plain(self.path("legacy.json"), payload("Legacy"))

        items, meta = core.load_source(self.sources, "legacy.json")

        self.assertEqual(items[0]["title"], "Legacy")
        self.assertEqual(meta["count"], 1)


class UrlSidecarTests(StorageTestBase):
    def test_write_uses_source_url_suffix(self):
        path = self.path("a.json.gz")
        core.write_source_file(path, payload())

        core.write_url_file(path, "https://example.com/a.json")

        self.assertTrue(os.path.exists(path + ".source-url"))
        self.assertFalse(os.path.exists(path + ".url"))
        self.assertEqual(core.read_source_url(path), "https://example.com/a.json")


if __name__ == "__main__":
    unittest.main()
