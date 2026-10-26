"""Core logic for the local game catalog manager: gzip storage, parsing, search.

No GUI imports, so it can be shared by the PySide6 UI and Qt worker threads.
"""

import gzip
import html
import json
import os
import re
import ssl
import unicodedata
import urllib.error
import urllib.request
from datetime import datetime
from difflib import SequenceMatcher
from functools import lru_cache
from urllib.parse import urlparse

ssl._create_default_https_context = ssl._create_unverified_context

APP_VERSION = "v1.0"
MAX_RESULTS = 1000
LATEST_LIMIT = 100
SOURCE_URL_SUFFIX = ".source-url"

DEFAULT_HEADERS = {
    'User-Agent': 'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Safari/537.36',
    'Accept': 'application/json, text/plain, */*',
    'Accept-Language': 'zh-CN,zh;q=0.9,en;q=0.8',
    'Accept-Encoding': 'gzip, deflate, br',
    'Connection': 'keep-alive',
    'Sec-Fetch-Dest': 'empty',
    'Sec-Fetch-Mode': 'cors',
    'Sec-Fetch-Site': 'same-origin',
}

REPACK_KEYS = ['repacks', 'repack', 'games', 'items', 'downloads', 'sources']

# Cloudflare-protected mirrors reject the default Python TLS fingerprint. When
# ``curl_cffi`` is available we retry with real browser fingerprints so source
# updates and downloads keep working without user interaction.
IMPERSONATE_PROFILES = ("chrome", "safari", "firefox")
CF_STATUS_CODES = (403, 429, 503)
CF_BODY_MARKERS = (
    "just a moment",
    "checking your browser",
    "cf-chl",
    "cf_chl_opt",
    "attention required",
    "cf-mitigated",
    "enable javascript and cookies to continue",
    "请稍候",
    "正在验证您是否是真人",
    "验证您是人类",
)

_CURL_CFFI = None
_CURL_CFFI_CHECKED = False


def curl_requests():
    """Return ``curl_cffi.requests`` when installed, else ``None``."""
    global _CURL_CFFI, _CURL_CFFI_CHECKED
    if not _CURL_CFFI_CHECKED:
        _CURL_CFFI_CHECKED = True
        try:
            from curl_cffi import requests as module
        except Exception:
            module = None
        _CURL_CFFI = module
    return _CURL_CFFI


def is_cloudflare_challenge(status, headers, body):
    """Best-effort detection of a Cloudflare interstitial/challenge response."""
    if int(status or 0) not in CF_STATUS_CODES:
        return False
    header_text = " ".join(
        f"{key}:{value}" for key, value in (headers or {}).items()
    ).lower()
    if "cf-mitigated" in header_text:
        return True
    if isinstance(body, (bytes, bytearray)):
        snippet = body[:8192].decode("utf-8", "ignore")
    else:
        snippet = str(body or "")[:8192]
    snippet = snippet.lower()
    return any(marker in snippet for marker in CF_BODY_MARKERS)


CF_BLOCKED_HINT = (
    "需要执行 JavaScript 的验证无法自动绕过：可在浏览器中打开该地址，"
    "保存返回的 JSON 后通过「从本地导入 JSON」导入；或改用其他镜像地址。"
)


class CloudflareBlocked(RuntimeError):
    """Raised when a Cloudflare challenge cannot be solved without a browser."""

    def __init__(self, message, url=""):
        super().__init__(message)
        self.url = url


_VERSION_RE = re.compile(r"\bv\d+(?:\.\d+)+\w*\b", re.I)
_ORDINAL_RE = re.compile(r"(\d+)(?:st|nd|rd|th)\b", re.I)

DATE_FORMATS = (
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M",
    "%Y-%m-%d",
    "%Y/%m/%d %H:%M:%S",
    "%Y/%m/%d %H:%M",
    "%Y/%m/%d",
    "%Y.%m.%d %H:%M:%S",
    "%Y.%m.%d %H:%M",
    "%Y.%m.%d",
    "%d-%m-%Y %H:%M:%S",
    "%d-%m-%Y %H:%M",
    "%d-%m-%Y",
    "%d/%m/%Y %H:%M:%S",
    "%d/%m/%Y %H:%M",
    "%d/%m/%Y",
    "%B %d, %Y",
    "%b %d, %Y",
    "%d %B %Y",
    "%d %b %Y",
    "%B %d %Y",
    "%b %d %Y",
)


def normalize_text(value):
    if value is None:
        return ""
    s = unicodedata.normalize("NFKC", str(value)).lower()
    s = "".join(ch if ch.isalnum() else " " for ch in s)
    return " ".join(s.split())


