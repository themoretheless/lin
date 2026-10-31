#!/usr/bin/env python3
"""Check roadmap structure and provenance links; does not validate hypotheses."""
import collections
import json
from pathlib import Path
import re
import unicodedata


def main():
    root = Path(__file__).resolve().parents[1]
    data = json.loads((root / "docs/roadmap-2000.json").read_text())
    markdown = (root / "docs/roadmap-2000.md").read_text()
    items = data["items"]
    assert data["task_count"] == len(items) == 2000
    status_counts = dict(sorted(collections.Counter(item["status"] for item in items).items()))
    priority_counts = dict(sorted(collections.Counter(item["priority"] for item in items).items()))
    assert data["status_counts"] == status_counts, "Stale JSON status counts"
    assert data["priority_counts"] == priority_counts, "Stale JSON priority counts"
    totals = "Итоговые количества: " + ", ".join(
        f"{key}: {value}" for key, value in priority_counts.items()
    ) + "; статусы: " + ", ".join(
        f"{key}: {value}" for key, value in status_counts.items()
    ) + "."
    assert totals in markdown, "Stale Markdown summary counts"
    ids = {item["id"] for item in items}
    assert ids == {f"LIN-{number:04}" for number in range(1, 2001)}
    assert [item["rank"] for item in items] == list(range(1, 2001))
    titles = [unicodedata.normalize("NFKC", item["title"]).casefold() for item in items]
    assert len(set(titles)) == 2000, "Duplicate normalized task title"
    areas = {area["id"]: area for area in data["areas"]}
    assert len(areas) == data["area_count"] == 50
    counts = collections.Counter(item["area"] for item in items)
    assert set(counts) == set(areas)
    assert set(counts.values()) == {40}
    for area in areas.values():
        assert area["task_count"] == counts[area["id"]]
        assert set(area["depends_on_areas"]) <= set(areas)
        assert f'id="{area["id"].lower()}"' in markdown
    by_id = {item["id"]: item for item in items}
    active, done = set(), set()

    def visit(identity):
        assert identity not in active, f"Cyclic task dependency: {identity}"
        if identity in done:
            return
        active.add(identity)
        for dependency in by_id[identity]["depends_on"]:
            assert dependency in ids, dependency
            visit(dependency)
        active.remove(identity)
        done.add(identity)

    for item in items:
        assert item["gate"] in data["gates"]
        assert item["priority"] in {"P1", "P2", "P3"}
        assert item["status"] in {
            "candidate", "qualification", "measured_gap", "blocked_external",
            "in_progress", "covered", "done", "rejected", "deferred"
        }
        assert not re.search(r"\.\.\.|…|TODO|TBD", item["title"])
        assert item["source_paths"] == areas[item["area"]]["source_paths"]
        for source in item["source_paths"]:
            assert (root / source).is_file(), source
        visit(item["id"])
    rows = re.findall(r"^\| (\d+) \| (LIN-\d{4}) \| (.+)$", markdown, re.MULTILINE)
    assert len(rows) == 2000
    for (rank, identity, body), item in zip(rows, items):
        assert int(rank) == item["rank"] and identity == item["id"]
        assert f"| {item['title']} |" in "| " + body
        for value in (item["kind"], item["status"], item["effort"], item["gate"]):
            assert value in body
    print("PASS: 2000 tasks, 50 areas, unique IDs/titles, ranks, links, gates and dependencies")


if __name__ == "__main__":
    main()
