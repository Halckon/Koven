"""Required generated batches must have real exporter hits and complete cases."""
import tempfile
from pathlib import Path
import unittest
from scripts import check_generated_owners as gate


class GeneratedDriverTests(unittest.TestCase):
    def test_evidence_write_errors_cannot_replace_the_first_case_failure(self):
        import contextlib
        import io
        import json
        from unittest import mock
        failure = gate.Failure("frontend", "diagnostic_mismatch", "L0131:primary")
        real_write = gate.write_json
        for failed_file in ("verdict.json", "minimization.json"):
            with self.subTest(failed_file=failed_file), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary) / "artifacts"
                executor = mock.Mock()
                executor.case.side_effect = failure
                def write(path, value):
                    if path.name == failed_file:
                        raise OSError("controlled evidence write unavailable")
                    real_write(path, value)
                with mock.patch.object(gate, "Execution", return_value=executor), \
                        mock.patch.object(gate, "write_json", side_effect=write), \
                        contextlib.redirect_stderr(io.StringIO()):
                    self.assertEqual(1, gate.main(["--artifacts", str(root)]))
                report = json.loads((root / "failure.json").read_text())
                self.assertEqual(failure.record(), report["first_failure"])
                self.assertEqual(failure.kind, report["cases"][0]["kind"])
                self.assertEqual("OSError", report["evidence_failure"]["witness"])

    def test_replay_io_error_preserves_first_failure(self):
        import contextlib
        import io
        import json
        from unittest import mock
        failure = gate.Failure("frontend", "diagnostic_mismatch", "L0131:primary")
        executor = mock.Mock()
        executor.case.side_effect = [failure, FileNotFoundError("missing replay diagnostics.tsv")]
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "artifacts"
            with mock.patch.object(gate, "Execution", return_value=executor), \
                    contextlib.redirect_stderr(io.StringIO()) as stderr:
                status = gate.main(["--artifacts", str(root)])
            self.assertEqual(status, 1)
            report = json.loads((root / "failure.json").read_text())
            self.assertEqual(report["first_failure"], failure.record())
            self.assertIn(str(failure), stderr.getvalue())
            reduced = json.loads((root / "minimization.json").read_text())
            self.assertEqual(reduced["status"], "minimization_incomplete")
            self.assertEqual(reduced["original_failure"], failure.record())
            self.assertEqual(reduced["minimal"], reduced["original"])
            self.assertEqual(reduced["confirmation_count"], 0)
            self.assertEqual(reduced["reduction_error"]["witness"], "FileNotFoundError")
            self.assertIn("missing replay diagnostics.tsv", reduced["reduction_error"]["detail"])

    def test_exporter_binary_comes_from_unique_cargo_test_artifact(self):
        import json
        row = {"reason": "compiler-artifact", "target": {"name": "lang_codegen"},
               "profile": {"test": True}, "executable": "/tmp/test-bin"}
        self.assertEqual(gate.test_binary(json.dumps(row)), Path("/tmp/test-bin"))
        for text in ("", json.dumps({**row, "executable": None}),
                     json.dumps(row) + "\n" + json.dumps({**row, "executable": "/tmp/other"})):
            with self.assertRaises(gate.Failure):
                gate.test_binary(text)

    def test_export_test_must_run_exactly_once(self):
        good = f"test {gate.EXPORT_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 800 filtered out;"
        gate.assert_export(good)
        for value in ("", good.replace("1 passed", "0 passed"), good.replace("0 ignored", "1 ignored"),
                      good.replace(gate.EXPORT_TEST, "other")):
            with self.assertRaises(gate.Failure):
                gate.assert_export(value)

    def test_batch_cannot_drop_cases_or_duplicate_one(self):
        expected = ["a", "b"]
        gate.assert_complete(expected, [{"id": "a", "status": "pass"}, {"id": "b", "status": "pass"}])
        for observed in ([], [{"id": "a", "status": "pass"}],
                         [{"id": "a", "status": "pass"}]*2,
                         [{"id": "a", "status": "pass"}, {"id": "b", "status": "failure"}]):
            with self.assertRaises(gate.Failure):
                gate.assert_complete(expected, observed)

    def test_preparing_case_seals_model_and_source_for_replay(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            case = gate.model.cases()[0]
            directory = root / "case"
            gate.prepare_case(case, directory)
            saved = gate.checks.verify_inputs(directory)
            self.assertTrue({"case.ko", "case.json", "expected.stdout", "oracle.json",
                             "expected-diagnostics.json", "expected-allocations.txt"} <= set(saved))
            replay = root / "replay"
            gate.copy_replay(directory, replay)
            for name in saved:
                self.assertEqual((directory / name).read_bytes(), (replay / name).read_bytes())
            with self.assertRaises(FileExistsError):
                gate.copy_replay(directory, replay)

    def test_acceptance_aggregates_g3_g4_g5_and_records_partial_when_detectors_skipped(self):
        import contextlib
        import io
        import json
        from unittest import mock
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary) / "artifacts"
            executor = mock.Mock()
            executor.case.return_value = None  # all cases pass
            calib_rep = dict(status="partial", skipped_reasons=["v1:macos-counter-only"])
            checker_rep = dict(status="pass", mutant_killed=True)
            reduct_rep = dict(status="reproduced", confirmation_count=3)

            with mock.patch.object(gate, "Execution", return_value=executor), \
                    mock.patch.object(gate.calibration, "verify", return_value=calib_rep), \
                    mock.patch.object(gate.checker_mutation, "verify_checker_mutant", return_value=checker_rep), \
                    mock.patch.object(gate.reduction, "run_reduction", return_value=reduct_rep), \
                    contextlib.redirect_stdout(io.StringIO()):
                status = gate.main(["--artifacts", str(root)])
            self.assertEqual(status, 0)
            acceptance = json.loads((root / "acceptance.json").read_text())
            self.assertEqual("partial", acceptance["status"])
            self.assertEqual(calib_rep, acceptance["calibration"])
            self.assertEqual(checker_rep, acceptance["checker_mutation"])
            self.assertEqual(reduct_rep, acceptance["reduction"])
            self.assertIn("v1:macos-counter-only", acceptance["partial_reasons"])