def dedup_key(title):
    s = unicodedata.normalize("NFKC", str(title or "")).lower()
    s = _VERSION_RE.sub(" ", s)
    s = "".join(ch if ch.isalnum() else " " for ch in s)
    return " ".join(s.split()) or normalize_text(title)


def display_name(filename):
    lower = filename.lower()
    if lower.endswith(".json.gz"):
        stem = filename[:-8]
    elif lower.endswith(".json"):
        stem = filename[:-5]
    else:
        stem = filename
    stem = stem.lower()
    for suffix in ("_cleaned", "-cleaned", "_source", "-source", "source", "-games", "_games"):
        if stem.endswith(suffix):
            stem = stem[: -len(suffix)]
            break
    return stem or filename


def clean_html(text):
    if not text:
        return ""
    t = re.sub(r"<br\s*/?>", "\n", text, flags=re.I)
    t = re.sub(r"</p\s*>", "\n\n", t, flags=re.I)
    t = re.sub(r"<[^>]+>", " ", t)
    t = html.unescape(t)
    t = re.sub(r"[ \t]+", " ", t)
    t = re.sub(r"\n{3,}", "\n\n", t)
    return t.strip()


def parse_upload_date(value):
    if isinstance(value, str):
        return _parse_date_cached(value)
    return _parse_upload_date(value)


@lru_cache(maxsize=65536)
def _parse_date_cached(value):
    return _parse_upload_date(value)


def _parse_upload_date(value):
    if value is None:
        return None

    s = _ORDINAL_RE.sub(r"\1", str(value).strip())
    if not s or s.upper() in ("N/A", "NA", "NONE", "NULL", "-"):
        return None

    if re.fullmatch(r"\d{10,13}", s):
        try:
            ts = int(s)
            if len(s) == 13:
                ts /= 1000
            return datetime.fromtimestamp(ts)
        except (ValueError, OSError, OverflowError):
            pass

    iso = s.replace("Z", "+00:00")
    try:
        dt = datetime.fromisoformat(iso)
        if dt.tzinfo is not None:
            dt = dt.replace(tzinfo=None)
        return dt
    except ValueError:
        pass

    for fmt in DATE_FORMATS:
        try:
            return datetime.strptime(s, fmt)
        except ValueError:
            continue

    m = re.search(r"(20\d{2})[-/.](\d{1,2})[-/.](\d{1,2})", s)
    if m:
        try:
            return datetime(int(m.group(1)), int(m.group(2)), int(m.group(3)))
        except ValueError:
            pass

    return None


def fuzzy_ratio(query, title):
    q = normalize_text(query)
    t = normalize_text(title)
    if not q or not t:
        return 0.0
    return SequenceMatcher(None, q, t).ratio()


def extract_repacks(data, filename):
    for key in REPACK_KEYS:
        if key in data and isinstance(data[key], list):
            return data[key], key

    def find_first_list(obj, depth=0):
        if depth > 3:
            return None
        if isinstance(obj, list) and len(obj) > 0 and isinstance(obj[0], dict):
            sample = obj[0]
            if any(k in sample for k in ['title', 'uris', 'url', 'name', 'magnet', 'download']):
                return obj
            return None
        if isinstance(obj, dict):
            for v in obj.values():
                result = find_first_list(v, depth + 1)
                if result:
                    return result
        return None

    result = find_first_list(data)
    if result:
        return result, "auto-detected"

    if isinstance(data, list) and len(data) > 0 and isinstance(data[0], dict):
        return data, "root-list"

    return [], None


def validate_repack_item(item):
    if not isinstance(item, dict):
        return None

    title = None
    for key in ['title', 'name', 'game', 'description']:
        if key in item and isinstance(item[key], str):
            title = item[key]
            break

    uris = []
    for key in ['uris', 'uri', 'urls', 'url', 'links', 'link', 'magnet', 'torrent']:
        if key in item:
            val = item[key]
            if isinstance(val, list):
                uris.extend([str(u) for u in val if isinstance(u, str)])
            elif isinstance(val, str):
                uris.append(val)

    if not title and not uris:
        return None

    return {
        'title': title or 'Unknown',
        'uris': uris,
        'fileSize': item.get('fileSize', item.get('size', item.get('file_size', 'N/A'))),
        'uploadDate': item.get('uploadDate', item.get('date', item.get('upload_date', 'N/A'))),
        'raw': item,
    }


