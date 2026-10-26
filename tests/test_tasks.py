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


def search_item(title, source="test", date="2025-01-01", uri="magnet:?xt=urn:btih:test"):
    return {
        "title": title,
        "nt": hydra_core.normalize_text(title),
        "dkey": hydra_core.dedup_key(title),
        "source_display": source,
        "uris": [uri],
        "fileSize": "1 GB",
        "uploadDate": date,
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


class LatestTaskTests(unittest.TestCase):
    def test_returns_newest_groups_first_and_limits(self):
        items = [
            search_item("Older Game", date="2024-01-01", uri="magnet:older"),
            search_item("Newest Game", date="2025-05-01", uri="magnet:newest"),
            search_item("Middle Game", date="2024-11-01", uri="magnet:middle"),
        ]
        groups, total = hydra_tasks.latest_sources(items, "", 2)
        self.assertEqual(total, 3)
        self.assertEqual([group["title"] for group in groups], ["Newest Game", "Middle Game"])
        self.assertEqual(groups[0]["date_dt"].year, 2025)

    def test_groups_duplicates_and_uses_newest_date(self):
        items = [
            search_item("Same Game", date="2024-01-01", uri="magnet:a"),
            search_item("Same Game", date="2025-02-02", uri="magnet:b"),
        ]
        groups, total = hydra_tasks.latest_sources(items)
        self.assertEqual(total, 1)
        self.assertEqual(len(groups), 1)
        self.assertEqual(len(groups[0]["uris"]), 2)
        self.assertEqual(groups[0]["date_dt"].month, 2)

    def test_filters_by_source_and_keeps_undated_last(self):
        items = [
            search_item("Alpha One", source="alpha", date="2024-01-01"),
            search_item("Beta One", source="beta", date="2025-01-01"),
            search_item("Alpha Undated", source="alpha", date="N/A"),
        ]
        groups, total = hydra_tasks.latest_sources(items, "alpha")
        self.assertEqual(total, 2)
        self.assertEqual(
            [group["title"] for group in groups], ["Alpha One", "Alpha Undated"]
        )

    def test_default_limit_is_one_hundred(self):
        items = [
            search_item("Game %03d" % index, date="2025-01-%02d" % (index % 28 + 1))
            for index in range(120)
        ]
        groups, total = hydra_tasks.latest_sources(items)
        self.assertEqual(total, 120)
        self.assertEqual(len(groups), 100)


if __name__ == "__main__":
    unittest.main()
