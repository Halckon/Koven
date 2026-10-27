#!/usr/bin/env python3
"""Unit tests for the public-entry conformance runner, oracle comparator, and regression gate."""

from __future__ import annotations

import hashlib
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


class ConformanceGateSelfTests(unittest.TestCase):
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

    def test_1_all_expected_cases_pass_and_raw_evidence_files_written(self) -> None:
        kovenc = _make_mock_compiler(
            self.root / "kovenc",
            """
args = sys.argv[1:]
is_project = "--project" in args
src = open("src/app/main.ko", "rb").read() if is_project else open(args[-1], "rb").read()
if is_project:
    assert src.startswith(b"package app\\n"), "missing package app header"
if b'println("hello")' in src:
    sys.stdout.buffer.write(b"hello\\n")
    sys.exit(0)
if b"val x = 1" in src:
    sys.stderr.buffer.write(b"error[L0084]: type mismatch\\n")
    sys.exit(1)
if b'error("boom")' in src:
    sys.stderr.buffer.write(b"abort: boom\\n  at main.ko:1:19\\n")
    sys.exit(134)
sys.stderr.buffer.write(b"lowering failed with UnsupportedNode\\n")
sys.exit(1)
""",
        )
        evidence_dir = self.root / "evidence"
        ledger = gate.evaluate_suite(
            kovenc=kovenc,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            baseline_path=self.baseline_path,
            compiler_commit="05b1f3b6",
            evidence_dir=evidence_dir,
        )
        self.assertTrue(ledger["summary"]["gate_passed"], ledger["summary"])
        self.assertEqual(ledger["compiler"]["commit"], "05b1f3b6")
        self.assertEqual(ledger["summary"]["probe_capability_passed"], 3)
        self.assertEqual(ledger["summary"]["counts"]["known_unimplemented"], 2)
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stdout").read_bytes(), b"hello\n"
        )
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stderr").read_bytes(), b""
        )
        self.assertTrue((evidence_dir / "results.json").is_file())
        self.assertTrue((evidence_dir / "matrix.tsv").is_file())

    def test_2_wrong_output_or_missing_trailing_newline_with_exit_0_is_rejected(self) -> None:
        for name, snippet in [
            ("wrong_text", 'sys.stdout.buffer.write(b"wrong\\n"); sys.exit(0)'),
            ("missing_nl", 'sys.stdout.buffer.write(b"hello"); sys.exit(0)'),
            ("silent", "sys.exit(0)"),
            (
                "dirty_stderr",
                'sys.stdout.buffer.write(b"hello\\n"); sys.stderr.buffer.write(b"warn\\n"); sys.exit(0)',
            ),
        ]:
            with self.subTest(case=name):
                kovenc = _make_mock_compiler(self.root / f"kovenc_{name}", snippet + "\n")
                ledger = gate.evaluate_suite(
                    kovenc=kovenc,
                    corpus_root=self.corpus,
                    expectations_path=self.expectations_path,
                    selected_probes=["p01_ok"],
                    include_programs=False,
                )
                self.assertFalse(ledger["summary"]["gate_passed"])
                self.assertEqual(
                    ledger["probes"][0]["comparison"]["status"], "wrong_output"
                )

    def test_3_missing_or_duplicate_probes_in_matrix_or_inventory_are_rejected(self) -> None:
        with self.assertRaisesRegex(ValueError, "duplicate probe row"):
            gate.parse_tsv_matrix("p01_ok\tOK\tOK\np01_ok\tOK\tOK\n")

        with self.assertRaisesRegex(ValueError, "missing probe rows"):
            gate.parse_tsv_matrix(
                "p01_ok\tOK\tOK\n",
                expected_names={"p01_ok", "p02_diag"},
            )

        extra_file = self.corpus / "probes" / "p99_unlisted.ko"
        extra_file.write_text('fun main(): Unit { println("x") }\n', encoding="utf-8")
        try:
            with self.assertRaisesRegex(ValueError, "probe inventory mismatch"):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            extra_file.unlink()

    def test_4_timeout_is_caught_and_preserves_partial_bytes(self) -> None:
        kovenc_slow = _make_mock_compiler(
            self.root / "kovenc_slow",
            """
os.write(1, b"partial-out")
os.write(2, b"partial-err")
time.sleep(5)
""",
        )
        evidence_dir = self.root / "evidence_timeout"
        ledger = gate.evaluate_suite(
            kovenc=kovenc_slow,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            selected_probes=["p01_ok"],
            include_programs=False,
            timeout_sec=3.0,
            evidence_dir=evidence_dir,
        )
        self.assertFalse(ledger["summary"]["gate_passed"])
        comp = ledger["probes"][0]["comparison"]
        self.assertEqual(comp["status"], "internal_error")
        self.assertEqual(comp["single_marker"], "timeout")
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stdout").read_bytes(),
            b"partial-out",
        )
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stderr").read_bytes(),
            b"partial-err",
        )
        status_json = json.loads(
            (evidence_dir / "raw" / "p01_ok.single.status.json").read_text(
                encoding="utf-8"
            )
        )
        self.assertEqual(status_json, {"exit": 124, "timed_out": True})

    def test_5_regression_and_both_modes_rejected_never_count_as_pass(self) -> None:
        kovenc_regress = _make_mock_compiler(
            self.root / "kovenc_regress",
            """
sys.stderr.buffer.write(b"lowering failed with UnsupportedNode\\n")
sys.exit(1)
""",
        )
        ledger = gate.evaluate_suite(
            kovenc=kovenc_regress,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            selected_probes=["p01_ok", "p04_unimpl"],
            include_programs=False,
        )
        self.assertFalse(ledger["summary"]["gate_passed"])
        self.assertEqual(ledger["summary"]["probe_capability_passed"], 0)
        p01_comp = ledger["probes"][0]["comparison"]
        self.assertTrue(p01_comp["mode_consistent"])
        self.assertEqual(p01_comp["status"], "regression")
        self.assertFalse(p01_comp["capability_passed"])

    def test_6_mode_mismatch_between_single_and_project_is_rejected(self) -> None:
        kovenc_split = _make_mock_compiler(
            self.root / "kovenc_split",
            """
if "--project" in sys.argv[1:]:
    sys.stderr.buffer.write(b"lowering failed with MissingFact\\n")
    sys.exit(1)
sys.stdout.buffer.write(b"hello\\n")
sys.exit(0)
""",
        )
        ledger = gate.evaluate_suite(
            kovenc=kovenc_split,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            selected_probes=["p01_ok"],
            include_programs=False,
        )
        self.assertFalse(ledger["summary"]["gate_passed"])
        self.assertEqual(ledger["probes"][0]["comparison"]["status"], "mode_mismatch")

    def test_7_unreviewed_item_blocks_strict_gate_and_derive_static_marks_complex_probes_unreviewed(
        self,
    ) -> None:
        complex_src = 'fun main(): Unit { for (i in 0..<2) { println("line") } }\n'
        (self.corpus / "probes" / "p05_complex.ko").write_text(
            complex_src, encoding="utf-8"
        )
        self.baseline_path.write_text(
            self.baseline_path.read_text(encoding="utf-8") + "p05_complex\tOK\tOK\n",
            encoding="utf-8",
        )
        derived = gate.derive_static_expectations(self.corpus, self.baseline_path)
        self.assertEqual(derived["probes"]["p01_ok"]["kind"], "run_ok")
        self.assertEqual(derived["probes"]["p01_ok"]["stdout"], "hello\n")
        self.assertEqual(derived["probes"]["p05_complex"]["kind"], "unreviewed")
        self.expectations_path.write_text(json.dumps(derived, indent=2), encoding="utf-8")

        kovenc = _make_mock_compiler(
            self.root / "kovenc_unrev",
            """
sys.stdout.buffer.write(b"line\\nline\\n")
sys.exit(0)
""",
        )
        ledger_strict = gate.evaluate_suite(
            kovenc=kovenc,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            selected_probes=["p05_complex"],
            include_programs=False,
            strict_unreviewed=True,
        )
        self.assertFalse(ledger_strict["summary"]["gate_passed"])
        self.assertEqual(
            ledger_strict["probes"][0]["comparison"]["status"], "unreviewed"
        )

    def test_8_cli_init_expectations_and_gate_exit_codes(self) -> None:
        out_exp = self.root / "derived_expectations.json"
        rc_init = gate.main(
            [
                "--init-expectations",
                "--corpus",
                str(self.corpus),
                "--baseline",
                str(self.baseline_path),
                "--expectations",
                str(out_exp),
            ]
        )
        self.assertEqual(rc_init, 0)
        self.assertTrue(out_exp.is_file())

    def test_9_backend_object_marker_is_not_an_internal_error(self) -> None:
        marker = gate.classify_raw(
            2,
            False,
            b"",
            b"error: native build failed: native object UnsupportedSource: companion function calls are not supported\n",
        )
        self.assertEqual(marker, "BE:UnsupportedSource")
        self.assertFalse(gate.is_internal_error_marker(marker))
        self.assertEqual(
            gate.classify_raw(
                2,
                False,
                b"",
                b"error: native build failed: native object InvalidModel: frontend lowering failed with MissingFact\n",
            ),
            "BE:MissingFact",
        )

    def test_10_repo_expectations_match_static_derivation_and_matrix_sh_recognizes_object_markers(
        self,
    ) -> None:
        repo_root = Path(__file__).resolve().parents[2]
        repo_corpus = repo_root / "conformance"
        repo_exp_path = repo_corpus / "expectations.json"
        repo_base_path = repo_corpus / "baseline-aef3f4f8.tsv"

        loaded = gate.load_expectations(repo_exp_path, corpus_root=repo_corpus)
        self.assertEqual(len(loaded["probes"]), 154)
        self.assertEqual(len(loaded["programs"]), 6)

        raw_checked_in = json.loads(repo_exp_path.read_text(encoding="utf-8"))
        derived = gate.derive_static_expectations(repo_corpus, repo_base_path)
        self.assertEqual(derived, raw_checked_in)

        probe_dir = self.root / "mini_probes"
        probe_dir.mkdir(parents=True)
        (probe_dir / "p28_companion.ko").write_text(
            'fun main(): Unit { println("companion") }\n', encoding="utf-8"
        )
        kovenc_obj = _make_mock_compiler(
            self.root / "kovenc_obj",
            """
sys.stderr.buffer.write(b"error: native build failed: native object UnsupportedSource: companion function calls are not supported\\n")
sys.exit(2)
""",
        )
        env = dict(os.environ)
        env["KOVENC"] = str(kovenc_obj)
        out = subprocess.check_output(
            ["bash", str(repo_root / "scripts" / "conformance_matrix.sh"), str(probe_dir)],
            cwd=repo_root,
            env=env,
            timeout=10.0,
        ).decode("utf-8")
        self.assertEqual(
            out,
            "p28_companion\tBE:UnsupportedSource\tBE:UnsupportedSource\n",
        )

    def test_11_parallel_jobs_preserve_deterministic_order_and_docs_checker_validates_corpus(
        self,
    ) -> None:
        from check_docs import DocsChecker

        kovenc = _make_mock_compiler(
            self.root / "kovenc_parallel",
            """
args = sys.argv[1:]
is_project = "--project" in args
src = open("src/app/main.ko", "rb").read() if is_project else open(args[-1], "rb").read()
if b'println("hello")' in src:
    sys.stdout.buffer.write(b"hello\\n")
    sys.exit(0)
if b"val x = 1" in src:
    sys.stderr.buffer.write(b"error[L0084]: type mismatch\\n")
    sys.exit(1)
if b'error("boom")' in src:
    sys.stderr.buffer.write(b"abort: boom\\n  at main.ko:1:19\\n")
    sys.exit(134)
sys.stderr.buffer.write(b"lowering failed with UnsupportedNode\\n")
sys.exit(1)
""",
        )
        ev_seq = self.root / "ev_seq"
        ev_par = self.root / "ev_par"
        ledger_seq = gate.evaluate_suite(
            kovenc=kovenc,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            baseline_path=self.baseline_path,
            compiler_commit="655772d8",
            evidence_dir=ev_seq,
            jobs=1,
        )
        ledger_par = gate.evaluate_suite(
            kovenc=kovenc,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            baseline_path=self.baseline_path,
            compiler_commit="655772d8",
            evidence_dir=ev_par,
            jobs=4,
        )
        self.assertEqual(
            [p["comparison"] for p in ledger_seq["probes"]],
            [p["comparison"] for p in ledger_par["probes"]],
        )
        self.assertEqual(
            (ev_seq / "matrix.tsv").read_text(encoding="utf-8"),
            (ev_par / "matrix.tsv").read_text(encoding="utf-8"),
        )

        # Verify DocsChecker.check_conformance_corpus detects tampered expectations.json
        checker = DocsChecker(self.root)
        checker.check_conformance_corpus()
        self.assertEqual(checker.errors, [])

        bad_data = json.loads(self.expectations_path.read_text(encoding="utf-8"))
        bad_data["probes"]["p01_ok"]["source_sha256"] = "0" * 64
        self.expectations_path.write_text(json.dumps(bad_data), encoding="utf-8")
        checker_bad = DocsChecker(self.root)
        checker_bad.check_conformance_corpus()
        self.assertEqual(len(checker_bad.errors), 1)
        self.assertIn("source_sha256 mismatch for probes.p01_ok", checker_bad.errors[0])

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


if __name__ == "__main__":
    unittest.main()


