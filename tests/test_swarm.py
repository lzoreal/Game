import base64
import os
import socket
import struct
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from urllib.parse import quote, unquote_to_bytes, urlparse

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

import swarm_stats as swarm

INFO_HASH = bytes.fromhex("0123456789abcdef0123456789abcdef01234567")


def magnet(info_hash=INFO_HASH, trackers=(), name=None):
    parts = [f"magnet:?xt=urn:btih:{info_hash.hex()}"]
    if name:
        parts.append("dn=" + quote(name))
    for tracker in trackers:
        parts.append("tr=" + quote(tracker, safe=""))
    return "&".join(parts)


def bencoded_counts(seeders, leechers, completed=0):
    return (
        b"d8:completei" + str(seeders).encode() + b"e"
        b"10:downloadedi" + str(completed).encode() + b"e"
        b"10:incompletei" + str(leechers).encode() + b"e"
        b"5:peers0:e"
    )


def start_http_tracker(seeders=0, leechers=0, completed=0, failure=None):
    calls = []

    class Handler(BaseHTTPRequestHandler):
        def do_GET(self):
            parsed = urlparse(self.path)
            fields = {}
            for part in parsed.query.split("&"):
                key, _, value = part.partition("=")
                fields.setdefault(key, []).append(value)
            raw_hash = fields.get("info_hash", [""])[0]
            calls.append((parsed.path, unquote_to_bytes(raw_hash)))
            if failure:
                body = b"d14:failure reason" + str(len(failure)).encode() + b":" + failure.encode() + b"e"
                status = 200
            else:
                body = bencoded_counts(seeders, leechers, completed)
                status = 200
            self.send_response(status)
            self.send_header("Content-Type", "text/plain")
            self.send_header("Content-Length", str(len(body)))
            self.end_headers()
            self.wfile.write(body)

        def log_message(self, format, *args):
            pass

    server = ThreadingHTTPServer(("127.0.0.1", 0), Handler)
    threading.Thread(target=server.serve_forever, daemon=True).start()
    return server, calls


class FakeUdpTracker(threading.Thread):
    def __init__(self, seeders, leechers, completed=0):
        super().__init__(daemon=True)
        self.seeders = seeders
        self.leechers = leechers
        self.completed = completed
        self.received_hash = None
        self.sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.sock.bind(("127.0.0.1", 0))
        self.port = self.sock.getsockname()[1]
        self.connection_id = 0x1122334455667788

    def run(self):
        self.sock.settimeout(5)
        try:
            connect, addr = self.sock.recvfrom(2048)
            _, txid = struct.unpack(">II", connect[8:16])
            self.sock.sendto(struct.pack(">IIQ", 0, txid, self.connection_id), addr)

            announce, addr = self.sock.recvfrom(2048)
            _, txid = struct.unpack(">II", announce[8:16])
            self.received_hash = announce[16:36]
            response = struct.pack(
                ">IIIII", 1, txid, 1800, self.leechers, self.seeders
            )
            self.sock.sendto(response, addr)
        except (socket.timeout, OSError):
            pass

    def close(self):
        self.sock.close()


class ParseTests(unittest.TestCase):
    def test_extracts_hex_info_hash_and_trackers(self):
        uri = magnet(
            trackers=[
                "http://tracker.example/announce",
                "udp://tracker.example:6969/announce",
            ]
        )
        parsed = swarm.parse_magnet(uri)
        self.assertEqual(parsed.info_hash, INFO_HASH)
        self.assertEqual(
            parsed.trackers,
            (
                "http://tracker.example/announce",
                "udp://tracker.example:6969/announce",
            ),
        )

    def test_accepts_base32_info_hash(self):
        encoded = base64.b32encode(INFO_HASH).decode().lower()
        parsed = swarm.parse_magnet(f"magnet:?xt=urn:btih:{encoded}")
        self.assertEqual(parsed.info_hash, INFO_HASH)

    def test_rejects_non_magnets_and_missing_hash(self):
        with self.assertRaises(swarm.MagnetError):
            swarm.parse_magnet("https://example.com/game")
        with self.assertRaises(swarm.MagnetError):
            swarm.parse_magnet("magnet:?dn=no-hash")


class BencodeTests(unittest.TestCase):
    def test_decodes_nested_structures(self):
        self.assertEqual(
            swarm.bdecode(b"d8:completei5e10:incompletei2ee"),
            {b"complete": 5, b"incomplete": 2},
        )
        self.assertEqual(swarm.bdecode(b"l4:spam3:eggi3ee"), [b"spam", b"egg", 3])


