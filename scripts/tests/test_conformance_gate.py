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
            ledger["summary"]["by_batch"]["p"],
            {
                "total": 4,
                "pass_run": 1,
                "pass_diagnostic": 1,
                "pass_abort": 1,
                "known_unimplemented": 1,
                "capability_passed": 3,
            },
        )
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stdout").read_bytes(), b"hello\n"
        )
        self.assertEqual(
            (evidence_dir / "raw" / "p01_ok.single.stderr").read_bytes(), b""
        )
        self.assertTrue((evidence_dir / "results.json").is_file())
        self.assertTrue((evidence_dir / "matrix.tsv").is_file())
        self.assertTrue((evidence_dir / "SUMMARY.md").is_file())
        self.assertEqual(
            (evidence_dir / "SUMMARY.md").read_text(encoding="utf-8"),
            gate.render_evidence_summary_markdown(ledger),
        )

        verified = gate.verify_evidence_bundle(
            evidence_dir=evidence_dir,
            expectations_path=self.expectations_path,
            corpus_root=self.corpus,
            baseline_path=self.baseline_path,
        )
        self.assertEqual(verified["summary"], ledger["summary"])
        self.assertEqual(
            gate.main(
                [
                    "--verify-evidence",
                    str(evidence_dir),
                    "--corpus",
                    str(self.corpus),
                    "--expectations",
                    str(self.expectations_path),
                    "--baseline",
                    str(self.baseline_path),
                ]
            ),
            0,
        )
        self.assertEqual(
            gate.main(
                [
                    "--check-matrix",
                    str(evidence_dir / "matrix.tsv"),
                    "--corpus",
                    str(self.corpus),
                    "--expectations",
                    str(self.expectations_path),
                    "--baseline",
                    str(self.baseline_path),
                ]
            ),
            0,
        )
        # Tamper with raw stdout file and verify offline bundle verification catches it
        (evidence_dir / "raw" / "p01_ok.single.stdout").write_bytes(b"tampered\n")
        try:
            with self.assertRaisesRegex(ValueError, "raw stdout mismatch for probes.p01_ok.single"):
                gate.verify_evidence_bundle(
                    evidence_dir=evidence_dir,
                    expectations_path=self.expectations_path,
                    corpus_root=self.corpus,
                    baseline_path=self.baseline_path,
                )
        finally:
            (evidence_dir / "raw" / "p01_ok.single.stdout").write_bytes(b"hello\n")

        # Tamper with SUMMARY.md and verify offline bundle verification catches it
        orig_summary_md = (evidence_dir / "SUMMARY.md").read_text(encoding="utf-8")
        (evidence_dir / "SUMMARY.md").write_text("# Tampered\n", encoding="utf-8")
        try:
            with self.assertRaisesRegex(ValueError, "SUMMARY.md mismatch in evidence bundle"):
                gate.verify_evidence_bundle(
                    evidence_dir=evidence_dir,
                    expectations_path=self.expectations_path,
                    corpus_root=self.corpus,
                    baseline_path=self.baseline_path,
                )
        finally:
            (evidence_dir / "SUMMARY.md").write_text(orig_summary_md, encoding="utf-8")

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

        extra_prog_dir = self.corpus / "programs" / "unlisted_prog"
        extra_prog_dir.mkdir(parents=True)
        (extra_prog_dir / "main.ko").write_text(self.prog_src, encoding="utf-8")
        (extra_prog_dir / "expected.txt").write_text("ok\n", encoding="utf-8")
        try:
            with self.assertRaisesRegex(ValueError, "program inventory mismatch"):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            (extra_prog_dir / "main.ko").unlink()
            (extra_prog_dir / "expected.txt").unlink()
            extra_prog_dir.rmdir()

        # Verify expected.txt without trailing newline is rejected
        exp_txt_path = self.corpus / "programs" / "word_count" / "expected.txt"
        exp_txt_path.write_text("word\t1", encoding="utf-8")
        try:
            with self.assertRaisesRegex(
                ValueError, "must be non-empty and end with trailing newline"
            ):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            exp_txt_path.write_text(self.prog_expected, encoding="utf-8")

        # Verify probe with top-level package directive or missing/duplicate fun main is rejected
        p01_path = self.corpus / "probes" / "p01_ok.ko"
        for bad_src, err_re in [
            ('package custom\nfun main(): Unit { println("hello") }\n', "must not contain a package directive"),
            ('fun helper(): Unit { println("hello") }\n', "must declare a single fun main"),
            ('fun main(): Unit { println("a") }\nfun main(): Unit { println("b") }\n', "must declare a single fun main"),
        ]:
            p01_path.write_text(bad_src, encoding="utf-8")
            self._write_default_expectations(
                {"p01_ok": {"kind": "run_ok", "source_sha256": _sha256(bad_src), "stdout": "hello\n"}}
            )
            try:
                with self.assertRaisesRegex(ValueError, err_re):
                    gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
            finally:
                p01_path.write_text(self.probe_ok_src, encoding="utf-8")
                self._write_default_expectations()

        # Verify known_unimplemented with invalid or internal-error allowed_markers is rejected
        for bad_markers in ([], ["BE:InvalidSsa"], ["ICE:ownership"], ["BE:UnsupportedNode", "BE:UnsupportedNode"]):
            self._write_default_expectations(
                {
                    "p04_unimpl": {
                        "kind": "known_unimplemented",
                        "source_sha256": _sha256(self.probe_unimpl_src),
                        "allowed_markers": bad_markers,
                        "target_stdout": "3.0\n",
                    }
                }
            )
            try:
                with self.assertRaisesRegex(ValueError, "invalid allowed_markers"):
                    gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
            finally:
                self._write_default_expectations()

        # Verify invalid spec_refs format or duplicate entries are rejected
        for bad_srefs in ([], ["0355"], ["SPEC-0355", "SPEC-0355"]):
            self._write_default_expectations(
                {
                    "p03_abort": {
                        "kind": "runtime_abort",
                        "source_sha256": _sha256(self.probe_abort_src),
                        "abort_message": "boom",
                        "v046_baseline_exempt": True,
                        "spec_refs": bad_srefs,
                    }
                }
            )
            try:
                with self.assertRaisesRegex(ValueError, "invalid spec_refs"):
                    gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
            finally:
                self._write_default_expectations()

        # Verify run_ok without trailing newline in stdout or invalid oracle_origin is rejected
        self._write_default_expectations(
            {
                "p01_ok": {
                    "kind": "run_ok",
                    "source_sha256": _sha256(self.probe_ok_src),
                    "stdout": "hello",
                }
            }
        )
        try:
            with self.assertRaisesRegex(ValueError, "ending with newline"):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            self._write_default_expectations()

        self._write_default_expectations(
            {
                "p01_ok": {
                    "kind": "run_ok",
                    "source_sha256": _sha256(self.probe_ok_src),
                    "stdout": "hello\n",
                    "oracle_origin": "bogus_origin",
                }
            }
        )
        try:
            with self.assertRaisesRegex(ValueError, "invalid oracle_origin"):
                gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
        finally:
            self._write_default_expectations()

        # Verify expected_lines/expected_bytes mismatch or stdout on negative probes is rejected
        for bad_spec, err_re in [
            (
                {"p01_ok": {"kind": "run_ok", "source_sha256": _sha256(self.probe_ok_src), "stdout": "hello\n", "expected_lines": 2}},
                "expected_lines mismatch",
            ),
            (
                {"p01_ok": {"kind": "run_ok", "source_sha256": _sha256(self.probe_ok_src), "stdout": "hello\n", "expected_bytes": 99}},
                "expected_bytes mismatch",
            ),
            (
                {"p02_diag": {"kind": "compile_fail", "source_sha256": _sha256(self.probe_diag_src), "diagnostic": "L0084", "stdout": "x\n"}},
                "must not define stdout",
            ),
            (
                {"p01_ok": {"kind": "run_ok", "source_sha256": _sha256(self.probe_ok_src), "stdout": "hello\n", "probe_batch": "q"}},
                "invalid probe_batch",
            ),
            (
                {"p04_unimpl": {"kind": "known_unimplemented", "source_sha256": _sha256(self.probe_unimpl_src), "baseline_marker": "FE:L0165", "allowed_markers": ["BE:UnsupportedNode"], "target_stdout": "3.0\n"}},
                "must include non-exempt baseline_marker",
            ),
        ]:
            self._write_default_expectations(bad_spec)
            try:
                with self.assertRaisesRegex(ValueError, err_re):
                    gate.load_expectations(self.expectations_path, corpus_root=self.corpus)
            finally:
                self._write_default_expectations()

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
        matrix_ev_dir = self.root / "matrix_ev"
        env = dict(os.environ)
        env["KOVENC"] = str(kovenc_obj)
        env["KOVEN_CONFORMANCE_EVIDENCE"] = str(matrix_ev_dir)
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
        verified_matrix = gate.verify_matrix_evidence_dir(
            matrix_ev_dir, expected_names={"p28_companion"}
        )
        self.assertEqual(verified_matrix["manifest"]["probe_count"], 1)

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
        parity = gate.compare_dual_host_evidence(
            evidence_dir_a=ev_seq,
            evidence_dir_b=ev_par,
            expectations_path=self.expectations_path,
            corpus_root=self.corpus,
            baseline_path=self.baseline_path,
        )
        self.assertTrue(parity["parity_passed"])
        self.assertEqual(
            gate.main(
                [
                    "--compare-evidence",
                    str(ev_seq),
                    str(ev_par),
                    "--corpus",
                    str(self.corpus),
                    "--expectations",
                    str(self.expectations_path),
                    "--baseline",
                    str(self.baseline_path),
                ]
            ),
            0,
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

    def test_13_derivation_reports_naming_diagnostic_instead_of_baseline_keyerror(
        self,
    ) -> None:
        """违规命名的 probe 源文件必须以命名诊断失败。

        意图：新增 probe 时命名写错，作者需要看到"违反了哪条命名约定"；
        若落到 baseline[name] 的 KeyError，作者只会看到缺行，误以为是漏填 baseline。
        """
        (self.corpus / "probes" / "zz9_bad_name.ko").write_text(
            self.probe_ok_src, encoding="utf-8"
        )
        with self.assertRaises(ValueError) as ctx:
            gate.derive_static_expectations(self.corpus, self.baseline_path)
        message = str(ctx.exception)
        self.assertIn("zz9_bad_name", message)
        self.assertIn("naming convention", message)

    def test_14_named_probe_absent_from_baseline_matrix_reports_missing_row(
        self,
    ) -> None:
        """命名合法但 baseline 缺项或单文件/工程列不一致时必须报 ValueError。"""
        (self.corpus / "probes" / "p06_absent.ko").write_text(
            self.probe_ok_src, encoding="utf-8"
        )
        with self.assertRaises(ValueError) as ctx:
            gate.derive_static_expectations(self.corpus, self.baseline_path)
        message = str(ctx.exception)
        self.assertIn("p06_absent", message)
        self.assertIn("baseline", message)
        (self.corpus / "probes" / "p06_absent.ko").unlink()

        orig_baseline = self.baseline_path.read_text(encoding="utf-8")
        self.baseline_path.write_text(
            orig_baseline.replace("p01_ok\tOK\tOK", "p01_ok\tOK\tFE:L0084"),
            encoding="utf-8",
        )
        try:
            with self.assertRaisesRegex(ValueError, "baseline mode mismatch for probes.p01_ok"):
                gate.derive_static_expectations(self.corpus, self.baseline_path)
        finally:
            self.baseline_path.write_text(orig_baseline, encoding="utf-8")

    def test_15_cli_init_expectations_renders_derivation_error_without_traceback(
        self,
    ) -> None:
        """CLI 边界必须把推导错误与运行期参数错误渲染成单行诊断并返回 1。"""
        (self.corpus / "probes" / "zz9_bad_name.ko").write_text(
            self.probe_ok_src, encoding="utf-8"
        )
        stderr = io.StringIO()
        with contextlib.redirect_stderr(stderr):
            rc = gate.main(
                [
                    "--init-expectations",
                    "--corpus",
                    str(self.corpus),
                    "--baseline",
                    str(self.baseline_path),
                    "--expectations",
                    str(self.root / "derived.json"),
                ]
            )
        self.assertEqual(rc, 1)
        self.assertIn("conformance: error:", stderr.getvalue())
        self.assertIn("naming convention", stderr.getvalue())
        self.assertFalse((self.root / "derived.json").exists())
        (self.corpus / "probes" / "zz9_bad_name.ko").unlink()

        for bad_args, expected_err in [
            (["--cli", str(self.root / "missing_kovenc"), "--corpus", str(self.corpus), "--expectations", str(self.expectations_path)], "conformance: error:"),
            (["--jobs", "0", "--corpus", str(self.corpus), "--expectations", str(self.expectations_path)], "jobs must be >= 1"),
            (["--timeout", "0", "--corpus", str(self.corpus), "--expectations", str(self.expectations_path)], "timeout_sec must be > 0"),
            (["--program", "word_count", "--no-programs", "--corpus", str(self.corpus), "--expectations", str(self.expectations_path)], "cannot combine --program with --no-programs"),
        ]:
            err_buf = io.StringIO()
            with contextlib.redirect_stderr(err_buf):
                self.assertEqual(gate.main(bad_args), 1)
            self.assertIn(expected_err, err_buf.getvalue())

        self.assertEqual(gate.resolve_chapter_selector("09"), "09-nullability-errors.md")
        self.assertEqual(
            gate.resolve_chapter_selector("09-nullability-errors.md"),
            "09-nullability-errors.md",
        )
        with self.assertRaisesRegex(ValueError, "unknown Guide chapter selector"):
            gate.resolve_chapter_selector("99")

        kovenc = _make_mock_compiler(
            self.root / "kovenc_filter",
            'sys.stdout.buffer.write(b"hello\\n"); sys.exit(0)\n',
        )
        self._write_default_expectations(
            {
                "p01_ok": {
                    "kind": "run_ok",
                    "source_sha256": _sha256(self.probe_ok_src),
                    "guide_chapter": "05-declarations-callables.md",
                    "probe_batch": "p",
                    "stdout": "hello\n",
                }
            }
        )
        ledger_filtered = gate.evaluate_suite(
            kovenc=kovenc,
            corpus_root=self.corpus,
            expectations_path=self.expectations_path,
            selected_batches=["p"],
            selected_chapters=["05"],
            include_programs=False,
        )
        self.assertEqual([p["comparison"]["name"] for p in ledger_filtered["probes"]], ["p01_ok"])
        self.assertEqual(
            ledger_filtered["summary"]["foundational_p2"],
            {
                "group_a_evaluated": 1,
                "group_a_runnable_passed": 1,
                "group_b_evaluated": 0,
                "group_b_diagnostic_passed": 0,
                "group_c_evaluated": 0,
                "p2_target_min": 70,
                "p2_gate_met": False,
            },
        )
        with self.assertRaisesRegex(ValueError, "no probes matched the requested filters"):
            gate.evaluate_suite(
                kovenc=kovenc,
                corpus_root=self.corpus,
                expectations_path=self.expectations_path,
                selected_batches=["e"],
                include_programs=False,
            )


if __name__ == "__main__":
    unittest.main()
