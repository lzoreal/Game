import json
import os
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import update_check as updates


def release_payload(tag="v4.1", assets=None, body="Release notes", name="GameMagNet 4.1"):
    return {
        "tag_name": tag,
        "name": name,
        "html_url": f"https://github.com/lzoreal/Game/releases/tag/{tag}",
        "body": body,
        "published_at": "2026-10-20T12:00:00Z",
        "assets": assets
        if assets is not None
        else [
            {"browser_download_url": "https://example.com/GameMagNet.exe"},
            {"browser_download_url": "https://example.com/GameMagNet-windows.zip"},
        ],
    }


def start_api(payload, status=200):
    calls = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            calls.append(
                (
                    self.path,
                    self.headers.get("Authorization", ""),
                    self.headers.get("Accept", ""),
                )
            )
            body = json.dumps(payload).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, format, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, calls


class VersionTests(unittest.TestCase):
    def test_parse_version_handles_prefix_and_segments(self):
        self.assertEqual(updates.parse_version("v4.0"), (4, 0))
        self.assertEqual(updates.parse_version("4.1.2"), (4, 1, 2))
        self.assertEqual(updates.parse_version("V10"), (10,))
        self.assertEqual(updates.parse_version(""), (0,))

    def test_is_newer_compares_numeric_segments(self):
        self.assertTrue(updates.is_newer("v4.1", "v4.0"))
        self.assertTrue(updates.is_newer("v4.0.1", "v4.0"))
        self.assertFalse(updates.is_newer("v4.0", "v4.0"))
        self.assertFalse(updates.is_newer("v4.0", "v4.0.0"))
        self.assertFalse(updates.is_newer("v3.9", "v4.0"))


class CheckLatestTests(unittest.TestCase):
    def test_detects_new_release_and_prefers_exe_asset(self):
        server, calls = start_api(release_payload())
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        api_base = f"http://127.0.0.1:{server.server_port}"

        result = updates.check_latest("lzoreal/Game", "v4.0", api_base=api_base)

        self.assertTrue(result.update_available)
        self.assertEqual(result.latest, "v4.1")
        self.assertEqual(result.release.download_url, "https://example.com/GameMagNet.exe")
        self.assertEqual(result.release.url, "https://github.com/lzoreal/Game/releases/tag/v4.1")
        self.assertIn("Release notes", result.release.notes)
        self.assertEqual(
            calls[0][0], "/repos/lzoreal/Game/releases/latest"
        )
        self.assertIn("application/vnd.github+json", calls[0][2])
        self.assertEqual(calls[0][1], "")

    def test_same_version_is_not_an_update(self):
        server, _ = start_api(release_payload(tag="v4.0"))
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        api_base = f"http://127.0.0.1:{server.server_port}"

        result = updates.check_latest("lzoreal/Game", "v4.0", api_base=api_base)

        self.assertFalse(result.update_available)

    def test_token_is_sent_as_bearer_header(self):
        server, calls = start_api(release_payload(tag="v4.0"))
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        api_base = f"http://127.0.0.1:{server.server_port}"

        updates.check_latest("lzoreal/Game", "v4.0", token="abc123", api_base=api_base)

        self.assertEqual(calls[0][1], "Bearer abc123")

    def test_missing_release_has_clear_error(self):
        server, _ = start_api({"message": "Not Found"}, status=404)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        api_base = f"http://127.0.0.1:{server.server_port}"

        with self.assertRaises(updates.UpdateError) as ctx:
            updates.check_latest("lzoreal/Game", "v4.0", api_base=api_base)
        self.assertIn("未找到 Release", str(ctx.exception))

    def test_invalid_payload_is_rejected(self):
        server, _ = start_api({"message": "ok"}, status=200)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        api_base = f"http://127.0.0.1:{server.server_port}"

        with self.assertRaises(updates.UpdateError):
            updates.check_latest("lzoreal/Game", "v4.0", api_base=api_base)

    def test_invalid_repo_is_rejected_without_request(self):
        with self.assertRaises(updates.UpdateError):
            updates.check_latest("not-a-repo", "v4.0")


if __name__ == "__main__":
    unittest.main()