def finalize_group(group):
    items = group["items"]
    representative = min(items, key=lambda it: (len(it["title"]), it["title"]))
    dated = [it for it in items if parse_upload_date(it["uploadDate"]) is not None]
    newest = max(dated, key=lambda it: parse_upload_date(it["uploadDate"])) if dated else items[0]

    sources = []
    uris = []
    seen = set()
    for it in items:
        if it["source_display"] not in sources:
            sources.append(it["source_display"])
        for uri in it["uris"]:
            if uri not in seen:
                seen.add(uri)
                uris.append(uri)

    items.sort(key=lambda it: (parse_upload_date(it["uploadDate"]) or datetime.min), reverse=True)

    group["title"] = representative["title"]
    group["sources"] = sources
    group["size"] = newest["fileSize"]
    group["date_str"] = newest["uploadDate"]
    group["date_dt"] = parse_upload_date(newest["uploadDate"])
    group["date_display"] = (group["date_dt"].strftime("%Y-%m-%d")
                             if group["date_dt"] else str(newest["uploadDate"]))
    group["uris"] = uris
    group["link_count"] = len(uris)
    return group


def read_source_url(path):
    try:
        with open(path + SOURCE_URL_SUFFIX, "r", encoding="utf-8") as f:
            return f.read().strip() or None
    except OSError:
        return None


def write_url_file(path, url):
    with open(path + SOURCE_URL_SUFFIX, "w", encoding="utf-8") as f:
        f.write(url)


def source_filenames(sources_dir):
    try:
        names = os.listdir(sources_dir)
    except OSError:
        return []
    plain = {name for name in names if name.lower().endswith(".json")}
    packed = {name for name in names if name.lower().endswith(".json.gz")}
    shadowed = {name[:-3] for name in packed}
    return sorted(
        name
        for name in names
        if name in packed or (name in plain and name not in shadowed)
    )


def read_source_file(path):
    if path.lower().endswith(".gz"):
        with gzip.open(path, "rt", encoding="utf-8") as handle:
            return json.load(handle)
    with open(path, "r", encoding="utf-8") as handle:
        return json.load(handle)


def write_source_file(path, data):
    payload = json.dumps(
        data, ensure_ascii=False, separators=(",", ":")
    ).encode("utf-8")
    if path.lower().endswith(".gz"):
        payload = gzip.compress(payload, compresslevel=6, mtime=0)
    tmp = path + ".tmp"
    with open(tmp, "wb") as handle:
        handle.write(payload)
    os.replace(tmp, path)


def load_source(sources_dir, name):
    path = os.path.join(sources_dir, name)
    stat = os.stat(path)
    url = read_source_url(path)
    display = display_name(name)

    data = read_source_file(path)

    raw_items, detected_key = extract_repacks(data, name)
    if not raw_items:
        keys = list(data.keys()) if isinstance(data, dict) else type(data).__name__
        raise ValueError(f"无法找到游戏列表字段。JSON 顶层字段: {keys}")

    items = []
    invalid = 0
    for raw in raw_items:
        norm = validate_repack_item(raw)
        if not norm:
            invalid += 1
            continue
        title = norm["title"]
        items.append({
            "title": title,
            "nt": normalize_text(title),
            "dkey": dedup_key(title),
            "uris": norm["uris"],
            "fileSize": norm["fileSize"],
            "uploadDate": norm["uploadDate"],
            "source_name": name,
            "source_display": display,
            "raw": raw,
        })

    if not url and isinstance(data, dict):
        fallback = data.get("url")
        if isinstance(fallback, str):
            url = fallback.strip() or None

    return items, {
        "display": display,
        "count": len(items),
        "url": url,
        "path": path,
        "updated": stat.st_mtime,
        "detected_key": detected_key,
        "invalid_count": invalid,
    }


def stream_sources(sources_dir, progress=None, on_source=None):
    """Load every source, handing each one to on_source as soon as it is ready.

    Returns the list of per-source error messages. The caller owns accumulation;
    this function never keeps the items it has already delivered.
    """
    names = source_filenames(sources_dir)

    errors = []
    total = len(names)
    for index, name in enumerate(names, 1):
        if progress:
            progress(index, total, name)
        try:
            source_items, meta = load_source(sources_dir, name)
        except Exception as exc:
            errors.append(f"{name}: {exc}")
            continue
        url = meta.get("url")
        for item in source_items:
            item["url"] = url
        if on_source:
            on_source(name, source_items, meta)
    return errors


