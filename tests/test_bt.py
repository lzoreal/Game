import os
import sys
import unittest

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import bt_clients


class FakeProbe:
    def __init__(self, existing=(), environment=None, app_paths=None, handler=None, thunder=None):
        self.existing = {os.path.normcase(os.path.abspath(path)) for path in existing}
        self.env = {
            "ProgramFiles": r"C:\Program Files",
            "ProgramFiles(x86)": r"C:\Program Files (x86)",
            "LOCALAPPDATA": r"C:\Users\Test\AppData\Local",
            "APPDATA": r"C:\Users\Test\AppData\Roaming",
        }
        if environment:
            self.env.update(environment)
        self.app_paths = app_paths or {}
        self.handler = handler
        self.thunder = thunder

    def environment(self):
        return self.env

    def exists(self, path):
        if not path:
            return False
        return os.path.normcase(os.path.abspath(path)) in self.existing

    def app_path(self, exe_name):
        return self.app_paths.get(exe_name)

    def magnet_handler(self):
        return self.handler

    def thunder_path(self):
        return self.thunder


QBITTORRENT = r"C:\Program Files\qBittorrent\qbittorrent.exe"
THUNDER = r"C:\Thunder\Thunder.exe"
QUARK = r"C:\Quark\quark.exe"


class CatalogTests(unittest.TestCase):
    def test_catalog_consistency(self):
        keys = [entry["key"] for entry in bt_clients.CATALOG]
        self.assertEqual(len(keys), len(set(keys)))
        self.assertEqual(
            keys,
            [
                "qbittorrent",
                "bitcomet",
                "transmission",
                "deluge",
                "utorrent",
                "tixati",
                "biglybt",
                "thunder",
            ],
        )
        for entry in bt_clients.CATALOG:
            for field in ("key", "name", "badge", "color", "desc", "url"):
                self.assertTrue(entry.get(field), f"{entry.get('key')} missing {field}")
            self.assertIn(entry.get("args"), ("uri", "none"))
            self.assertIn(entry.get("fallback"), (None, "shell"))
            if entry.get("thunder"):
                self.assertTrue(entry.get("thunder"))
                self.assertEqual(entry["args"], "none")
                self.assertEqual(entry["fallback"], "shell")
            else:
                self.assertTrue(entry.get("exe_name"))
                self.assertTrue(entry.get("paths"))
                self.assertEqual(entry["args"], "uri")
                self.assertIsNone(entry["fallback"])
        self.assertTrue(bt_clients.CATALOG[0].get("recommended"))


class DetectionTests(unittest.TestCase):
    def test_catalog_order_and_system_last(self):
        probe = FakeProbe(
            existing=(QBITTORRENT, THUNDER),
            handler={"exe": QUARK, "command": f'"{QUARK}" "%1"'},
            thunder=THUNDER,
        )
        targets = bt_clients.detect(probe)
        self.assertEqual([target["key"] for target in targets], ["qbittorrent", "thunder", "__system__"])
        self.assertEqual([target["kind"] for target in targets], ["client", "client", "system"])
        self.assertEqual(targets[-1]["name"], "quark")

    def test_default_handler_is_not_duplicated_when_it_is_a_client(self):
        probe = FakeProbe(
            existing=(QBITTORRENT,),
            handler={"exe": QBITTORRENT, "command": f'"{QBITTORRENT}" "%1"'},
        )
        targets = bt_clients.detect(probe)
        self.assertEqual([target["key"] for target in targets], ["qbittorrent"])
        self.assertFalse(any(target["kind"] == "system" for target in targets))

    def test_no_targets(self):
        probe = FakeProbe()
        self.assertEqual(bt_clients.detect(probe), [])

    def test_app_path_fallback(self):
        probe = FakeProbe(
            existing=(),
            app_paths={"qbittorrent.exe": r"C:\Portable\qbittorrent.exe"},
        )
        targets = bt_clients.detect(probe)
        self.assertEqual(len(targets), 1)
        self.assertEqual(targets[0]["key"], "qbittorrent")
        self.assertEqual(targets[0]["exe"], r"C:\Portable\qbittorrent.exe")


class PlanTests(unittest.TestCase):
    def test_qbittorrent_plan_uses_uri_argument(self):
        target = {"key": "qbittorrent", "name": "qBittorrent", "kind": "client", "exe": QBITTORRENT}
        result = bt_clients.plan("magnet:?xt=urn:btih:abc", target)
        self.assertEqual(result["kind"], "process")
        self.assertEqual(result["exe"], QBITTORRENT)
        self.assertEqual(result["args"], ["magnet:?xt=urn:btih:abc"])
        self.assertEqual(result["clipboard"], "magnet:?xt=urn:btih:abc")
        self.assertIsNone(result["fallback"])

    def test_thunder_plan_is_no_arg_with_shell_fallback(self):
        target = {"key": "thunder", "name": "迅雷", "kind": "client", "exe": THUNDER}
        result = bt_clients.plan("magnet:?xt=urn:btih:def", target)
        self.assertEqual(result["kind"], "process")
        self.assertEqual(result["args"], [])
        self.assertEqual(result["clipboard"], "magnet:?xt=urn:btih:def")
        self.assertEqual(result["fallback"], {"kind": "shell", "clipboard": "magnet:?xt=urn:btih:def"})

    def test_system_plan_is_shell(self):
        target = {"key": "__system__", "name": "quark", "kind": "system", "exe": QUARK}
        result = bt_clients.plan("magnet:?xt=urn:btih:ghi", target)
        self.assertEqual(result["kind"], "shell")
        self.assertEqual(result["clipboard"], "magnet:?xt=urn:btih:ghi")

    def test_unknown_target_has_no_plan(self):
        self.assertIsNone(bt_clients.plan("magnet:x", None))
        self.assertIsNone(bt_clients.plan("magnet:x", {"key": "unknown", "kind": "client"}))


if __name__ == "__main__":
    unittest.main()
