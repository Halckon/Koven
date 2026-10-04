"""Required generated batches must have real exporter hits and complete cases."""
import tempfile
from pathlib import Path
import unittest
from scripts import check_generated_owners as gate


class GeneratedDriverTests(unittest.TestCase):
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


if __name__ == "__main__":
    unittest.main()
