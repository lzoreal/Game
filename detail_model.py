"""GUI-free display model for the game details pane."""

import json
from dataclasses import dataclass

import catalog as core

LINK_LIMIT = 6
RAW_LIMIT = 100000


@dataclass(frozen=True)
class DetailLink:
    uri: str
    is_magnet: bool


@dataclass(frozen=True)
class DetailLinkGroup:
    label: str
    total: int
    visible: tuple[DetailLink, ...]
    hidden: int

    @property
    def can_expand(self):
        return self.hidden > 0


@dataclass(frozen=True)
class DetailCard:
    key: str
    source: str
    date_text: str
    size: str
    description: str
    links: tuple[DetailLinkGroup, ...]


@dataclass(frozen=True)
class DetailModel:
    title: str
    sources: tuple[str, ...]
    meta: str
    cards: tuple[DetailCard, ...]
    hint: str
    raw_visible: bool
    raw_text: str
    raw_label: str


def card_key(item):
    return "|".join(
        (
            str(item.get("source_name") or item.get("source_display") or ""),
            str(item.get("dkey") or item.get("nt") or item.get("title") or ""),
            str(item.get("uploadDate") or ""),
        )
    )


def build_detail(
    group,
    expanded=frozenset(),
    raw_visible=False,
    link_limit=LINK_LIMIT,
    raw_limit=RAW_LIMIT,
):
    cards = []
    used_keys = set()
    for item in group["items"]:
        key = card_key(item)
        if key in used_keys:
            suffix = 2
            while f"{key}#{suffix}" in used_keys:
                suffix += 1
            key = f"{key}#{suffix}"
        used_keys.add(key)
        cards.append(_build_card(item, key, key in expanded, link_limit))

    raw_text = ""
    if raw_visible:
        raw_text = json.dumps(
            [item.get("raw") for item in group["items"]],
            ensure_ascii=False,
            indent=2,
        )
        if len(raw_text) > raw_limit:
            raw_text = raw_text[:raw_limit] + "\n\n...（内容过长，已截断）"

    return DetailModel(
        title=group["title"],
        sources=tuple(group["sources"]),
        meta=(
            f"{group['date_display']}   ·   {group['size']}"
            f"   ·   {group['link_count']} 条链接"
        ),
        cards=tuple(cards),
        hint=f"{len(group['sources'])} 个来源",
        raw_visible=raw_visible,
        raw_text=raw_text,
        raw_label="收起原始 JSON" if raw_visible else "查看原始 JSON",
    )


def _build_card(item, key, expanded, link_limit):
    raw = item.get("raw") if isinstance(item.get("raw"), dict) else {}
    description = core.clean_html(raw.get("descriptionHtml") or "")
    date = core.parse_upload_date(item["uploadDate"])
    date_text = date.strftime("%Y-%m-%d") if date else str(item["uploadDate"])

    magnets = [DetailLink(uri, True) for uri in item["uris"] if uri.startswith("magnet")]
    https = [DetailLink(uri, False) for uri in item["uris"] if uri.startswith("http")]
    others = [
        DetailLink(uri, False)
        for uri in item["uris"]
        if not uri.startswith(("magnet", "http"))
    ]

    links = []
    for label, entries in (
        ("磁力链接", magnets),
        ("HTTP 链接", https),
        ("其他链接", others),
    ):
        if not entries:
            continue
        visible = entries if expanded or len(entries) <= link_limit else entries[:link_limit]
        links.append(
            DetailLinkGroup(
                label=label,
                total=len(entries),
                visible=tuple(visible),
                hidden=len(entries) - len(visible),
            )
        )

    return DetailCard(
        key=key,
        source=item["source_display"],
        date_text=date_text,
        size=str(item["fileSize"]),
        description=description,
        links=tuple(links),
    )
