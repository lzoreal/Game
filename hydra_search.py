"""GUI-free search state: query, token, filter, timing and date sort."""

import time
from dataclasses import dataclass


@dataclass(frozen=True)
class SearchTicket:
    token: int
    query: str
    source_filter: str
    fuzzy: bool
    threshold: float
    started: float


def sort_by_date(groups, desc=True):
    dated = [group for group in groups if group.get("date_dt") is not None]
    undated = [group for group in groups if group.get("date_dt") is None]
    dated.sort(key=lambda group: group["date_dt"], reverse=desc)
    return dated + undated


class SearchController:
    def __init__(self, debounce_ms=250):
        self.debounce_ms = debounce_ms
        self.token = 0
        self.query = ""
        self.sort_desc = True

    def observe(self, text):
        self.query = str(text or "").strip()
        self.token += 1
        return bool(self.query)

    def begin(self, query, source_filter="", fuzzy=True, threshold=0.3, now=None):
        self.query = str(query or "").strip()
        self.token += 1
        if not self.query:
            return None
        return SearchTicket(
            token=self.token,
            query=self.query,
            source_filter=source_filter,
            fuzzy=fuzzy,
            threshold=threshold,
            started=time.perf_counter() if now is None else now,
        )

    def accepts(self, token):
        return token == self.token

    def invalidate(self):
        """Cancel any in-flight search without issuing a new ticket."""
        self.token += 1

    def elapsed_ms(self, ticket, now=None):
        end = time.perf_counter() if now is None else now
        return (end - ticket.started) * 1000

    def toggle_date_sort(self):
        self.sort_desc = not self.sort_desc
        return self.sort_desc

    def apply_sort(self, groups):
        return sort_by_date(groups, self.sort_desc)
