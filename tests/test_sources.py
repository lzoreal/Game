import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from source_library import SourceDelta, SourceLibrary


def item(title, dkey, count=1):
    return {
        "title": title,
        "dkey": dkey,
        "uris": ["magnet:x"] * count,
        "source_display": "s",
    }


def meta(display, count):
    return {
        "display": display,
        "count": count,
        "path": f"/tmp/{display}.json",
        "url": "https://example.com/x.json",
        "updated": 1.0,
    }


class SourceLibraryTests(unittest.TestCase):
    def test_add_source_returns_delta_and_accumulates(self):
        library = SourceLibrary()
        delta = library.add_source("a.json", [item("A", "a"), item("B", "b")], meta("alpha", 2))
        self.assertIsInstance(delta, SourceDelta)
        self.assertEqual(delta.name, "a.json")
        self.assertEqual(delta.meta["display"], "alpha")
        self.assertEqual((delta.items_count, delta.sources_count, delta.total_groups), (2, 1, 2))
        self.assertEqual(len(library.items), 2)
        self.assertEqual(library.total_groups, 2)

    def test_duplicate_keys_across_sources_count_once(self):
        library = SourceLibrary()
        library.add_source("a.json", [item("Same", "same")], meta("alpha", 1))
        delta = library.add_source(
            "b.json", [item("Same", "same"), item("Other", "other")], meta("beta", 2)
        )
        self.assertEqual(delta.items_count, 3)
        self.assertEqual(delta.sources_count, 2)
        self.assertEqual(library.total_groups, 2)

    def test_reset_clears_everything(self):
        library = SourceLibrary()
        library.add_source("a.json", [item("A", "a")], meta("alpha", 1))
        library.reset()
        self.assertEqual(library.items, [])
        self.assertEqual(library.metas, {})
        self.assertEqual(library.total_groups, 0)
        delta = library.add_source("b.json", [item("A", "a")], meta("beta", 1))
        self.assertEqual(delta.items_count, 1)
        self.assertEqual(delta.total_groups, 1)

    def test_source_meta_lookup(self):
        library = SourceLibrary()
        library.add_source("a.json", [], meta("alpha", 0))
        self.assertEqual(library.source_meta("a.json")["display"], "alpha")
        self.assertIsNone(library.source_meta("missing"))


if __name__ == "__main__":
    unittest.main()
