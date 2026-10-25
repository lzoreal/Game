"""Core logic for the local game catalog manager: parsing, caching, search.

No GUI imports, so it can be shared by the PySide6 UI and Qt worker threads.
"""

import html
import json
import os
import re
import ssl
import subprocess
import unicodedata
import urllib.error
import urllib.request
from datetime import datetime
from difflib import SequenceMatcher

ssl._create_default_https_context = ssl._create_unverified_context

try:
    import cloudscraper
    CLOUDSCRAPER_AVAILABLE = True
except ImportError:
    CLOUDSCRAPER_AVAILABLE = False

APP_VERSION = "v4.0"
CACHE_VERSION = 3
MAX_RESULTS = 1000

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
    stem = filename[:-5] if filename.lower().endswith(".json") else filename
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
        with open(path + ".url", "r", encoding="utf-8") as f:
            return f.read().strip() or None
    except OSError:
        return None


def write_url_file(path, url):
    with open(path + ".url", "w", encoding="utf-8") as f:
        f.write(url)


def cache_path_for(sources_dir, name):
    return os.path.join(sources_dir, ".cache", name + ".cache")


def invalidate_cache(sources_dir, name):
    cache_path = cache_path_for(sources_dir, name)
    for path in (cache_path, cache_path + ".tmp"):
        try:
            os.remove(path)
        except OSError:
            pass


def write_json_atomic(path, data, indent=2):
    tmp = path + ".tmp"
    with open(tmp, "w", encoding="utf-8") as f:
        json.dump(data, f, ensure_ascii=False, indent=indent)
    os.replace(tmp, path)


def write_cache_file(cache_path, signature, detected_key, invalid_count, items):
    try:
        os.makedirs(os.path.dirname(cache_path), exist_ok=True)
        tmp = cache_path + ".tmp"
        with open(tmp, "w", encoding="utf-8") as f:
            json.dump({
                "version": CACHE_VERSION,
                "signature": signature,
                "detected_key": detected_key,
                "invalid_count": invalid_count,
                "items": items,
            }, f, ensure_ascii=False)
        os.replace(tmp, cache_path)
    except OSError:
        pass


def load_source(sources_dir, name):
    path = os.path.join(sources_dir, name)
    stat = os.stat(path)
    signature = [stat.st_mtime, stat.st_size]
    cache_path = cache_path_for(sources_dir, name)
    url = read_source_url(path)
    display = display_name(name)

    if os.path.exists(cache_path):
        try:
            with open(cache_path, "r", encoding="utf-8") as f:
                cache = json.load(f)
            if cache.get("version") == CACHE_VERSION and cache.get("signature") == signature:
                items = cache["items"]
                for item in items:
                    item.setdefault("nt", normalize_text(item.get("title")))
                    item.setdefault("dkey", dedup_key(item.get("title")))
                    item["source_name"] = name
                    item["source_display"] = display
                return items, {
                    "display": display,
                    "count": len(items),
                    "url": url,
                    "path": path,
                    "updated": stat.st_mtime,
                    "detected_key": cache.get("detected_key"),
                    "invalid_count": cache.get("invalid_count", 0),
                }
        except (OSError, ValueError, KeyError):
            pass

    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)

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

    write_cache_file(cache_path, signature, detected_key, invalid, items)

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
    try:
        names = sorted(n for n in os.listdir(sources_dir) if n.endswith(".json"))
    except OSError:
        names = []

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


class Downloader:
    """Sequential downloader with cloudscraper -> curl -> urllib fallback."""

    def __init__(self):
        self.scraper = None

    def download_json(self, method, url, timeout=30):
        if method == "manual":
            raise ValueError("当前为手动导入模式，无法自动下载。")

        errors = []

        if method == "cloudscraper":
            if not CLOUDSCRAPER_AVAILABLE:
                errors.append("cloudscraper 未安装")
            else:
                try:
                    if self.scraper is None:
                        self.scraper = cloudscraper.create_scraper()
                    resp = self.scraper.get(
                        url, timeout=timeout, headers={'Accept': 'application/json'}
                    )
                    resp.raise_for_status()
                    return resp.text
                except Exception as exc:
                    errors.append(f"cloudscraper: {exc}")

        if method in ("curl", "cloudscraper"):
            try:
                if subprocess.run(["curl", "--version"], capture_output=True).returncode == 0:
                    content = self._download_with_curl(url, timeout)
                    json.loads(content)
                    return content
                errors.append("系统未安装 curl")
            except Exception as exc:
                errors.append(f"curl: {exc}")

        if method in ("urllib", "curl", "cloudscraper"):
            try:
                return self._download_with_urllib(url, timeout)
            except urllib.error.HTTPError as exc:
                if exc.code == 403:
                    errors.append(
                        "urllib: 403 Forbidden - Cloudflare 拦截了自动下载。\n"
                        "可改用 cloudscraper，或手动在浏览器下载 JSON 后导入，或使用备用镜像源。"
                    )
                else:
                    errors.append(f"urllib: {exc}")
            except Exception as exc:
                errors.append(f"urllib: {exc}")

        raise RuntimeError("下载失败:\n" + "\n".join(errors or ["未知下载方式"]))

    def _download_with_curl(self, url, timeout=30):
        cmd = [
            'curl', '-s', '-L', '--max-time', str(timeout),
            '-H', f'User-Agent: {DEFAULT_HEADERS["User-Agent"]}',
            '-H', 'Accept: application/json,text/plain,*/*',
            '-H', 'Accept-Language: zh-CN,zh;q=0.9,en;q=0.8',
            '-H', 'Connection: keep-alive',
            '--compressed',
            url,
        ]
        result = subprocess.run(
            cmd, capture_output=True, text=True, encoding='utf-8', errors='ignore'
        )
        if result.returncode != 0:
            raise RuntimeError(f"curl 失败: {result.stderr}")
        return result.stdout

    def _download_with_urllib(self, url, timeout=30):
        headers = DEFAULT_HEADERS.copy()
        headers['Accept-Encoding'] = 'gzip'
        from urllib.parse import urlparse
        parsed = urlparse(url)
        headers['Referer'] = f"{parsed.scheme}://{parsed.netloc}/"

        req = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(req, timeout=timeout) as response:
            import gzip
            content = response.read()
            if response.info().get('Content-Encoding') == 'gzip':
                content = gzip.decompress(content)
            return content.decode('utf-8')