class HttpTrackerTests(unittest.TestCase):
    def test_announce_reads_counts_and_sends_binary_hash(self):
        server, calls = start_http_tracker(seeders=12, leechers=3, completed=45)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        url = f"http://127.0.0.1:{server.server_port}/announce"

        stats = swarm.announce_http(url, INFO_HASH, timeout=3)

        self.assertEqual((stats.seeders, stats.leechers, stats.completed), (12, 3, 45))
        self.assertEqual(len(calls), 1)
        self.assertEqual(calls[0], ("/announce", INFO_HASH))

    def test_failure_reason_becomes_tracker_error(self):
        server, _ = start_http_tracker(failure="unregistered torrent")
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        url = f"http://127.0.0.1:{server.server_port}/announce"

        with self.assertRaises(swarm.TrackerError) as ctx:
            swarm.announce_http(url, INFO_HASH, timeout=3)
        self.assertIn("unregistered torrent", str(ctx.exception))


class UdpTrackerTests(unittest.TestCase):
    def test_announce_reads_counts(self):
        tracker = FakeUdpTracker(seeders=9, leechers=4)
        tracker.start()
        self.addCleanup(tracker.close)
        url = f"udp://127.0.0.1:{tracker.port}/announce"

        stats = swarm.announce_udp(url, INFO_HASH, timeout=3)

        self.assertEqual((stats.seeders, stats.leechers), (9, 4))
        self.assertIsNone(stats.completed)
        self.assertEqual(tracker.received_hash, INFO_HASH)


class QuerySwarmTests(unittest.TestCase):
    def setUp(self):
        swarm.clear_cache()

    def test_takes_max_counts_across_trackers(self):
        first, _ = start_http_tracker(seeders=10, leechers=2, completed=20)
        second, _ = start_http_tracker(seeders=7, leechers=5, completed=50)
        self.addCleanup(first.server_close)
        self.addCleanup(first.shutdown)
        self.addCleanup(second.server_close)
        self.addCleanup(second.shutdown)
        uri = magnet(
            trackers=[
                f"http://127.0.0.1:{first.server_port}/announce",
                f"http://127.0.0.1:{second.server_port}/announce",
            ]
        )

        stats = swarm.query_swarm(uri, timeout=3)

        self.assertEqual((stats.seeders, stats.leechers, stats.completed), (10, 5, 50))
        self.assertEqual((stats.trackers_ok, stats.trackers_total), (2, 2))
        self.assertEqual(stats.errors, ())

    def test_keeps_good_data_when_a_tracker_fails(self):
        good, _ = start_http_tracker(seeders=7, leechers=1)
        self.addCleanup(good.server_close)
        self.addCleanup(good.shutdown)
        probe = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
        probe.bind(("127.0.0.1", 0))
        dead_port = probe.getsockname()[1]
        probe.close()
        uri = magnet(
            trackers=[
                f"http://127.0.0.1:{good.server_port}/announce",
                f"http://127.0.0.1:{dead_port}/announce",
            ]
        )

        stats = swarm.query_swarm(uri, timeout=0.5)

        self.assertEqual(stats.seeders, 7)
        self.assertEqual(stats.trackers_ok, 1)
        self.assertEqual(stats.trackers_total, 2)
        self.assertEqual(len(stats.errors), 1)

    def test_caches_results_for_the_ttl(self):
        server, calls = start_http_tracker(seeders=4, leechers=1)
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        uri = magnet(trackers=[f"http://127.0.0.1:{server.server_port}/announce"])

        first = swarm.query_swarm(uri, timeout=3, cache_ttl=120, now=1000.0)
        second = swarm.query_swarm(uri, timeout=3, cache_ttl=120, now=1100.0)
        third = swarm.query_swarm(uri, timeout=3, cache_ttl=120, now=1201.0)

        self.assertIs(first, second)
        self.assertIsNot(first, third)
        self.assertEqual(len(calls), 2)

    def test_reports_missing_trackers(self):
        stats = swarm.query_swarm(magnet())
        self.assertIsNone(stats.seeders)
        self.assertEqual(stats.trackers_total, 0)
        self.assertEqual(len(stats.errors), 1)


class FormatTests(unittest.TestCase):
    def test_formats_counts_and_tracker_summary(self):
        stats = swarm.SwarmStats(
            seeders=12,
            leechers=4,
            completed=231,
            trackers_ok=2,
            trackers_total=3,
            errors=("udp://tracker.example: timeout",),
        )
        self.assertEqual(swarm.format_stats(stats), "做种 12 · 下载 4 · 完成 231")
        summary = swarm.describe_stats(stats)
        self.assertIn("2/3", summary)
        self.assertIn("udp://tracker.example", summary)


if __name__ == "__main__":
    unittest.main()
