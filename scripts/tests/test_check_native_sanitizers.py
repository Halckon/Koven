"""Detection evidence must reject ordinary failure, missing wiring and empty tests."""
import importlib.util
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest import mock

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

    def test_pinned_debian_runtime_uses_resource_linux_layout(self):
        # Clang 21 reports a per-target runtime dir even when Debian only ships
        # the older lib/linux archives that its linker falls back to.
        with tempfile.TemporaryDirectory() as temporary:
            directory = Path(temporary)
            resource = directory / "lib/clang/21"
            libraries = resource / "lib/linux"
            libraries.mkdir(parents=True)
            archives = [libraries / f"libclang_rt.{name}-x86_64.a" for name in ("asan", "lsan")]
            for archive in archives:
                archive.touch()

            def query(command, *_args):
                reported = {"--print-resource-dir": resource,
                            "--print-runtime-dir": resource / "lib/x86_64-pc-linux-gnu"}
                return subprocess.CompletedProcess(command, 0, f"{reported[command[1]]}\n".encode(), b"")

            with mock.patch.object(GATE, "checked", side_effect=query):
                self.assertEqual(GATE.linux_runtime_archives(directory, "clang"), archives)
                archives[1].unlink()
                with self.assertRaisesRegex(AssertionError, "libclang_rt.lsan-x86_64.a"):
                    GATE.linux_runtime_archives(directory, "clang")

    def test_lsan_diagnostic_never_replaces_original_missing_detection(self):
        detector = GATE.assert_detected
        report = b"ERROR: LeakSanitizer: detected memory leaks\nDirect leak of 4 byte(s) in 1 object(s)\ntarget"
        outcomes = (subprocess.CompletedProcess([], 87, b"read\ndrop\n", report),
                    subprocess.CompletedProcess([], 0, b"read\ndrop\n", b""),
                    OSError("diagnostic spawn failed"), subprocess.TimeoutExpired("diagnostic", 10))
        for outcome in outcomes:
            with self.subTest(outcome=outcome), tempfile.TemporaryDirectory() as temporary:
                directory = Path(temporary) / "artifacts"
                failures = []

                def checked(_command, output, name, **_kwargs):
                    stdout = {"llvm-version": b"21.1.8", "clang-version": b"clang version 21.1.8"}.get(name, b"")
                    if name == "export-koven":
                        (output / "fixtures").mkdir()
                        (output / "fixtures/expected.stdout").write_bytes(b"read\ndrop\n")
                        for fixture in ("clean", "leak"):
                            (output / f"fixtures/{fixture}.raw.ll").write_text("define i32 @main() {\n ret i32 0\n}\n")
                    return subprocess.CompletedProcess([], 0, stdout, b"")

                def detect(result, name, category):
                    try:
                        detector(result, name, category)
                    except AssertionError as error:
                        failures.append(error)
                        raise

                def execute(_command, _output, name, **_kwargs):
                    if name == "diagnose-leak-lsan":
                        if isinstance(outcome, Exception):
                            raise outcome
                        return outcome
                    if name in {"run-user-asan", "run-runtime-asan", "run-drop-asan"}:
                        category = "heap-use-after-free" if name == "run-drop-asan" else "heap-buffer-overflow"
                        return subprocess.CompletedProcess([], 86, b"", f"ERROR: AddressSanitizer: {category}\ntarget".encode())
                    # The original leak run remains a missed detection.
                    return subprocess.CompletedProcess([], 0, b"read\ndrop\n", b"")

                with mock.patch.multiple(GATE.platform, system=lambda: "Linux", machine=lambda: "x86_64"), \
                        mock.patch.dict(os.environ, LLVM_SYS_211_PREFIX=temporary), \
                        mock.patch.object(GATE, "checked", side_effect=checked) as builds, \
                        mock.patch.object(GATE, "linux_runtime_archives", return_value=[]), \
                        mock.patch.object(GATE, "fixtures", return_value={name: ("target", "load") for name in GATE.CASES}), \
                        mock.patch.object(GATE, "check_ir"), mock.patch.object(GATE, "assert_export_ran"), \
                        mock.patch.object(GATE, "assert_detected", side_effect=detect), \
                        mock.patch.object(GATE, "run", side_effect=execute) as run:
                    with self.assertRaises(AssertionError) as caught:
                        GATE.check_linux(directory)
                    self.assertEqual(len(failures), 1)
                    self.assertIs(caught.exception, failures[0])
                    diagnoses = [call for call in run.call_args_list if call.args[2] == "diagnose-leak-lsan"]
                    self.assertEqual(len(diagnoses), 1)
                    self.assertEqual(diagnoses[0].kwargs["timeout"], 10)
                    options = diagnoses[0].kwargs["env"]["LSAN_OPTIONS"]
                    self.assertIn("detect_leaks=1", options)
                    for option in ("verbosity=1", "log_threads=1", "log_pointers=1"):
                        self.assertIn(option, options)
                    self.assertNotIn("use_stacks", options)
                    self.assertNotIn("use_registers", options)
                    self.assertFalse((directory / "acceptance.txt").exists())
                    for fixture in ("clean", "leak"):
                        build = [call.args[0] for call in builds.call_args_list
                                 if call.args[2] == f"build-{fixture}-lsan"]
                        self.assertEqual(len(build), 1)
                        self.assertIn("-pthread", build[0])
                        self.assertIn("-Wl,--wrap=main", build[0])
                        self.assertIn(directory / "lsan-entry.c", build[0])
                    self.assertEqual((directory / "lsan-entry.c").read_bytes(),
                                     (ROOT / "scripts/native_lsan_entry.c").read_bytes())

    def test_lsan_adapter_rejects_a_changed_main_abi(self):
        entry = "define i32 @main() {\n ret i32 0\n}\n"
        GATE.assert_lsan_entry(entry)
        for wrong in (entry.replace("@main()", "@main(i32 %argc, ptr %argv)"),
                      entry.replace("define i32", "define void"), entry.replace("@main", "@other"),
                      entry + entry):
            with self.subTest(wrong=wrong):
                with self.assertRaises(AssertionError):
                    GATE.assert_lsan_entry(wrong)

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
        for name in ("scripts/check_native_sanitizers.py", "scripts/native_lsan_entry.c",
                     "scripts/tests/test_check_native_sanitizers.py"):
            self.assertIn(f"- '{name}'", filters)


class LsanEntryAdapterTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.temporary = tempfile.TemporaryDirectory()
        cls.addClassCleanup(cls.temporary.cleanup)
        directory = Path(cls.temporary.name)
        oracle = directory / "oracle.c"
        oracle.write_text(r"""
#include <errno.h>
#include <pthread.h>
#include <stdatomic.h>
#include <stdio.h>
#include <stdlib.h>
#include <unistd.h>

static int mode, joined;
static atomic_int calls;
static pthread_t parent;
extern int __wrap_main(void);

int __real_main(void) {
    atomic_fetch_add(&calls, 1);
    if (write(1, "real entry\n", 11) != 11) return 36;
    if (pthread_equal(pthread_self(), parent)) return 31;
    return mode == 3 ? 32 : 0;
}
int probe_create(pthread_t *thread, const pthread_attr_t *attributes,
                 void *(*start)(void *), void *argument) {
    if (mode == 1) {
        if (atomic_load(&calls) != 0) _Exit(37);
        return EAGAIN;
    }
    return pthread_create(thread, attributes, start, argument);
}
int probe_join(pthread_t thread, void **result) {
    if (mode == 2) return EINVAL;
    int status = pthread_join(thread, result);
    joined = status == 0;
    return status;
}
int main(int argc, char **argv) {
    if (argc != 2) return 34;
    mode = atoi(argv[1]);
    parent = pthread_self();
    int status = __wrap_main();
    if (!joined || atomic_load(&calls) != 1) return 33;
    return status;
}
""")
        obj = directory / "entry.o"
        cls.executable = directory / "entry-test"
        for command in (["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread",
                         "-Dpthread_create=probe_create", "-Dpthread_join=probe_join", "-c",
                         ROOT / "scripts/native_lsan_entry.c", "-o", obj],
                        ["cc", "-std=c11", "-Wall", "-Wextra", "-Werror", "-pthread",
                         oracle, obj, "-o", cls.executable]):
            result = subprocess.run(command, capture_output=True, timeout=30)
            if result.returncode:
                raise AssertionError(f"adapter test build failed: {result.stderr.decode()}")

    def test_real_entry_runs_once_in_worker_with_original_status(self):
        for mode, status in ((0, 0), (3, 32)):
            with self.subTest(status=status):
                result = subprocess.run([self.executable, str(mode)], capture_output=True, timeout=10)
                self.assertEqual(result.returncode, status)
                self.assertEqual(result.stdout, b"real entry\n")
                self.assertEqual(result.stderr, b"")
                if status:
                    with self.assertRaises(AssertionError):
                        GATE.assert_detected(result, "LeakSanitizer", "detected memory leaks")

    def test_thread_failures_cannot_count_as_leak_detection(self):
        for mode, operation in ((1, "pthread_create"), (2, "pthread_join")):
            with self.subTest(operation=operation):
                result = subprocess.run([self.executable, str(mode)], capture_output=True, timeout=10)
                self.assertEqual(result.returncode, 90)
                self.assertIn(f"LSan fixture {operation} failed:".encode(), result.stderr)
                if mode == 1:
                    self.assertEqual(result.stdout, b"", "entry cannot run when thread creation fails")
                with self.assertRaises(AssertionError):
                    GATE.assert_detected(result, "LeakSanitizer", "detected memory leaks")


if __name__ == "__main__":
    unittest.main()
