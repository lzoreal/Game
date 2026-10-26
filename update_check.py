"""GitHub release update checks (GUI-free)."""

import json
import re
import urllib.error
import urllib.request
from dataclasses import dataclass

API_BASE = "https://api.github.com"
USER_AGENT = "GameMagNet/4.0"
READ_LIMIT = 262144


class UpdateError(RuntimeError):
    pass


@dataclass(frozen=True)
class ReleaseInfo:
    tag: str
    name: str
    url: str
    download_url: str | None
    notes: str
    published_at: str


@dataclass(frozen=True)
class UpdateResult:
    current: str
    latest: str
    update_available: bool
    release: ReleaseInfo


def parse_version(value):
    text = str(value or "").strip().lower()
    if text.startswith("v"):
        text = text[1:]
    numbers = re.findall(r"\d+", text)
    return tuple(int(part) for part in numbers) or (0,)


def is_newer(latest, current):
    newer = parse_version(latest)
    current_version = parse_version(current)
    width = max(len(newer), len(current_version))
    newer += (0,) * (width - len(newer))
    current_version += (0,) * (width - len(current_version))
    return newer > current_version


def check_latest(repo, current, token=None, timeout=6.0, api_base=API_BASE):
    if not re.fullmatch(r"[A-Za-z0-9_.-]+/[A-Za-z0-9_.-]+", str(repo or "")):
        raise UpdateError(f"无效的仓库名: {repo!r}")

    url = api_base.rstrip("/") + "/repos/" + repo + "/releases/latest"
    headers = {
        "Accept": "application/vnd.github+json",
        "User-Agent": USER_AGENT,
    }
    if token:
        headers["Authorization"] = "Bearer " + token

    request = urllib.request.Request(url, headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=timeout) as response:
            payload = json.loads(response.read(READ_LIMIT).decode("utf-8"))
    except urllib.error.HTTPError as exc:
        if exc.code == 404:
            raise UpdateError(
                "未找到 Release（仓库不存在、未公开或尚未发布版本）"
            ) from exc
        if exc.code in (401, 403):
            raise UpdateError(f"GitHub API 拒绝访问（HTTP {exc.code}）") from exc
        raise UpdateError(f"GitHub API 错误（HTTP {exc.code}）") from exc
    except (urllib.error.URLError, OSError, ValueError) as exc:
        raise UpdateError(f"无法连接 GitHub: {exc}") from exc

    if not isinstance(payload, dict) or not payload.get("tag_name"):
        raise UpdateError("GitHub 返回的 Release 数据无效")

    assets = payload.get("assets") or []
    download_url = None
    for asset in assets:
        if not isinstance(asset, dict):
            continue
        candidate = asset.get("browser_download_url")
        if not isinstance(candidate, str):
            continue
        if candidate.lower().endswith(".exe"):
            download_url = candidate
            break
        if download_url is None:
            download_url = candidate

    tag = str(payload["tag_name"]).strip()
    release = ReleaseInfo(
        tag=tag,
        name=str(payload.get("name") or tag),
        url=str(payload.get("html_url") or ""),
        download_url=download_url,
        notes=str(payload.get("body") or ""),
        published_at=str(payload.get("published_at") or ""),
    )
    return UpdateResult(
        current=str(current),
        latest=tag,
        update_available=is_newer(tag, current),
        release=release,
    )
