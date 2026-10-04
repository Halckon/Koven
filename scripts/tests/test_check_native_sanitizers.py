"""Detection evidence must reject ordinary failure, missing wiring and empty tests."""
import importlib.util
from pathlib import Path
import subprocess
import tempfile
import time
import unittest

ROOT = Path(__file__).resolve().parents[2]
SPEC = importlib.util.spec_from_file_location("sanitizers", ROOT / "scripts/check_native_sanitizers.py")
GATE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(GATE)


class NativeSanitizerGateTests(unittest.TestCase):
    def test_detector_requires_failure_and_its_exact_category(self):
        report = b"ERROR: AddressSanitizer: heap-buffer-overflow"
        good = subprocess.CompletedProcess([], 86, b"", report)
        GATE.assert_detected(good, "AddressSanitizer", "heap-buffer-overflow")
        for result in (subprocess.CompletedProcess([], 0, b"", report),
                       subprocess.CompletedProcess([], -11, b"", b"Segmentation fault"),
                       subprocess.CompletedProcess([], 86, b"", b"ERROR: AddressSanitizer: SEGV"),
                       subprocess.CompletedProcess([], 86, b"", report + b"-other"),
                       subprocess.CompletedProcess([], 86, b"", b"ERROR: LeakSanitizer: detected memory leaks")):
            with self.subTest(result=result):
                with self.assertRaises(AssertionError):
                    GATE.assert_detected(result, "AddressSanitizer", "heap-buffer-overflow")

    def test_disabled_detector_cannot_report_a_sanitizer_failure(self):
        GATE.assert_disabled(subprocess.CompletedProcess([], -11, b"", b"Segmentation fault"))
        for report in (b"ERROR: AddressSanitizer: SEGV", b"AddressSanitizer:DEADLYSIGNAL"):
            with self.assertRaises(AssertionError):
                GATE.assert_disabled(subprocess.CompletedProcess([], 86, b"", report))

    def test_exact_exporter_test_must_execute_once_without_ignore(self):
        good = f"test {GATE.EXPORT_TEST} ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored; 5 filtered out;"
        GATE.assert_export_ran(good)
        for text in ("test result: ok. 0 passed; 0 failed; 0 ignored;", "", good.replace("1 passed", "2 passed"),
                     good.replace("0 ignored", "1 ignored"), good.replace(GATE.EXPORT_TEST, "other")):
            with self.subTest(text=text):
                with self.assertRaises(AssertionError):
                    GATE.assert_export_ran(text)

    def test_ir_check_is_scoped_to_the_actual_function(self):
        text = "define void @other() {\n call void @__asan_report_load4(i64 0)\n}\ndefine void @user() {\n ret void\n}\n"
        with self.assertRaises(AssertionError):
            GATE.assert_instrumented(text, "user", "load", True)
        GATE.assert_instrumented(text, "other", "load", True)
        GATE.assert_instrumented(text, "user", "load", False)
        with self.assertRaises(AssertionError):
            GATE.assert_instrumented(text, "missing", "load", False)

    def test_missing_tool_and_timeout_preserve_failure_evidence(self):
        import sys
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            with self.assertRaises(FileNotFoundError):
                GATE.run([str(directory / "missing")], directory, "missing", timeout=1)
            self.assertTrue((directory / "missing.command.json").is_file())
            with self.assertRaises(subprocess.TimeoutExpired):
                GATE.run([sys.executable, "-c", "import time; time.sleep(30)"], directory, "timeout", timeout=0.1)
            self.assertIn('"timed_out": true', (directory / "timeout.result.json").read_text())

    def test_timeout_terminates_child_processes_too(self):
        import sys
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            marker = directory / "escaped-child"
            child = f"import time; from pathlib import Path; time.sleep(0.5); Path({str(marker)!r}).touch()"
            parent = f"import subprocess,sys,time; subprocess.Popen([sys.executable, '-c', {child!r}]); time.sleep(30)"
            with self.assertRaises(subprocess.TimeoutExpired):
                GATE.run([sys.executable, "-c", parent], directory, "tree", timeout=0.2)
            time.sleep(0.6)
            self.assertFalse(marker.exists(), "a timed-out probe must not leave executable descendants")

    def test_linux_step_is_required_and_installs_pinned_runtime(self):
        workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        job = workflow.split("  test:\n", 1)[1].split("  ci-passed:\n", 1)[0]
        self.assertIn("- name: Linux native sanitizer contracts", job)
        step = job.split("- name: Linux native sanitizer contracts", 1)[1].split("      - ", 1)[0]
        self.assertIn("if: runner.os == 'Linux'", step)
        self.assertNotIn("continue-on-error", job)
        self.assertIn("python3 scripts/check_native_sanitizers.py --linux", step)
        self.assertIn('"libclang-rt-21-dev=$version"', (ROOT / "scripts/install_ci_llvm.sh").read_text())
        filters = workflow.split("            rust:\n", 1)[1].split("            frontend:\n", 1)[0]
        for name in ("scripts/check_native_sanitizers.py", "scripts/tests/test_check_native_sanitizers.py"):
            self.assertIn(f"- '{name}'", filters)


if __name__ == "__main__":
    unittest.main()
