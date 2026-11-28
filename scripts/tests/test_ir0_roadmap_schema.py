"""IR0 §3 多阶段路线图 schema 与 v1 编号防冲突校验单元测试。"""
from __future__ import annotations

import importlib.util
from pathlib import Path
import sys
import unittest

REPO_ROOT = Path(__file__).resolve().parents[2]
PATH = REPO_ROOT / "scripts" / "loan_kernel" / "ir0_roadmap_schema.py"
SPEC = importlib.util.spec_from_file_location("ir0_roadmap_schema", PATH)
SCHEMA = importlib.util.module_from_spec(SPEC)
sys.modules[SPEC.name] = SCHEMA
SPEC.loader.exec_module(SCHEMA)


class Ir0RoadmapSchemaTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.v1_ids = SCHEMA.load_v1_item_ids(REPO_ROOT / "docs" / "roadmap" / "v1.json")

    def test_v1_json_has_expected_bare_ids(self):
        """确认只读加载的真实 v1.json 包含文档记载的裸编号前缀（如 L1、S1 等）。"""
        self.assertGreater(len(self.v1_ids), 20)
        self.assertTrue(any(i.startswith("L") for i in self.v1_ids))

    def test_valid_stage1_roadmap_passes_and_respects_entry_gates(self):
        """合规的 stage1.json（带路线前缀、显式 entry_gates、无环）通过校验；门槛未满时禁止激活。"""
        stage1 = {
            "meta": {
                "stage_id": "stage1",
                "entry_gates": ["v1.0", "P1/E2/E3/E5"],
            },
            "items": [
                {"id": "RIR-IR0", "status": "planned", "depends_on": []},
                {"id": "RIR-IR1", "status": "planned", "depends_on": ["RIR-IR0"]},
                {"id": "RL-L0", "status": "planned", "depends_on": ["RIR-IR1"]},
                {"id": "RC-C0", "status": "planned", "depends_on": []},
                {"id": "RF-F5", "status": "planned", "depends_on": []},
            ],
        }
        self.assertEqual([], SCHEMA.validate_stage_roadmap(stage1, self.v1_ids, frozenset()))

        stage1_active = {
            "meta": stage1["meta"],
            "items": [{"id": "RIR-IR0", "status": "active", "depends_on": []}],
        }
        blocked = SCHEMA.validate_stage_roadmap(stage1_active, self.v1_ids, frozenset({"v1.0"}))
        self.assertTrue(any("ENTRY_GATE_BLOCKED" in e for e in blocked))

        unblocked = SCHEMA.validate_stage_roadmap(
            stage1_active, self.v1_ids, frozenset({"v1.0", "P1/E2/E3/E5"})
        )
        self.assertEqual([], unblocked)

    def test_rejects_v1_colliding_bare_ids_missing_prefix_and_dep_cycle(self):
        """拒绝与 v1 撞名的裸编号、无路线前缀编号、缺少 entry_gates 及阶段内依赖环。"""
        sample_v1_id = next(iter(sorted(self.v1_ids)))
        bad = {
            "meta": {"stage_id": "stage1", "entry_gates": []},
            "items": [
                {"id": sample_v1_id, "status": "planned", "depends_on": []},
                {"id": "L99", "status": "planned", "depends_on": []},
                {"id": "RL-A", "status": "planned", "depends_on": ["RL-B"]},
                {"id": "RL-B", "status": "planned", "depends_on": ["RL-A"]},
            ],
        }
        errs = SCHEMA.validate_stage_roadmap(bad, self.v1_ids, frozenset())
        self.assertTrue(any("SCHEMA_ENTRY_GATES" in e for e in errs))
        self.assertTrue(any("ID_COLLIDES_V1" in e for e in errs))
        self.assertTrue(any("ID_MISSING_ROUTE_PREFIX" in e for e in errs))
        self.assertTrue(any("DEP_CYCLE" in e for e in errs))


if __name__ == "__main__":
    unittest.main()
