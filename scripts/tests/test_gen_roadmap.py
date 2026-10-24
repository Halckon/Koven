from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPTS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS))

import gen_roadmap  # noqa: E402


def minimal(**item_overrides: object) -> dict:
    item = {
        "id": "A1",
        "stage": "P0",
        "area": "types",
        "priority": "must",
        "status": "todo",
        "guide": "needed",
        "phases": [2],
        "title": "Example",
    }
    item.update(item_overrides)
    return {
        "meta": {"title": "t", "updated": "2026-10-16", "baseline": "b", "v1_definition": "d"},
        "stages": [{"id": "P0", "title": "止血", "goal": "g"}, {"id": "X", "title": "外", "goal": "g"}],
        "areas": [{"id": "types", "title": "类型"}],
        "decisions": [],
        "items": [item],
    }


class RoadmapTests(unittest.TestCase):
    def write(self, root: Path, data: dict) -> None:
        path = root / gen_roadmap.DATA
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(data, ensure_ascii=False), encoding="utf-8")

    def test_absent_roadmap_is_silent(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            self.assertEqual(gen_roadmap.drift_errors(Path(temporary)), [])

    def test_generated_views_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            self.write(root, minimal())
            self.assertEqual(len(gen_roadmap.drift_errors(root)), 3)
            for path, text in gen_roadmap.render_targets(root).items():
                path.write_text(text, encoding="utf-8")
            self.assertEqual(gen_roadmap.drift_errors(root), [])

    def test_invalid_entries_are_rejected(self) -> None:
        cases = [
            {"status": "finished"},
            {"specs": ["9999"]},
            {"links": ["guide/missing.md"]},
            {"decision": "D404"},
            {"priority": "must", "status": "deferred"},
            {"stage": "X"},
            {"phases": [7]},
            {"owner": "someone"},
        ]
        for overrides in cases:
            with self.subTest(overrides=overrides), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                self.write(root, minimal(**overrides))
                errors = gen_roadmap.drift_errors(root)
                self.assertEqual(len(errors), 1)
                self.assertIn("v1.json", errors[0])

    def test_spec_numbers_resolve_across_lifecycle_directories(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            spec = root / "docs/specs/drafts/v0.44/0291-readonly-map-construction.md"
            spec.parent.mkdir(parents=True)
            spec.write_text("# SPEC-0291: x\n", encoding="utf-8")
            self.write(root, minimal(specs=["0291"]))
            text = gen_roadmap.render_targets(root)[root / gen_roadmap.VIEWS["stage"]]
            self.assertIn("[0291](../specs/drafts/v0.44/0291-readonly-map-construction.md)", text)


if __name__ == "__main__":
    unittest.main()