def search_items(items, query, fuzzy, min_similarity, source_filter, limit=MAX_RESULTS):
    q = normalize_text(query)
    if not q:
        return [], 0

    groups = {}
    order = []
    substring_hits = 0

    def add(item, score):
        key = item["dkey"] or item["nt"]
        group = groups.get(key)
        if group is None:
            group = {"items": [], "score": 0.0}
            groups[key] = group
            order.append(key)
        group["items"].append(item)
        if score > group["score"]:
            group["score"] = score

    for item in items:
        if source_filter and item["source_display"] != source_filter:
            continue
        nt = item["nt"]
        pos = nt.find(q)
        if pos >= 0:
            if nt == q:
                score = 1.0
            else:
                coverage = len(q) / max(1, len(nt))
                score = 0.9 + 0.09 * coverage + (0.005 if pos == 0 else 0.0)
            add(item, score)
            substring_hits += 1

    if fuzzy and substring_hits < 50:
        qcompact = q.replace(" ", "")
        head = qcompact[:3]
        qchars = set(qcompact)
        min_shared = max(1, int(len(qchars) * 0.7))
        for item in items:
            if source_filter and item["source_display"] != source_filter:
                continue
            nt = item["nt"]
            if q in nt:
                continue
            if len(qcompact) >= 3 and head not in nt:
                continue
            tchars = set(nt.replace(" ", ""))
            if len(qchars & tchars) < min_shared:
                continue
            score = 0.9 * fuzzy_ratio(q, item["title"])
            if score >= min_similarity:
                add(item, score)

    total = len(order)
    ranked = sorted(order, key=lambda k: groups[k]["score"], reverse=True)
    result = [finalize_group(groups[k]) for k in ranked[:limit]]
    result.sort(key=lambda g: (g["score"], g["date_dt"] or datetime.min), reverse=True)
    return result, total


def latest_items(items, source_filter="", limit=LATEST_LIMIT):
    """Group all items and return the newest groups, newest first.

    Used as the default listing when the search box is empty. Two passes are
    used so only the newest ``limit`` groups are built and finalized, which
    keeps large catalogs responsive.
    """
    newest = {}

    for item in items:
        if source_filter and item["source_display"] != source_filter:
            continue
        key = item["dkey"] or item["nt"]
        date_dt = parse_upload_date(item["uploadDate"])
        if date_dt is None:
            if key not in newest:
                newest[key] = None
        else:
            current = newest.get(key)
            if current is None or date_dt > current:
                newest[key] = date_dt

    total = len(newest)
    keys = sorted(
        newest, key=lambda key: newest[key] or datetime.min, reverse=True
    )[:limit]
    selected = set(keys)
    groups = {key: {"items": [], "score": 0.0} for key in keys}

    for item in items:
        if source_filter and item["source_display"] != source_filter:
            continue
        key = item["dkey"] or item["nt"]
        if key in selected:
            groups[key]["items"].append(item)

    return [finalize_group(groups[key]) for key in keys], total


class Downloader:
    def download_json(self, url, timeout=30):
        headers = DEFAULT_HEADERS.copy()
        headers["Accept-Encoding"] = "gzip"
        parsed = urlparse(url)
        headers["Referer"] = f"{parsed.scheme}://{parsed.netloc}/"

        module = curl_requests()
        if module is not None:
            return self._download_impersonated(module, url, headers, timeout)
        return self._download_urllib(url, headers, timeout)

    def _download_impersonated(self, module, url, headers, timeout):
        blocked = []
        for profile in IMPERSONATE_PROFILES:
            try:
                response = module.get(
                    url,
                    headers=headers,
                    impersonate=profile,
                    timeout=timeout,
                    allow_redirects=True,
                    verify=False,
                )
            except Exception as exc:
                raise RuntimeError(f"下载失败: {exc}") from exc

            body = response.content
            if is_cloudflare_challenge(
                response.status_code, response.headers, body
            ):
                blocked.append(profile)
                continue
            if response.status_code >= 400:
                reason = response.reason or ""
                raise RuntimeError(
                    f"下载失败: HTTP {response.status_code} {reason}".strip()
                )
            try:
                return body.decode("utf-8")
            except ValueError as exc:
                raise RuntimeError(f"下载失败: 响应内容无效: {exc}") from exc

        raise CloudflareBlocked(
            "下载失败: 目标站点启用了 Cloudflare 人机验证，"
            f"已尝试 {' / '.join(blocked)} 浏览器指纹仍被拦截。"
            + CF_BLOCKED_HINT,
            url,
        )

    def _download_urllib(self, url, headers, timeout):
        request = urllib.request.Request(url, headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                content = response.read()
                if response.info().get("Content-Encoding") == "gzip":
                    content = gzip.decompress(content)
                return content.decode("utf-8")
        except urllib.error.HTTPError as exc:
            body = b""
            try:
                body = exc.read(8192)
            except Exception:
                pass
            if is_cloudflare_challenge(exc.code, exc.headers, body):
                raise CloudflareBlocked(
                    "下载失败: 目标站点启用了 Cloudflare 人机验证，"
                    "自动绕过不可用（未安装 curl_cffi）。" + CF_BLOCKED_HINT,
                    url,
                ) from exc
            raise RuntimeError(f"下载失败: HTTP {exc.code} {exc.reason}") from exc
        except (urllib.error.URLError, OSError) as exc:
            raise RuntimeError(f"下载失败: {exc}") from exc
        except ValueError as exc:
            raise RuntimeError(f"下载失败: 响应内容无效: {exc}") from exc


