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

        # Verify probe with top-level package directive or missing fun main is rejected
        p01_path = self.corpus / "probes" / "p01_ok.ko"
        for bad_src, err_re in [
            ('package custom\nfun main(): Unit { println("hello") }\n', "must not contain a package directive"),
            ('fun helper(): Unit { println("hello") }\n', "must declare a fun main"),
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
        self.assertEqual(len(base_evo["promoted_to_run_ok"]), 27)
        self.assertEqual(len(base_evo["upgraded_to_structured_abort"]), 3)
        self.assertEqual(len(base_evo["upgraded_to_compile_fail"]), 1)
        self.assertEqual(len(base_evo["baseline_compile_fail_preserved"]), 13)
        self.assertEqual(len(base_evo["remaining_known_unimplemented"]), 21)
        self.assertEqual(
            sum(len(v) for v in base_evo.values()),
            len(loaded["probes"]),
        )

        # 11. Verify all 35 v046_baseline_exempt probes have resolved spec_refs in docs/specs or docs/archive/specs
        exempt_probes = [
            name for name, spec in loaded["probes"].items() if spec.get("v046_baseline_exempt")
        ]
        self.assertEqual(len(exempt_probes), 35)
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
        self.assertEqual(len(spec_trace), 14)
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
        self.assertEqual(origin_stats["v046_spec_contract"]["probes"], 35)
        self.assertEqual(origin_stats["baseline_diagnostic_audited"]["probes"], 13)
        self.assertEqual(origin_stats["baseline_unimplemented_audited"]["probes"], 18)
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


if __name__ == "__main__":
    unittest.main()


