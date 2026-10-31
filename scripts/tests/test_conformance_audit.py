#!/usr/bin/env python3
"""Unit tests for the public-entry conformance runner, oracle comparator, and regression gate."""

from __future__ import annotations

import hashlib
import contextlib
import io
import json
import os
from pathlib import Path
import re
import stat
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import conformance_gate as gate


def _sha256(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def _make_mock_compiler(path: Path, body: str) -> Path:
    script = "#!/usr/bin/env python3\nimport os, sys, time, signal\n" + body
    path.write_text(script, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


class ConformanceAuditSelfTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp_dir = tempfile.TemporaryDirectory(prefix="koven-conf-test-")
        self.root = Path(self.temp_dir.name)
        self.corpus = self.root / "conformance"
        (self.corpus / "probes").mkdir(parents=True)
        (self.corpus / "programs" / "word_count").mkdir(parents=True)

        self.probe_ok_src = 'fun main(): Unit { println("hello") }\n'
        self.probe_diag_src = "fun main(): Unit { val x = 1; println(x) }\n"
        self.probe_abort_src = 'fun main(): Unit { error("boom") }\n'
        self.probe_unimpl_src = 'fun main(): Unit { val d = 1.5 * 2.0; println("3.0") }\n'
        self.prog_src = 'fun main(): Unit { println("word\\t1") }\n'
        self.prog_expected = "word\t1\n"

        (self.corpus / "probes" / "p01_ok.ko").write_text(self.probe_ok_src, encoding="utf-8")
        (self.corpus / "probes" / "p02_diag.ko").write_text(self.probe_diag_src, encoding="utf-8")
        (self.corpus / "probes" / "p03_abort.ko").write_text(self.probe_abort_src, encoding="utf-8")
        (self.corpus / "probes" / "p04_unimpl.ko").write_text(self.probe_unimpl_src, encoding="utf-8")

        (self.corpus / "programs" / "word_count" / "main.ko").write_text(
            self.prog_src, encoding="utf-8"
        )
        (self.corpus / "programs" / "word_count" / "expected.txt").write_text(
            self.prog_expected, encoding="utf-8"
        )

        self.expectations_path = self.corpus / "expectations.json"
        self.baseline_path = self.corpus / "baseline.tsv"
        self._write_default_expectations()
        self.baseline_path.write_text(
            "\n".join(
                [
                    "p01_ok\tOK\tOK",
                    "p02_diag\tFE:L0084\tFE:L0084",
                    "p03_abort\tabort(exit 1,no msg)\tabort(exit 1,no msg)",
                    "p04_unimpl\tBE:UnsupportedNode\tBE:UnsupportedNode",
                ]
            )
            + "\n",
            encoding="utf-8",
        )

    def tearDown(self) -> None:
        self.temp_dir.cleanup()

    def _write_default_expectations(self, extra_probe: dict | None = None) -> None:
        probes = {
            "p01_ok": {
                "kind": "run_ok",
                "source_sha256": _sha256(self.probe_ok_src),
                "stdout": "hello\n",
            },
            "p02_diag": {
                "kind": "compile_fail",
                "source_sha256": _sha256(self.probe_diag_src),
                "diagnostic": "L0084",
            },
            "p03_abort": {
                "kind": "runtime_abort",
                "source_sha256": _sha256(self.probe_abort_src),
                "abort_message": "boom",
                "v046_baseline_exempt": True,
            },
            "p04_unimpl": {
                "kind": "known_unimplemented",
                "source_sha256": _sha256(self.probe_unimpl_src),
                "allowed_markers": ["BE:UnsupportedNode"],
                "target_stdout": "3.0\n",
            },
        }
        if extra_probe:
            probes.update(extra_probe)
        data = {
            "schema": gate.SCHEMA_EXPECTATIONS,
            "probes": probes,
            "programs": {
                "word_count": {
                    "kind": "known_unimplemented",
                    "source_sha256": _sha256(self.prog_src),
                    "expected_file": "programs/word_count/expected.txt",
                    "expected_sha256": _sha256(self.prog_expected),
                    "allowed_markers": ["BE:UnsupportedNode", "FE:L0080"],
                }
            },
        }
        self.expectations_path.write_text(json.dumps(data, indent=2), encoding="utf-8")

    def test_12_compile_fail_hardening_and_all_154_probe_oracles_cross_checked_against_sources(
        self,
    ) -> None:
        # 1. Non-header Lxxxx in label/help/note or stdout must NEVER classify as FE:Lxxxx
        self.assertEqual(
            gate.classify_raw(1, False, b"", b"  label main.ko:1:1: see L0084 for details\n"),
            "ERR",
        )
        self.assertEqual(
            gate.classify_raw(1, False, b"error[L0084] main.ko:1:1: on stdout\n", b"fatal\n"),
            "ERR",
        )
        # 2. Backend/ICE failure must take precedence over FE:Lxxxx
        self.assertEqual(
            gate.classify_raw(
                2,
                False,
                b"",
                b"error[L0084] main.ko:1:1: diag\nlowering failed with InvalidSsa\n",
            ),
            "BE:InvalidSsa",
        )
        # 3. Unexpected extra error[Lyyyy] on a single-diagnostic compile_fail probe is rejected
        verdict, reason = gate.evaluate_mode_against_oracle(
            {
                "marker": "FE:L0084",
                "exit": 1,
                "stdout_bytes": b"",
                "stderr_bytes": (
                    b"error[L0084] main.ko:1:1: first\n"
                    b"  label main.ko:1:2: L9999 in label is ignored\n"
                    b"error[L0132] main.ko:2:1: unexpected second error\n"
                ),
            },
            {"kind": "compile_fail", "diagnostic": "L0084"},
        )
        self.assertEqual(verdict, "regression")
        self.assertIn("expected error diagnostics ['L0084'], got ['L0084', 'L0132']", reason or "")

        # 4. Programmatically cross-check all 154 probe expectations against their .ko source files
        repo_root = Path(__file__).resolve().parents[2]
        repo_corpus = repo_root / "conformance"
        loaded = gate.load_expectations(repo_corpus / "expectations.json", corpus_root=repo_corpus)
        for name, spec in loaded["probes"].items():
            self.assertIn(
                spec.get("oracle_origin"),
                {
                    "single_println_literal",
                    "source_trace_audited",
                    "v046_spec_contract",
                    "baseline_diagnostic_audited",
                    "baseline_unimplemented_audited",
                },
                f"missing or invalid oracle_origin on {name}",
            )
            src_text = (repo_corpus / "probes" / f"{name}.ko").read_text(encoding="utf-8")
            println_literals = gate.SIMPLE_PRINTLN_LITERAL_RE.findall(src_text)
            all_string_literals = re.findall(r'"([^"\\$]+)"', src_text)
            joined_literals = "".join(all_string_literals)
            if spec["kind"] == "run_ok":
                expected_lines = [ln for ln in spec["stdout"].splitlines() if ln]
                self.assertTrue(expected_lines, f"empty stdout oracle for {name}")
                if spec["oracle_origin"] == "single_println_literal":
                    self.assertEqual(println_literals, [spec["stdout"].rstrip("\n")])
                else:
                    for ln in expected_lines:
                        self.assertTrue(
                            ln in all_string_literals or ln in joined_literals,
                            f"probe {name} oracle line {ln!r} not backed by source string literals {all_string_literals}",
                        )
            self.assertIn(
                spec.get("guide_chapter"),
                gate.VALID_GUIDE_CHAPTERS,
                f"missing or invalid guide_chapter on {name}",
            )
            if spec["kind"] in ("run_ok", "compile_fail", "runtime_abort"):
                self.assertTrue(
                    isinstance(spec.get("oracle_derivation"), str)
                    and len(spec["oracle_derivation"].strip()) > 0,
                    f"missing oracle_derivation on active probe {name}",
                )

        # 5. Cross-check all compile_fail diagnostic codes against crates/lang-frontend/src/diagnostic.rs
        diag_rs = (
            repo_root / "crates" / "lang-frontend" / "src" / "diagnostic.rs"
        ).read_text(encoding="utf-8")
        registered_codes = set(re.findall(r'"(L[0-9]{4})"', diag_rs))
        for name, spec in loaded["probes"].items():
            if spec["kind"] == "compile_fail":
                for code in spec.get("all_diagnostics", [spec["diagnostic"]]):
                    self.assertIn(
                        code,
                        registered_codes,
                        f"probe {name} references unregistered frontend diagnostic code {code}",
                    )

        # 6. Verify all 13 normative Guide chapters are covered and ORACLE_AUDIT.md is in sync
        covered_chapters = {spec["guide_chapter"] for spec in loaded["probes"].values()}
        self.assertEqual(covered_chapters, set(gate.VALID_GUIDE_CHAPTERS))
        audit_md_path = repo_corpus / "ORACLE_AUDIT.md"
        self.assertTrue(audit_md_path.is_file())
        self.assertEqual(
            audit_md_path.read_text(encoding="utf-8"),
            gate.render_oracle_audit_markdown(loaded),
        )

        # 7. Cross-check all 31 known_unimplemented probes and 6 programs against docs/roadmap/v1.json
        roadmap_items = {
            it["id"]: it
            for it in json.loads(
                (repo_root / "docs" / "roadmap" / "v1.json").read_text(encoding="utf-8")
            )["items"]
        }
        for name, spec in loaded["probes"].items():
            if spec["kind"] == "known_unimplemented":
                blockers = spec.get("roadmap_blockers")
                self.assertTrue(
                    isinstance(blockers, list) and len(blockers) > 0,
                    f"probe {name} missing or empty roadmap_blockers",
                )
                for b in blockers:
                    self.assertIn(
                        b,
                        roadmap_items,
                        f"probe {name} references unknown roadmap blocker {b}",
                    )
                self.assertTrue(
                    any(roadmap_items[b]["status"] != "done" for b in blockers),
                    f"probe {name} has all blockers done: must transition out of known_unimplemented",
                )
        for name, spec in loaded["programs"].items():
            if spec["kind"] == "known_unimplemented":
                blockers = spec.get("roadmap_blockers")
                self.assertTrue(
                    isinstance(blockers, list) and len(blockers) > 0,
                    f"program {name} missing or empty roadmap_blockers",
                )
                for b in blockers:
                    self.assertIn(
                        b,
                        roadmap_items,
                        f"program {name} references unknown roadmap blocker {b}",
                    )
                self.assertTrue(
                    any(roadmap_items[b]["status"] != "done" for b in blockers),
                    f"program {name} has all blockers done: must transition out of known_unimplemented",
                )
            self.assertGreater(spec.get("expected_lines", 0), 0, f"program {name} missing expected_lines")
            self.assertGreater(spec.get("expected_bytes", 0), 0, f"program {name} missing expected_bytes")

        # 8. Verify per-chapter positive vs negative polarity matrix across all 13 chapters
        polarity = gate.summarize_chapter_polarity(loaded)
        self.assertEqual(set(polarity.keys()), set(gate.VALID_GUIDE_CHAPTERS))
        for ch, st in polarity.items():
            self.assertTrue(st["has_positive"], f"chapter {ch} lacks positive run_ok probes")
        pos_only = sorted(
            ch for ch, st in polarity.items() if st["polarity_status"] == "positive_only"
        )
        both_pol = sorted(
            ch
            for ch, st in polarity.items()
            if st["polarity_status"] == "both_positive_and_negative"
        )
        self.assertEqual(
            pos_only,
            [
                "01-lexical.md",
                "08-class-family-members.md",
                "11-copyability-layout-construction.md",
                "13-program-runtime-standard-library.md",
            ],
        )
        self.assertEqual(len(both_pol), 9)

        # 9. Verify DIAGNOSTIC_SYMBOL_CATALOG against diagnostic.rs::codes and summarize_negative_oracle_catalog
        for code, sym in gate.DIAGNOSTIC_SYMBOL_CATALOG.items():
            self.assertRegex(
                diag_rs,
                rf"const\s+{re.escape(sym)}\s*:\s*&str\s*=\s*\"{re.escape(code)}\";",
                f"DIAGNOSTIC_SYMBOL_CATALOG mismatch for {code} -> {sym}",
            )
        neg_catalog = gate.summarize_negative_oracle_catalog(loaded)
        self.assertEqual(len(neg_catalog["diagnostics"]), 10)
        self.assertEqual(len(neg_catalog["runtime_aborts"]), 3)
        for name, spec in loaded["probes"].items():
            if spec["kind"] == "compile_fail":
                self.assertEqual(
                    spec.get("diagnostic_symbol"),
                    gate.DIAGNOSTIC_SYMBOL_CATALOG[spec["diagnostic"]],
                )
            elif spec["kind"] == "known_unimplemented":
                ts = spec.get("target_stdout")
                self.assertTrue(
                    isinstance(ts, str) and len(ts) > 0 and ts.endswith("\n"),
                    f"known_unimplemented probe {name} missing valid target_stdout",
                )
                allowed = spec.get("allowed_markers", [])
                self.assertTrue(
                    len(allowed) > 0 and set(allowed).issubset(gate.VALID_UNIMPLEMENTED_MARKERS),
                    f"known_unimplemented probe {name} has invalid allowed_markers {allowed}",
                )
            self.assertTrue(
                isinstance(spec.get("oracle_derivation"), str)
                and len(spec["oracle_derivation"].strip()) > 0,
                f"probe {name} missing oracle_derivation",
            )
        for name, spec in loaded["programs"].items():
            self.assertTrue(
                isinstance(spec.get("oracle_derivation"), str)
                and len(spec["oracle_derivation"].strip()) > 0,
                f"program {name} missing oracle_derivation",
            )
            allowed = spec.get("allowed_markers", [])
            self.assertTrue(
                len(allowed) > 0 and set(allowed).issubset(gate.VALID_UNIMPLEMENTED_MARKERS),
                f"program {name} has invalid allowed_markers {allowed}",
            )

        # 10. Verify baseline_marker parity with baseline-aef3f4f8.tsv and summarize_baseline_evolution
        baseline_rows = gate.parse_tsv_matrix(
            (repo_corpus / "baseline-aef3f4f8.tsv").read_text(encoding="utf-8")
        )
        for name, spec in loaded["probes"].items():
            self.assertEqual(
                spec.get("baseline_marker"),
                baseline_rows[name][0],
                f"probe {name} baseline_marker mismatch against baseline-aef3f4f8.tsv",
            )
        base_evo = gate.summarize_baseline_evolution(loaded)
        self.assertEqual(base_evo["regressed_from_baseline_ok"], [])
        self.assertEqual(len(base_evo["baseline_ok_preserved"]), 89)
        self.assertEqual(len(base_evo["promoted_to_run_ok"]), 32)
        self.assertEqual(len(base_evo["upgraded_to_structured_abort"]), 3)
        self.assertEqual(len(base_evo["upgraded_to_compile_fail"]), 1)
        self.assertEqual(len(base_evo["baseline_compile_fail_preserved"]), 13)
        self.assertEqual(len(base_evo["remaining_known_unimplemented"]), 16)
        self.assertEqual(
            sum(len(v) for v in base_evo.values()),
            len(loaded["probes"]),
        )

        # 11. Verify all 39 v046_baseline_exempt probes have resolved spec_refs in docs/specs or docs/archive/specs
        exempt_probes = [
            name for name, spec in loaded["probes"].items() if spec.get("v046_baseline_exempt")
        ]
        self.assertEqual(len(exempt_probes), 39)
        for name in exempt_probes:
            spec = loaded["probes"][name]
            srefs = spec.get("spec_refs", [])
            self.assertTrue(len(srefs) > 0, f"exempt probe {name} missing spec_refs")
            for sref in srefs:
                resolved = gate.resolve_spec_doc_relpath(sref, repo_root)
                self.assertIsNotNone(resolved, f"exempt probe {name} references missing {sref}")
                relpath, status = resolved
                self.assertTrue((repo_root / relpath).is_file())
                self.assertIn(status, ("active", "archived"))
        spec_trace = gate.summarize_v046_spec_traceability(loaded, repo_root)
        self.assertEqual(len(spec_trace), 16)
        covered_by_specs = {
            p for info in spec_trace.values() for p in info["probes"]
        }
        self.assertEqual(covered_by_specs, set(exempt_probes))

        # 12. Verify summarize_oracle_origins covers all 154 probes and 6 programs
        origin_stats = gate.summarize_oracle_origins(loaded)
        self.assertEqual(set(origin_stats.keys()), set(gate.VALID_ORACLE_ORIGINS))
        self.assertEqual(
            sum(info["probes"] for info in origin_stats.values()),
            len(loaded["probes"]),
        )
        self.assertEqual(
            sum(info["programs"] for info in origin_stats.values()),
            len(loaded["programs"]),
        )
        self.assertEqual(origin_stats["single_println_literal"]["probes"], 31)
        self.assertEqual(origin_stats["source_trace_audited"]["probes"], 57)
        self.assertEqual(origin_stats["v046_spec_contract"]["probes"], 39)
        self.assertEqual(origin_stats["baseline_diagnostic_audited"]["probes"], 13)
        self.assertEqual(origin_stats["baseline_unimplemented_audited"]["probes"], 14)
        self.assertEqual(origin_stats["expected_txt_file"]["programs"], 6)

        # 13. Verify expected_lines/expected_bytes across all 137 output-bearing probes and summarize_output_shape_distribution
        shape_stats = gate.summarize_output_shape_distribution(loaded)
        self.assertEqual(len(shape_stats["single_line_probes"]), 117)
        self.assertEqual(len(shape_stats["multi_line_probes"]), 20)
        self.assertEqual(len(shape_stats["zero_stdout_probes"]), 17)
        self.assertEqual(
            len(shape_stats["single_line_probes"])
            + len(shape_stats["multi_line_probes"])
            + len(shape_stats["zero_stdout_probes"]),
            len(loaded["probes"]),
        )
        for name, spec in loaded["probes"].items():
            if spec["kind"] in ("run_ok", "known_unimplemented"):
                self.assertGreater(spec.get("expected_lines", 0), 0, f"probe {name} missing expected_lines")
                self.assertGreater(spec.get("expected_bytes", 0), 0, f"probe {name} missing expected_bytes")
            else:
                self.assertNotIn("expected_lines", spec)
                self.assertNotIn("expected_bytes", spec)

        # 14. Verify probe_batch taxonomy (11 batches, 154 probes) and conformance/README.md chapter table sync
        batch_stats = gate.summarize_probe_batches(loaded)
        self.assertEqual(set(batch_stats.keys()), set(gate.PROBE_BATCH_CATALOG.keys()))
        self.assertEqual(
            sum(b["total"] for b in batch_stats.values()),
            len(loaded["probes"]),
        )
        self.assertEqual(batch_stats["p"]["total"], 59)
        self.assertEqual(batch_stats["q"]["total"], 20)
        self.assertEqual(batch_stats["r"]["total"], 2)
        self.assertEqual(batch_stats["s"]["total"], 4)
        self.assertEqual(batch_stats["t"]["total"], 10)
        self.assertEqual(batch_stats["u"]["total"], 4)
        self.assertEqual(batch_stats["v"]["total"], 23)
        self.assertEqual(batch_stats["w"]["total"], 19)
        self.assertEqual(batch_stats["x"]["total"], 3)
        self.assertEqual(batch_stats["y"]["total"], 6)
        self.assertEqual(batch_stats["e"]["total"], 4)
        for full_batch in ("r", "t", "u", "v", "w", "x", "y", "e"):
            self.assertEqual(batch_stats[full_batch]["batch_status"], "all_passed")
            self.assertEqual(batch_stats[full_batch]["known_unimplemented"], 0)
        readme_md_path = repo_corpus / "README.md"
        self.assertTrue(readme_md_path.is_file())
        actual_readme = readme_md_path.read_text(encoding="utf-8")
        self.assertEqual(
            actual_readme,
            gate.sync_readme_chapter_table(actual_readme, loaded),
        )
        with self.assertRaisesRegex(ValueError, "missing the Guide chapter coverage table"):
            gate.sync_readme_chapter_table("# No table\n", loaded)
        with self.assertRaisesRegex(ValueError, "violates canonical batch naming convention"):
            gate.classify_probe_batch("z01_invalid_prefix")
        marker_stats = gate.summarize_unimplemented_markers(loaded)
        self.assertEqual(set(marker_stats.keys()), set(gate.UNIMPLEMENTED_MARKER_CATALOG.keys()))
        self.assertEqual(marker_stats["BE:UnsupportedNode"]["baseline_count"], 8)
        self.assertEqual(marker_stats["FE:L0165"]["baseline_count"], 5)
        self.assertEqual(marker_stats["FE:L0080"]["baseline_count"], 2)
        self.assertEqual(len(marker_stats["BE:UnsupportedSource"]["probes"]), 1)
        prog_blockers = gate.summarize_program_roadmap_blockers(loaded, roadmap_items)
        self.assertEqual(
            set(prog_blockers.keys()),
            {"L1", "L2", "L3b", "L7", "N01", "N02", "N03", "N05", "N07", "N08"},
        )
        self.assertEqual(prog_blockers["L3b"]["programs"], ["varint_codec"])
        self.assertEqual(prog_blockers["L3b"]["probes"], [])
        self.assertEqual(len(prog_blockers["N02"]["programs"]), 4)
        self.assertEqual(set(gate.GUIDE_CHAPTER_DOMAINS.keys()), set(gate.VALID_GUIDE_CHAPTERS))
        cohort = gate.summarize_foundational_cohort(loaded)
        self.assertEqual(cohort["total_foundational"], 85)
        self.assertEqual(cohort["groups"]["A"]["total"], 74)
        self.assertEqual(cohort["groups"]["A"]["runnable"], 58)
        self.assertEqual(cohort["groups"]["A"]["compile_fail"], ["q17_generic_class"])
        self.assertEqual(len(cohort["groups"]["A"]["known_unimplemented"]), 15)
        self.assertEqual(cohort["groups"]["B"]["total"], 8)
        self.assertEqual(len(cohort["groups"]["B"]["compile_fail"]), 8)
        self.assertEqual(cohort["groups"]["C"]["total"], 3)
        self.assertEqual(cohort["p2_gap_to_target"], 12)
        v2_ready = gate.summarize_v2_chapter_readiness(loaded)
        self.assertEqual(set(v2_ready.keys()), set(gate.VALID_GUIDE_CHAPTERS))
        self.assertEqual(
            [ch for ch, r in v2_ready.items() if r["v2_status"] == "v2_probe_ready"],
            [
                "04-expressions-operators.md",
                "05-declarations-callables.md",
                "12-collections-destructuring.md",
            ],
        )
        self.assertEqual(
            len([ch for ch, r in v2_ready.items() if r["v2_status"] == "blocked_by_roadmap"]),
            6,
        )
        self.assertEqual(
            [ch for ch, r in v2_ready.items() if r["v2_status"] == "needs_negative_and_roadmap"],
            [
                "01-lexical.md",
                "08-class-family-members.md",
                "11-copyability-layout-construction.md",
                "13-program-runtime-standard-library.md",
            ],
        )
        impact = gate.summarize_blocker_unlock_impact(loaded, roadmap_items)
        self.assertEqual(sum(len(v["sole_probes"]) for v in impact.values()), 12)
        self.assertEqual(impact["N05"]["sole_probes"], ["p05_double", "p06_char"])
        self.assertEqual(impact["L7"]["sole_probes"], [])
        self.assertEqual(impact["L7"]["co_probes"], ["p21_box_enum", "q08_enum_string_payload"])
        self.assertEqual(impact["L3b"]["programs"], ["varint_codec"])
        stage_prog = gate.summarize_stage_progression(loaded, roadmap_items)
        self.assertEqual([r["stage"] for r in stage_prog], ["P2", "P3", "P4"])
        self.assertEqual([r["projected_probe_cap"] for r in stage_prog], [147, 153, 154])
        self.assertEqual([r["projected_group_a_runnable"] for r in stage_prog], [67, 72, 73])
        self.assertEqual([r["projected_programs_ready"] for r in stage_prog], [2, 6, 6])
        self.assertEqual(
            stage_prog[0]["newly_unlocked_programs"], ["raii_state_machine", "word_count"]
        )
        area_impact = gate.summarize_roadmap_area_impact(loaded, roadmap_items)
        self.assertEqual(set(area_impact.keys()), set(gate.ROADMAP_AREA_CATALOG.keys()))
        self.assertEqual(area_impact["collections"]["probes"], ["s02_pair_builtin", "s03_to_pair"])
        self.assertEqual(area_impact["stdlib"]["probes"], ["p21_box_enum", "p27_thread"])
        promo_rows = gate.summarize_unimplemented_promotion_contracts(loaded)
        self.assertEqual(len(promo_rows), 16)
        self.assertEqual(
            [r["name"] for r in promo_rows if r["foundational_group"] == "C"],
            ["p35_value_eq"],
        )
        self.assertEqual(
            len([r for r in promo_rows if r["foundational_group"] == "A"]),
            15,
        )
        self.assertTrue(all(r["target_stdout"].endswith("\n") for r in promo_rows))
        phase_mat = gate.summarize_compiler_phase_matrix(loaded, roadmap_items)
        self.assertEqual(set(phase_mat.keys()), {2, 3, 4, 5})
        self.assertEqual(len(phase_mat[2]["active_negative_probes"]), 10)
        self.assertEqual(len(phase_mat[3]["active_negative_probes"]), 4)
        self.assertEqual(len(phase_mat[4]["active_negative_probes"]), 3)
        self.assertEqual(len(phase_mat[4]["blocked_probes"]), 16)
        self.assertEqual(len(phase_mat[4]["blocked_programs"]), 6)
        litmus_align = gate.summarize_guide_litmus_alignment(loaded)
        self.assertEqual(list(litmus_align.keys()), list(range(1, 13)))
        self.assertEqual(
            [k for k, v in litmus_align.items() if v["status"] == "all_passed"],
            [1, 2, 3, 8, 9, 10, 11, 12],
        )
        self.assertEqual(
            [k for k, v in litmus_align.items() if v["status"] == "partial"],
            [4, 5, 6, 7],
        )
        ver_bounds = gate.summarize_guide_version_boundaries(loaded)
        self.assertEqual(list(ver_bounds.keys()), ["v0.44", "v0.45", "v0.46", "v0.47"])
        self.assertEqual(ver_bounds["v0.44"]["status"], "all_passed")
        self.assertEqual(ver_bounds["v0.45"]["status"], "all_passed")
        self.assertEqual(ver_bounds["v0.46"]["status"], "partial")
        self.assertEqual(ver_bounds["v0.47"]["status"], "all_passed")
        prog_prereqs = gate.summarize_program_probe_prerequisites(loaded)
        self.assertEqual(set(prog_prereqs.keys()), set(loaded["programs"].keys()))
        self.assertEqual(
            len(prog_prereqs["word_count"]["passed_probes"]),
            5,
        )
        self.assertEqual(
            prog_prereqs["word_count"]["unimplemented_probes"],
            ["p15_elvis", "q08_enum_string_payload"],
        )
        src_scale = gate.summarize_corpus_source_scale(loaded)
        self.assertEqual(src_scale["total_probe_count"], 154)
        self.assertEqual(src_scale["total_probe_source_lines"], 370)
        self.assertEqual(src_scale["total_probe_source_bytes"], 23071)
        self.assertEqual(src_scale["total_program_count"], 6)
        self.assertEqual(src_scale["total_program_source_lines"], 554)
        self.assertEqual(src_scale["total_program_source_bytes"], 17175)
        self.assertEqual(src_scale["total_corpus_source_lines"], 924)
        self.assertEqual(src_scale["total_corpus_source_bytes"], 40246)
        v3_cov = gate.summarize_v3_sanitizer_coverage(loaded)
        self.assertEqual(
            list(v3_cov.keys()),
            [
                "deterministic_deinit_drop",
                "heap_class_rc_and_box",
                "string_and_text_owner",
                "collection_list_and_map_provider",
                "closure_env_and_thread_spawn",
                "checked_abort_and_overflow",
            ],
        )
        self.assertEqual(v3_cov["deterministic_deinit_drop"]["status"], "all_runnable")
        self.assertEqual(v3_cov["collection_list_and_map_provider"]["status"], "all_runnable")
        self.assertEqual(v3_cov["checked_abort_and_overflow"]["status"], "all_runnable")
        self.assertEqual(v3_cov["heap_class_rc_and_box"]["status"], "partial")
        self.assertEqual(v3_cov["string_and_text_owner"]["status"], "partial")
        self.assertEqual(v3_cov["closure_env_and_thread_spawn"]["status"], "partial")
        readme_prog_tbl = gate.render_readme_program_table(loaded)
        self.assertIn("| `word_count` | 词频 |", readme_prog_tbl)
        self.assertIn("| `raii_state_machine` | RAII 状态机 |", readme_prog_tbl)
        repo_readme_text = (repo_corpus / "README.md").read_text(encoding="utf-8")
        self.assertIn(readme_prog_tbl, repo_readme_text)
        self.assertEqual(
            gate.sync_readme_chapter_table(repo_readme_text, loaded),
            repo_readme_text,
        )

        # Verify LF trailing-newline source hygiene and source_lines / source_bytes checks
        p01_path = self.corpus / "probes" / "p01_ok.ko"
        orig_p01 = p01_path.read_bytes()
        try:
            crlf_bytes = b'fun main(): Unit { println("hello") }\r\n'
            p01_path.write_bytes(crlf_bytes)
            with self.assertRaisesRegex(
                ValueError, "must be non-empty, use LF line endings, and end with trailing newline"
            ):
                gate.derive_static_expectations(self.corpus, self.baseline_path)
            self._write_default_expectations(
                {
                    "p01_ok": {
                        "kind": "run_ok",
                        "source_sha256": hashlib.sha256(crlf_bytes).hexdigest(),
                        "stdout": "hello\n",
                    }
                }
            )
            with self.assertRaisesRegex(
                ValueError, "must be non-empty, use LF line endings, and end with trailing newline"
            ):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            p01_path.write_bytes(orig_p01)
            self._write_default_expectations()

        for bad_metric_spec, expected_err in [
            ({"source_lines": 0}, "invalid source_lines"),
            ({"source_bytes": True}, "invalid source_bytes"),
            ({"source_lines": 999}, "source_lines mismatch"),
            ({"source_bytes": 999}, "source_bytes mismatch"),
        ]:
            spec_override = {
                "kind": "run_ok",
                "source_sha256": _sha256(self.probe_ok_src),
                "stdout": "hello\n",
                **bad_metric_spec,
            }
            self._write_default_expectations({"p01_ok": spec_override})
            with self.assertRaisesRegex(ValueError, expected_err):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        self._write_default_expectations()

        verdict_no_frame, reason_no_frame = gate.evaluate_mode_against_oracle(
            {
                "marker": "abort:boom",
                "exit": 134,
                "stdout_bytes": b"",
                "stderr_bytes": b"abort: boom\n",
            },
            {"kind": "runtime_abort", "abort_message": "boom"},
        )
        self.assertEqual(verdict_no_frame, "wrong_output")
        self.assertIn("stack frame", reason_no_frame or "")

    def test_16_conformance_modules_respect_1000_line_srp_budget(self) -> None:
        """Verify all conformance implementation and test modules stay <= 1000 lines per SRP."""
        repo_root = Path(__file__).resolve().parents[2]
        expected_modules = [
            repo_root / "scripts" / "conformance_catalogs.py",
            repo_root / "scripts" / "conformance_expectations.py",
            repo_root / "scripts" / "conformance_runner.py",
            repo_root / "scripts" / "conformance_evidence.py",
            repo_root / "scripts" / "conformance_audit.py",
            repo_root / "scripts" / "conformance_gate.py",
            repo_root / "scripts" / "tests" / "test_conformance_gate.py",
            repo_root / "scripts" / "tests" / "test_conformance_audit.py",
        ]
        for mod_path in expected_modules:
            self.assertTrue(mod_path.is_file(), f"missing SRP module: {mod_path}")
            line_count = len(mod_path.read_text(encoding="utf-8").splitlines())
            self.assertLessEqual(
                line_count,
                1000,
                f"{mod_path.relative_to(repo_root)} has {line_count} lines (exceeds 1000-line SRP budget)",
            )


if __name__ == "__main__":
    unittest.main()
