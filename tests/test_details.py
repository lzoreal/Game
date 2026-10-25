import json
import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import hydra_core as core
from hydra_details import build_detail, card_key


def item(source="alpha", uris=None, date="2025-01-02", description=""):
    return {
        "title": "Game",
        "nt": core.normalize_text("Game"),
        "dkey": core.dedup_key("Game"),
        "uris": list(uris or []),
        "fileSize": "12 GB",
        "uploadDate": date,
        "source_name": source,
        "source_display": source,
        "raw": {"title": "Game", "descriptionHtml": description},
    }


def group(items):
    sources = []
    uris = []
    seen = set()
    for entry in items:
        if entry["source_display"] not in sources:
            sources.append(entry["source_display"])
        for uri in entry["uris"]:
            if uri not in seen:
                seen.add(uri)
                uris.append(uri)
    return {
        "title": "Game",
        "sources": sources,
        "size": "12 GB",
        "date_display": "2025-01-02",
        "link_count": len(uris),
        "items": items,
    }


class DetailModelTests(unittest.TestCase):
    def test_builds_header_and_hint(self):
        model = build_detail(group([item("alpha", ["magnet:a"]), item("beta", ["http://x"])]))
        self.assertEqual(model.title, "Game")
        self.assertEqual(model.sources, ("alpha", "beta"))
        self.assertEqual(model.meta, "2025-01-02   ·   12 GB   ·   2 条链接")
        self.assertEqual(model.hint, "2 个来源")
        self.assertEqual(len(model.cards), 2)
        self.assertFalse(model.raw_visible)
        self.assertEqual(model.raw_text, "")
        self.assertEqual(model.raw_label, "查看原始 JSON")

    def test_classifies_and_limits_links(self):
        magnets = [f"magnet:{index}" for index in range(8)]
        entry = item(uris=magnets + ["http://a", "http://b", "ftp://c"])
        collapsed = build_detail(group([entry]))
        groups = collapsed.cards[0].links

        self.assertEqual([link_group.label for link_group in groups], ["磁力链接", "HTTP 链接", "其他链接"])
        self.assertEqual((groups[0].total, len(groups[0].visible), groups[0].hidden), (8, 6, 2))
        self.assertTrue(groups[0].can_expand)
        self.assertEqual((groups[1].total, groups[1].hidden), (2, 0))
        self.assertFalse(groups[1].can_expand)
        self.assertFalse(groups[2].visible[0].is_magnet)

        expanded = build_detail(group([entry]), expanded={card_key(entry)})
        self.assertEqual(len(expanded.cards[0].links[0].visible), 8)
        self.assertEqual(expanded.cards[0].links[0].hidden, 0)
        self.assertFalse(expanded.cards[0].links[0].can_expand)

    def test_description_and_date(self):
        entry = item(date="2025-01-02T12:00:00Z", description="<p>Hello <b>world</b></p>")
        card = build_detail(group([entry])).cards[0]
        self.assertEqual(card.description, "Hello world")
        self.assertEqual(card.date_text, "2025-01-02")

        undated = build_detail(group([item(date="N/A")])).cards[0]
        self.assertEqual(undated.date_text, "N/A")

    def test_keys_are_stable_and_unique(self):
        first = item("alpha", [f"magnet:a{index}" for index in range(8)])
        second = item("alpha", [f"magnet:b{index}" for index in range(8)])
        model = build_detail(group([first, second]))
        keys = [card.key for card in model.cards]
        self.assertEqual(len(keys), 2)
        self.assertEqual(len(set(keys)), 2)
        self.assertEqual(keys[0], card_key(first))
        self.assertTrue(keys[1].startswith(keys[0]))
        self.assertEqual(keys, [card.key for card in build_detail(group([first, second])).cards])

        expanded = build_detail(group([first, second]), expanded={keys[1]})
        self.assertEqual(len(expanded.cards[0].links[0].visible), 6)
        self.assertEqual(len(expanded.cards[1].links[0].visible), 8)

    def test_raw_text_can_be_truncated(self):
        entries = [item(uris=["magnet:a"])]
        full = build_detail(group(entries), raw_visible=True)
        self.assertEqual(full.raw_label, "收起原始 JSON")
        self.assertEqual(json.loads(full.raw_text), [{"title": "Game", "descriptionHtml": ""}])

        clipped = build_detail(group(entries), raw_visible=True, raw_limit=10)
        self.assertEqual(
            clipped.raw_text,
            full.raw_text[:10] + "\n\n...（内容过长，已截断）",
        )


if __name__ == "__main__":
    unittest.main()
