"""Magnet swarm statistics from public trackers. No torrent data is downloaded."""

import base64
import gzip
import random
import re
import socket
import struct
import time
import urllib.error
import urllib.request
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass
from urllib.parse import quote, unquote_plus, urlparse

USER_AGENT = "GameMagNet/4.0"
ANNOUNCE_PORT = 6881
READ_LIMIT = 262144
CACHE_TTL = 120.0
MAX_TRACKERS = 4
UDP_MAGIC = 0x41727101980

_HEX_HASH = re.compile(r"^[0-9a-fA-F]{40}$")
_BASE32_HASH = re.compile(r"^[A-Za-z2-7]{32}$")


class MagnetError(ValueError):
    pass


class TrackerError(RuntimeError):
    pass


@dataclass(frozen=True)
class Magnet:
    info_hash: bytes
    trackers: tuple[str, ...]
    name: str = ""


@dataclass(frozen=True)
class TrackerStats:
    seeders: int | None
    leechers: int | None
    completed: int | None


@dataclass(frozen=True)
class SwarmStats:
    seeders: int | None
    leechers: int | None
    completed: int | None
    trackers_ok: int
    trackers_total: int
    errors: tuple[str, ...]


_CACHE: dict[bytes, tuple[float, SwarmStats]] = {}


def clear_cache():
    _CACHE.clear()


def parse_magnet(uri):
    if not isinstance(uri, str) or not uri.lower().startswith("magnet:?"):
        raise MagnetError("不是磁力链接")

    info_hash = None
    trackers = []
    name = ""
    for part in uri.split("?", 1)[1].split("&"):
        if not part:
            continue
        key, _, value = part.partition("=")
        key = key.lower()
        if key == "xt":
            candidate = unquote_plus(value)
            if candidate.lower().startswith("urn:btih:"):
                info_hash = _parse_info_hash(candidate[len("urn:btih:"):])
        elif key == "tr":
            tracker = unquote_plus(value).strip()
            if tracker and tracker not in trackers:
                trackers.append(tracker)
        elif key == "dn" and not name:
            name = unquote_plus(value)

    if not info_hash:
        raise MagnetError("磁力链接缺少 BTIH info_hash")
    return Magnet(info_hash=info_hash, trackers=tuple(trackers), name=name)


def _parse_info_hash(value):
    if _HEX_HASH.fullmatch(value):
        return bytes.fromhex(value)
    if _BASE32_HASH.fullmatch(value):
        try:
            return base64.b32decode(value.upper())
        except Exception as exc:
            raise MagnetError(f"无效的 info_hash: {value}") from exc
    raise MagnetError(f"无效的 info_hash: {value}")


def bdecode(data):
    if not isinstance(data, (bytes, bytearray)):
        raise TrackerError("tracker 响应不是字节串")
    value, _ = _bdecode(bytes(data), 0)
    return value


def _bdecode(data, index):
    if index >= len(data):
        raise TrackerError("bencode 数据不完整")
    marker = data[index:index + 1]

    if marker == b"i":
        end = data.find(b"e", index + 1)
        if end < 0:
            raise TrackerError("bencode 整数未结束")
        try:
            return int(data[index + 1:end]), end + 1
        except ValueError as exc:
            raise TrackerError("bencode 整数无效") from exc

    if marker == b"l":
        items = []
        index += 1
        while index < len(data) and data[index:index + 1] != b"e":
            value, index = _bdecode(data, index)
            items.append(value)
        return items, index + 1

    if marker == b"d":
        result = {}
        index += 1
        while index < len(data) and data[index:index + 1] != b"e":
            key, index = _bdecode(data, index)
            value, index = _bdecode(data, index)
            result[key] = value
        return result, index + 1

    colon = data.find(b":", index)
    if colon < 0:
        raise TrackerError("bencode 字符串长度缺失")
    try:
        length = int(data[index:colon])
    except ValueError as exc:
        raise TrackerError("bencode 字符串长度无效") from exc
    start = colon + 1
    end = start + length
    if end > len(data):
        raise TrackerError("bencode 字符串不完整")
    return data[start:end], end


def _peer_id():
    alphabet = "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
    return ("-GM4000-" + "".join(random.choices(alphabet, k=12))).encode("ascii")


def _as_count(value):
    if isinstance(value, bool) or not isinstance(value, int):
        return None
    return max(0, value)


def announce_http(url, info_hash, timeout=3.0):
    query = "&".join((
        "info_hash=" + quote(info_hash, safe=""),
        "peer_id=" + quote(_peer_id(), safe=""),
        f"port={ANNOUNCE_PORT}",
        "uploaded=0",
        "downloaded=0",
        "left=1",
        "compact=1",
        "numwant=0",
        "event=started",
    ))
    separator = "&" if "?" in url else "?"
    request = urllib.request.Request(
        url + separator + query,
        headers={"User-Agent": USER_AGENT, "Accept-Encoding": "gzip"},
    )
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            body = response.read(READ_LIMIT)
            if response.info().get("Content-Encoding") == "gzip":
                body = gzip.decompress(body)
    except (urllib.error.URLError, OSError) as exc:
        raise TrackerError(str(exc)) from exc

    payload = bdecode(body)
    if not isinstance(payload, dict):
        raise TrackerError("tracker 响应格式无效")

    failure = payload.get(b"failure reason")
    if failure:
        if isinstance(failure, bytes):
            failure = failure.decode("utf-8", "replace")
        raise TrackerError(str(failure))

    return TrackerStats(
        seeders=_as_count(payload.get(b"complete")),
        leechers=_as_count(payload.get(b"incomplete")),
        completed=_as_count(payload.get(b"downloaded")),
    )