class GeneratedToolchainTests(unittest.TestCase):
    def test_sanitizer_runtime_failure_is_not_a_memory_finding_or_program_crash(self):
        import subprocess
        execution = object.__new__(gate.Execution)
        stderr = (b"==66==LeakSanitizer has encountered a fatal error.\n"
                  b"==66==HINT: LeakSanitizer does not work under ptrace (strace, gdb, etc)\n")
        result = subprocess.CompletedProcess([], 87, b"done\n", stderr)
        with self.assertRaises(gate.Failure) as observed:
            execution.accept_native(result, b"done\n", "lsan")
        self.assertEqual(observed.exception.kind, "tool_or_harness_failure")
        self.assertEqual(observed.exception.witness, "lsan-runtime-unavailable")
        self.assertFalse(observed.exception.stable_witness)
        self.assertIn("does not work under ptrace", observed.exception.detail)

    def test_linux_requires_exact_pin_and_mac_uses_supported_release_family(self):
        gate.check_toolchain_version('21.1.8', 'Debian clang version 21.1.8', linux=True)
        gate.check_toolchain_version('21.1.9', 'Homebrew clang version 21.1.9', linux=False)
        for llvm, clang, linux in [('21.1.9', 'clang version 21.1.9', True),
                                   ('21.1.8', 'clang version 21.1.7', True),
                                   ('21.2.0', 'clang version 21.2.0', False),
                                   ('21.1.8', 'clang version 22.0.0', False)]:
            with self.subTest(llvm=llvm, clang=clang, linux=linux):
                with self.assertRaises(gate.Failure):
                    gate.check_toolchain_version(llvm, clang, linux=linux)

    def test_prefix_package_provenance_never_claims_missing_dpkg_packages(self):
        import json
        import subprocess
        from unittest import mock
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with mock.patch.dict('os.environ', {'LLVM_SYS_211_PREFIX': str(root / 'prefix')}):
                execution = gate.Execution(root, root / 'exporter')
            execution.run = mock.Mock(return_value=subprocess.CompletedProcess(
                [], 1, b'', b'no packages found matching llvm-21-dev'))
            execution.record_packages()
            record = json.loads((root / 'package-provenance.json').read_text())
            self.assertEqual(record['status'], 'package-manager-metadata-unavailable')
            self.assertFalse(record['system_packages_verified'])
            self.assertEqual(record['prefix'], str(root / 'prefix'))
            self.assertEqual(execution.run.call_args.kwargs, {})


if __name__ == "__main__":
    unittest.main()
