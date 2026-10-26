import os
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import hydra_core
import hydra_tasks


def write_source(path, title, uri):
    data = {"downloads": [{"title": title, "uris": [uri]}]}
    hydra_core.write_source_file(path, data)
    return data


def search_item(title, source="test"):
    return {
        "title": title,
        "nt": hydra_core.normalize_text(title),
        "dkey": hydra_core.dedup_key(title),
        "source_display": source,
        "uris": ["magnet:?xt=urn:btih:test"],
        "fileSize": "1 GB",
        "uploadDate": "2025-01-01",
    }


class TaskTestBase(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="gamemagnet_task_test_")

    def tearDown(self):
        shutil.rmtree(self.tmp, ignore_errors=True)


class LoadTaskTests(TaskTestBase):
    def test_load_streams_gzip_sources_and_returns_errors(self):
        sources = os.path.join(self.tmp, "sources")
        os.makedirs(sources)
        write_source(os.path.join(sources, "a.json.gz"), "Alpha", "magnet:a")
        write_source(os.path.join(sources, "b.json.gz"), "Beta", "magnet:b")
        events = []
        (errors,) = hydra_tasks.load_sources(
            sources, lambda event, payload: events.append((event, payload))
        )
        self.assertEqual(errors, [])
        statuses = [payload for event, payload in events if event == "status"]
        source_events = [payload for event, payload in events if event == "source"]
        self.assertEqual([payload["index"] for payload in statuses], [1, 2])
        self.assertEqual([payload["total"] for payload in statuses], [2, 2])
        self.assertEqual(
            [payload[0] for payload in source_events], ["a.json.gz", "b.json.gz"]
        )
        self.assertEqual([len(payload[1]) for payload in source_events], [1, 1])

    def test_load_reports_broken_source_and_continues(self):
        sources = os.path.join(self.tmp, "sources")
        os.makedirs(sources)
        write_source(os.path.join(sources, "a.json.gz"), "Alpha", "magnet:a")
        with open(os.path.join(sources, "bad.json"), "w", encoding="utf-8") as handle:
            handle.write("{not json")
        events = []
        (errors,) = hydra_tasks.load_sources(
            sources, lambda event, payload: events.append((event, payload))
        )
        self.assertEqual(len(errors), 1)
        self.assertIn("bad.json", errors[0])
        source_events = [payload for event, payload in events if event == "source"]
        self.assertEqual([payload[0] for payload in source_events], ["a.json.gz"])

class SearchTaskTests(unittest.TestCase):
    def test_search_returns_groups(self):
        items = [
            search_item("Cyberpunk 2077"),
            search_item("Cyberpunk 2077"),
            search_item("Vampire Survivors"),
        ]
        groups, total = hydra_tasks.search_sources(
            items, "cyberpunk", True, 0.3, ""
        )
        self.assertEqual(total, 1)
        self.assertEqual(len(groups), 1)
        self.assertEqual(groups[0]["title"], "Cyberpunk 2077")


if __name__ == "__main__":
    unittest.main()