def announce_udp(url, info_hash, timeout=3.0):
    parsed = urlparse(url)
    host = parsed.hostname
    if not host:
        raise TrackerError("UDP tracker 地址无效")
    try:
        address = socket.gethostbyname(host)
    except OSError as exc:
        raise TrackerError(f"无法解析 tracker 主机: {host}") from exc
    port = parsed.port or 6969

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(timeout)
    try:
        connect_tx = random.getrandbits(32)
        sock.sendto(
            struct.pack(">QII", UDP_MAGIC, 0, connect_tx), (address, port)
        )
        response, _ = sock.recvfrom(2048)
        if len(response) < 16:
            raise TrackerError("UDP tracker 连接响应过短")
        action, txid = struct.unpack(">II", response[:8])
        if action != 0 or txid != connect_tx:
            raise TrackerError("UDP tracker 连接响应无效")
        connection_id = struct.unpack(">Q", response[8:16])[0]

        announce_tx = random.getrandbits(32)
        packet = struct.pack(
            ">QII20s20sQQQIIIiH",
            connection_id,
            1,
            announce_tx,
            info_hash,
            _peer_id(),
            0,
            1,
            0,
            2,
            0,
            random.getrandbits(32),
            0,
            ANNOUNCE_PORT,
        )
        sock.sendto(packet, (address, port))
        response, _ = sock.recvfrom(2048)
    except socket.timeout as exc:
        raise TrackerError(f"UDP tracker 超时: {host}") from exc
    except OSError as exc:
        raise TrackerError(str(exc)) from exc
    finally:
        sock.close()

    if len(response) < 8:
        raise TrackerError("UDP tracker 应答过短")
    action, txid = struct.unpack(">II", response[:8])
    if action == 3:
        message = response[8:].decode("utf-8", "replace").strip()
        raise TrackerError(message or "UDP tracker 返回错误")
    if len(response) < 20:
        raise TrackerError("UDP tracker 应答过短")
    _, _, _interval, leechers, seeders = struct.unpack(">IIIII", response[:20])
    if action != 1 or txid != announce_tx:
        raise TrackerError("UDP tracker 应答无效")

    return TrackerStats(seeders=seeders, leechers=leechers, completed=None)


def query_swarm(uri, timeout=3.0, max_trackers=MAX_TRACKERS, cache_ttl=CACHE_TTL, now=None):
    parsed = parse_magnet(uri)
    current = time.monotonic() if now is None else now
    cached = _CACHE.get(parsed.info_hash)
    if cached and current - cached[0] < cache_ttl:
        return cached[1]

    trackers = []
    for tracker in parsed.trackers:
        if urlparse(tracker).scheme.lower() in ("http", "https", "udp"):
            if tracker not in trackers:
                trackers.append(tracker)
    trackers = trackers[:max_trackers]
    if not trackers:
        return SwarmStats(
            None, None, None, 0, 0, ("磁力链接中没有可查询的 tracker",)
        )

    def query(tracker):
        if urlparse(tracker).scheme.lower() == "udp":
            return announce_udp(tracker, parsed.info_hash, timeout)
        return announce_http(tracker, parsed.info_hash, timeout)

    seeders = []
    leechers = []
    completed = []
    errors = []
    with ThreadPoolExecutor(max_workers=min(4, len(trackers))) as pool:
        futures = {pool.submit(query, tracker): tracker for tracker in trackers}
        for future, tracker in futures.items():
            try:
                stats = future.result()
            except Exception as exc:
                errors.append(f"{tracker}: {exc}")
                continue
            if stats.seeders is not None:
                seeders.append(stats.seeders)
            if stats.leechers is not None:
                leechers.append(stats.leechers)
            if stats.completed is not None:
                completed.append(stats.completed)

    result = SwarmStats(
        seeders=max(seeders) if seeders else None,
        leechers=max(leechers) if leechers else None,
        completed=max(completed) if completed else None,
        trackers_ok=len(trackers) - len(errors),
        trackers_total=len(trackers),
        errors=tuple(errors),
    )
    if result.trackers_ok:
        _CACHE[parsed.info_hash] = (current, result)
    return result


def format_stats(stats):
    parts = []
    if stats.seeders is not None:
        parts.append(f"做种 {stats.seeders:,}")
    if stats.leechers is not None:
        parts.append(f"下载 {stats.leechers:,}")
    if stats.completed is not None:
        parts.append(f"完成 {stats.completed:,}")
    return " · ".join(parts) or "暂无数据"


def describe_stats(stats):
    summary = (
        f"{format_stats(stats)}"
        f"（{stats.trackers_ok}/{stats.trackers_total} 个 tracker 返回数据）"
    )
    if stats.errors:
        summary += "\n" + "\n".join(stats.errors[:3])
    return summary
