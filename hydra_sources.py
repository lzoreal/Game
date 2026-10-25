"""GUI-free library for loaded sources and deduplicated items."""

from dataclasses import dataclass


@dataclass(frozen=True)
class SourceDelta:
    name: str
    items_count: int
    sources_count: int
    total_groups: int
    meta: dict


class SourceLibrary:
    def __init__(self):
        self.items = []
        self.metas = {}
        self._group_keys = set()
        self.total_groups = 0

    def reset(self):
        self.items = []
        self.metas = {}
        self._group_keys = set()
        self.total_groups = 0

    def add_source(self, name, source_items, meta):
        self.items.extend(source_items)
        self.metas[name] = meta
        self._group_keys.update(item["dkey"] for item in source_items)
        self.total_groups = len(self._group_keys)
        return SourceDelta(
            name=name,
            items_count=len(self.items),
            sources_count=len(self.metas),
            total_groups=self.total_groups,
            meta=meta,
        )

    def source_meta(self, name):
        return self.metas.get(name)
