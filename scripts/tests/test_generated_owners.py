"""Independent, hand-written expectations for bounded ownership programs."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("owners", ROOT / "scripts/generated_owners.py")
OWNERS = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OWNERS)


def fixture(shape, operations, **extra):
    return {"generator_version": "generated-owners-v1", "id": "golden",
            "shape": shape, "seed": 11, "case_index": 0, "choices": [],
            "operations": operations, **extra}


V1 = fixture("V1", [
    {"op": "create", "target": "source", "resource": "a"},
    {"op": "create", "target": "spare", "resource": "b"},
    {"op": "move", "source": "source", "target": "moved"},
    {"op": "inspect", "source": "moved"},
    {"op": "consume", "source": "moved"},
    {"op": "marker", "text": "done"},
])


def v2(position="before", stop=False):
    operations = [
        {"op": "holder", "target": "holder", "resource": "old"},
        {"op": "create", "target": "local", "resource": "local"},
        {"op": "replace", "holder": "holder", "target": "old", "resource": "new"},
        {"op": "inspect", "source": "old"},
        {"op": "marker", "text": "helper-done"},
    ]
    operations.insert(2 if position == "before" else 4, {"op": "return_if"})
    return fixture("V2", operations, return_position=position, stop=stop)


class GeneratedOwnersTests(unittest.TestCase):
    def test_fixed_domain_and_required_paths(self):
        cases = OWNERS.cases()
        self.assertEqual(len(cases), 16)
        self.assertEqual([sum(c["shape"] == shape for c in cases)
                          for shape in ("V1", "V2", "I1", "I2")], [4] * 4)
        self.assertEqual({(c["return_position"], c["stop"]) for c in cases if c["shape"] == "V2"},
                         {("before", False), ("before", True), ("after", False), ("after", True)})
        for case in cases:
            OWNERS.validate(case)
            self.assertLessEqual(len(case["operations"]), 12)
            self.assertLessEqual(len(OWNERS.render(case)["source"].encode()), 8192)
        self.assertEqual(len({c["id"] for c in cases}), 16)

    def test_v1_hand_written_owner_and_cleanup_golden(self):
        result = OWNERS.evaluate(V1)
        self.assertEqual(result["stdout"], "borrow\ndrop:a\nconsume\ndrop:a\ndone\ndrop:b\n")
        self.assertEqual(result["allocations"], 2)
        self.assertEqual(result["events"], [
            {"kind": "scope_enter", "scope": "entry"},
            {"kind": "allocate", "resource": "a", "type": "Leaf", "owner": "entry.source"},
            {"kind": "allocate", "resource": "b", "type": "Leaf", "owner": "entry.spare"},
            {"kind": "move", "resource": "a", "source": "entry.source", "target": "entry.moved"},
            {"kind": "borrow_begin", "resource": "a", "owner": "entry.moved"},
            {"kind": "borrow_read", "resource": "a", "owner": "entry.moved", "field": "name"},
            {"kind": "borrow_end", "resource": "a", "owner": "entry.moved"},
            {"kind": "scope_enter", "scope": "consume"},
            {"kind": "move", "resource": "a", "source": "entry.moved", "target": "consume.item"},
            {"kind": "marker", "text": "consume"},
            {"kind": "drop", "resource": "a", "owner": "consume.item"},
            {"kind": "scope_exit", "scope": "consume"},
            {"kind": "marker", "text": "done"},
            {"kind": "drop", "resource": "b", "owner": "entry.spare"},
            {"kind": "scope_exit", "scope": "entry"},
        ])

    def test_v2_four_hand_written_stdout_goldens(self):
        for position, stop, expected, allocations in [
            ("before", True, "drop:local\ndrop:holder\ndrop:old\ndone\n", 3),
            ("before", False, "borrow\ndrop:old\nhelper-done\ndrop:old\ndrop:local\ndrop:holder\ndrop:new\ndone\n", 4),
            ("after", True, "borrow\ndrop:old\ndrop:old\ndrop:local\ndrop:holder\ndrop:new\ndone\n", 4),
            ("after", False, "borrow\ndrop:old\nhelper-done\ndrop:old\ndrop:local\ndrop:holder\ndrop:new\ndone\n", 4),
        ]:
            with self.subTest(position=position, stop=stop):
                result = OWNERS.evaluate(v2(position, stop))
                self.assertEqual(result["stdout"], expected)
                self.assertEqual(result["allocations"], allocations)
                drops = [e["resource"] for e in result["events"] if e["kind"] == "drop"]
                self.assertEqual(drops, ["local", "holder", "old"] if allocations == 3
                                 else ["old", "local", "holder", "new"])

    def test_fixed_physical_witness_is_explicit_not_inferred_from_drop_log(self):
        witnesses = [c for c in OWNERS.cases() if "expected_free_order" in c]
        self.assertEqual(len(witnesses), 1)
        self.assertEqual(witnesses[0]["expected_free_order"], [0, 2, 3, 1])
        self.assertEqual(OWNERS.evaluate(witnesses[0])["allocations"], 4)

    def test_v2_hand_written_event_golden(self):
        self.assertEqual(OWNERS.evaluate(v2())["events"], [
            {"kind": "scope_enter", "scope": "entry"},
            {"kind": "scope_enter", "scope": "work"},
            {"kind": "allocate", "resource": "old", "type": "Leaf", "owner": "work.holder.state"},
            {"kind": "allocate", "resource": "holder", "type": "Holder", "owner": "work.holder"},
            {"kind": "allocate", "resource": "local", "type": "Leaf", "owner": "work.local"},
            {"kind": "branch", "stop": False},
            {"kind": "allocate", "resource": "new", "type": "Leaf", "owner": "work.holder.state"},
            {"kind": "replace", "owner": "work.holder.state", "old": "old", "new": "new", "target": "work.old"},
            {"kind": "borrow_begin", "resource": "old", "owner": "work.old"},
            {"kind": "borrow_read", "resource": "old", "owner": "work.old", "field": "name"},
            {"kind": "borrow_end", "resource": "old", "owner": "work.old"},
            {"kind": "marker", "text": "helper-done"},
            {"kind": "drop", "resource": "old", "owner": "work.old"},
            {"kind": "drop", "resource": "local", "owner": "work.local"},
            {"kind": "drop", "resource": "holder", "owner": "work.holder"},
            {"kind": "drop", "resource": "new", "owner": "work.holder.state"},
            {"kind": "scope_exit", "scope": "work"},
            {"kind": "marker", "text": "done"},
            {"kind": "scope_exit", "scope": "entry"},
        ])

    def test_moved_local_uses_destination_declaration_order(self):
        case = v2()
        case["operations"][2:2] = [
            {"op": "create", "target": "extra0", "resource": "extra"},
            {"op": "move", "source": "local", "target": "local_moved"},
        ]
        result = OWNERS.evaluate(case)
        self.assertEqual([e["resource"] for e in result["events"] if e["kind"] == "drop"],
                         ["old", "local", "extra", "holder", "new"])

    def test_renderer_golden_and_entry_name(self):
        source = OWNERS.render(V1)["source"]
        self.assertEqual(source, '''// 资源程序：UTF-8 span witness
class Leaf(val name: String) { deinit() { println(this.name) } }
fun inspect(item: Leaf): Unit { println("borrow"); println(item.name) }
fun consume(own item: Leaf): Unit { println("consume") }
fun entry(): Unit {
    val source = Leaf("drop:a")
    val spare = Leaf("drop:b")
    val moved = source
    inspect(moved)
    consume(moved)
    println("done")
}
''')

    def test_invalid_occurrences_are_exact_utf8_tokens(self):
        for case in OWNERS.cases():
            if case["shape"] not in ("I1", "I2"):
                continue
            rendered = OWNERS.render(case)
            self.assertEqual(len(rendered["expected_diagnostics"]), 1)
            diagnostic = rendered["expected_diagnostics"][0]
            self.assertEqual(diagnostic["code"], "L0131" if case["shape"] == "I1" else "L0133")
            self.assertEqual(len(diagnostic["labels"]), 1)
            raw = rendered["source"].encode()
            for start, end in [diagnostic["primary"], *diagnostic["labels"]]:
                self.assertIn(raw[start:end].decode(), ("source", "item"))
                self.assertGreater(start, len(raw[:start].decode()))
            self.assertNotEqual(diagnostic["primary"], diagnostic["labels"][0])
            with self.assertRaises(ValueError):
                OWNERS.evaluate(case)

    def test_invalid_hand_written_byte_span_goldens(self):
        case = copy.deepcopy(V1)
        case["shape"] = "I1"
        case["operations"].insert(-1, {"op": "inspect", "source": "source"})
        self.assertEqual(OWNERS.render(case)["expected_diagnostics"],
                         [{"code": "L0131", "primary": [387, 393], "labels": [[330, 336]]}])
        case = fixture("I2", [{"op": "create", "target": "source", "resource": "a"},
                              {"op": "borrow_consume", "source": "source"},
                              {"op": "marker", "text": "done"}])
        self.assertEqual(OWNERS.render(case)["expected_diagnostics"],
                         [{"code": "L0133", "primary": [271, 275], "labels": [[243, 247]]}])

    def test_fixed_hash_encoding_and_choice_golden(self):
        self.assertEqual(OWNERS.cases()[0]["choices"], [
            {"key": "extra_locals", "modulus": 3,
             "sha256": "67ca05652a6c968ee9934c79ed606d61c8a03cba41aac05e6f77a5ed9aa38eef", "value": 1},
            {"key": "move_count", "modulus": 3,
             "sha256": "cab6a0c6b9ac02ad468e2c8fc2c7202f4155fd2286b5463614d24c5c989fb36a", "value": 1},
            {"key": "name_suffix", "modulus": 65536,
             "sha256": "37c372e6361f315b8cb58ebf8a986b77f2c6d879c4885048b820cb7af174b7af", "value": 47023},
        ])
        self.assertEqual(OWNERS.cases()[0]["operations"], [
            {"op": "create", "target": "source", "resource": "leaf_b7af"},
            {"op": "create", "target": "extra0", "resource": "extra0_b7af"},
            {"op": "move", "source": "source", "target": "moved0"},
            {"op": "move", "source": "moved0", "target": "moved1"},
            {"op": "inspect", "source": "moved1"},
            {"op": "consume", "source": "moved1"},
            {"op": "marker", "text": "done"},
        ])

    def test_bad_provenance_and_metadata_are_rejected(self):
        for key, value in (("generator_version", "next"), ("seed", True), ("case_index", 4),
                           ("choices", [{}]), ("shape", "V3"), ("operations", [{"op": []}])):
            bad = copy.deepcopy(V1)
            bad[key] = value
            with self.subTest(key=key), self.assertRaises(ValueError):
                OWNERS.validate(bad)
        bad = OWNERS.cases()[0]
        bad["choices"][0]["value"] += 1
        with self.assertRaises(ValueError):
            OWNERS.validate(bad)
        for seeds in ([], [1], [1, 1, 2, 3], [1, 2, 3, True], [1, 2, 3, -1]):
            with self.subTest(seeds=seeds), self.assertRaises(ValueError):
                OWNERS.cases(seeds)

    def test_invalid_domain_cannot_hide_a_second_root(self):
        case = next(c for c in OWNERS.cases() if c["shape"] == "I1")
        case["operations"].insert(-1, {"op": "inspect", "source": "source"})
        with self.assertRaises(ValueError):
            OWNERS.validate(case)
        case = next(c for c in OWNERS.cases() if c["shape"] == "I2")
        case["operations"].insert(-1, {"op": "borrow_consume", "source": "source"})
        with self.assertRaises(ValueError):
            OWNERS.validate(case)

    def test_leaf_and_holder_budgets_are_independent(self):
        case = v2()
        case["operations"][2:2] = [{"op": "create", "target": f"extra{i}", "resource": f"extra{i}"}
                                     for i in range(6)]
        self.assertEqual(len(case["operations"]), 12)
        with self.assertRaisesRegex(ValueError, "Leaf budget"):
            OWNERS.validate(case)
        case = v2()
        case["operations"].insert(1, {"op": "holder", "target": "spare", "resource": "second"})
        with self.assertRaisesRegex(ValueError, "Holder budget"):
            OWNERS.validate(case)

    def test_fixed_witness_cannot_silently_change_physical_order(self):
        case = next(c for c in OWNERS.cases() if "expected_free_order" in c)
        case["operations"].insert(2, {"op": "create", "target": "extra0", "resource": "extra"})
        with self.assertRaisesRegex(ValueError, "physical witness"):
            OWNERS.validate(case)

    def test_domain_rejects_out_of_scope_or_inconsistent_operations(self):
        mutations = []
        bad = copy.deepcopy(V1)
        bad["operations"][2]["source"] = "unknown"
        mutations.append(bad)
        bad = copy.deepcopy(V1)
        bad["operations"][1]["resource"] = "a"
        mutations.append(bad)
        bad = copy.deepcopy(V1)
        bad["operations"][3] = {"op": "loop"}
        mutations.append(bad)
        bad = copy.deepcopy(V1)
        bad["operations"][0]["target"] = 'x); panic("bad")'
        mutations.append(bad)
        bad = copy.deepcopy(V1)
        bad["operations"][1]["target"] = "return"
        mutations.append(bad)
        bad = v2()
        bad["return_position"] = "after"
        mutations.append(bad)
        bad = copy.deepcopy(V1)
        bad["operations"] *= 3
        mutations.append(bad)
        for bad in mutations:
            with self.subTest(case=bad):
                with self.assertRaises(ValueError):
                    OWNERS.validate(bad)

    def test_no_hash_seed_or_working_directory_dependence(self):
        import os
        import tempfile
        code = (f"import sys,json;sys.path.insert(0,{str(ROOT / 'scripts')!r});"
                "import generated_owners as g;print(json.dumps([[c,g.render(c),"
                "g.evaluate(c) if c['shape'].startswith('V') else None] for c in g.cases()],sort_keys=True))")
        outputs = []
        for seed in ("1", "987654"):
            with tempfile.TemporaryDirectory() as directory:
                outputs.append(subprocess.check_output([sys.executable, "-c", code], cwd=directory,
                                                       env={**os.environ, "PYTHONHASHSEED": seed}))
        self.assertEqual(outputs[0], outputs[1])

    def test_shrink_candidates_are_smaller_valid_and_retain_root(self):
        count = 0
        for case in OWNERS.cases():
            candidates = list(OWNERS.shrink_candidates(case))
            self.assertEqual(candidates, list(OWNERS.shrink_candidates(case)))
            for candidate in candidates:
                count += 1
                OWNERS.validate(candidate)
                self.assertEqual(candidate["shape"], case["shape"])
                self.assertLess(OWNERS.complexity(candidate), OWNERS.complexity(case))
                if case["shape"] == "V2":
                    self.assertEqual((candidate["stop"], candidate["return_position"]),
                                     (case["stop"], case["return_position"]))
                if case["shape"].startswith("I"):
                    self.assertEqual(OWNERS.render(candidate)["expected_diagnostics"][0]["code"],
                                     OWNERS.render(case)["expected_diagnostics"][0]["code"])
        self.assertGreater(count, 16)

    def test_shrinking_does_not_mutate_input_or_erase_fixed_witness(self):
        for case in OWNERS.cases():
            original = copy.deepcopy(case)
            candidates = list(OWNERS.shrink_candidates(case))
            self.assertEqual(case, original)
            if "expected_free_order" in case:
                self.assertTrue(candidates)
                for candidate in candidates:
                    self.assertEqual(candidate["expected_free_order"], [0, 2, 3, 1])
                    self.assertEqual(OWNERS.evaluate(candidate)["allocations"], 4)


if __name__ == "__main__":
    unittest.main()
