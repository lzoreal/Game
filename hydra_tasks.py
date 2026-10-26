"""GUI-free task functions for the local game catalog manager."""

import hydra_core as core


def load_sources(sources_dir, report):
    def progress(index, total, name):
        report("status", {"index": index, "total": total, "name": name})

    def on_source(name, source_items, meta):
        report("source", (name, source_items, meta))

    errors = core.stream_sources(sources_dir, progress=progress, on_source=on_source)
    return (errors,)


def search_sources(items, query, fuzzy, min_similarity, source_filter,
                   limit=core.MAX_RESULTS):
    return core.search_items(
        items, query, fuzzy, min_similarity, source_filter, limit
    )


def latest_sources(items, source_filter="", limit=core.LATEST_LIMIT):
    return core.latest_items(items, source_filter, limit)


