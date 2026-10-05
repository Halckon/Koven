"""Required generated-owner CI wiring; shell probes do not run Cargo or LLVM."""
import fnmatch
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import unittest

from scripts import check_ci_results


ROOT = Path(__file__).resolve().parents[2]
COMMAND = 'python3 scripts/check_generated_owners.py --artifacts "$RUNNER_TEMP/koven-generated-owners"'
RUN_STEP = "Generated resource program contracts"
UPLOAD_STEP = "Preserve generated resource evidence"


def section(text, name, indent):
    match = re.search(rf"^{indent}{re.escape(name)}:\n(.*?)(?=^{indent}\S|\Z)",
                      text, re.M | re.S)
    if match is None:
        raise AssertionError(f"missing required section: {name}")
    return match[1]


def step(job, name):
    matches = re.findall(rf"^      - name: {re.escape(name)}\n(.*?)(?=^      - |\Z)",
                         job, re.M | re.S)
    if len(matches) != 1:
        raise AssertionError(f"expected one required step: {name}; found {len(matches)}")
    return matches[0]


class GeneratedOwnerCiTests(unittest.TestCase):
    def setUp(self):
        self.workflow = (ROOT / ".github/workflows/ci.yml").read_text()
        self.job = section(self.workflow, "test", "  ")

    def command(self):
        run = step(self.job, RUN_STEP)
        commands = re.findall(r"^        run: (.+)$", run, re.M)
        self.assertEqual([COMMAND], commands)
        return commands[0]

    def test_full_batch_is_unconditional_in_existing_two_host_job(self):
        self.assertIn("name: Targeted Tests (${{ matrix.os }})", self.job)
        self.assertIn("os: [macos-14, ubuntu-24.04]", self.job)
        self.assertEqual(2, self.workflow.count("os: [macos-14, ubuntu-24.04]"))
        self.assertEqual(2, self.workflow.count("fail-fast: false"))
        self.assertNotIn("schedule:", section(self.workflow, "on", ""))
        self.assertNotIn("continue-on-error", self.job)
        run = step(self.job, RUN_STEP)
        self.assertNotRegex(run, r"(?m)^        if:")
        self.assertEqual(1, self.workflow.count(COMMAND))
        self.command()
        self.assertLess(self.job.index("uses: ./.github/actions/setup-llvm"),
                        self.job.index(f"- name: {RUN_STEP}"))
        self.assertIn("toolchain: '1.96.0'", self.job)

    def test_raw_evidence_is_required_after_success_and_failure_on_each_host(self):
        upload = step(self.job, UPLOAD_STEP)
        self.assertRegex(upload, r"(?m)^        if: always\(\)$")
        self.assertRegex(upload, r"(?m)^        uses: actions/upload-artifact@v\d+$")
        self.assertIn("name: generated-owners-${{ runner.os }}-${{ github.sha }}", upload)
        self.assertRegex(upload, r"(?m)^          path: \$\{\{ runner.temp \}\}/koven-generated-owners$")
        self.assertRegex(upload, r"(?m)^          if-no-files-found: error$")
        self.assertNotIn("continue-on-error", upload)
        self.assertLess(self.job.index(f"- name: {RUN_STEP}"),
                        self.job.index(f"- name: {UPLOAD_STEP}"))

    def test_rust_filter_covers_generator_calibration_reducer_and_their_tests(self):
        filters = section(self.workflow, "rust", "            ")
        patterns = re.findall(r"^              - '([^']+)'$", filters, re.M)
        for pattern in ("scripts/*generated_owner*.py", "scripts/tests/test_*generated_owner*.py"):
            self.assertIn(pattern, patterns)
        # Include the planned calibration/checker modules even before they exist.
        required = {"scripts/generated_owner_calibration.py", "scripts/check_generated_owner_checker.py",
                    "scripts/generated_owner_reduction.py", "scripts/tests/test_generated_owner_calibration.py",
                    "scripts/tests/test_check_generated_owner_checker.py", "scripts/tests/test_generated_owner_reduction.py"}
        discovered = {*ROOT.glob("scripts/*generated_owner*.py"),
                      *ROOT.glob("scripts/tests/test_*generated_owner*.py")}
        self.assertTrue(discovered, "zero discovered generator inputs cannot establish filter coverage")
        required.update(path.relative_to(ROOT).as_posix() for path in discovered)
        required.update(("scripts/check_native_sanitizers.py", "scripts/native_lsan_entry.c",
                         "scripts/tests/test_check_native_sanitizers.py", ".github/workflows/ci.yml",
                         ".github/actions/setup-llvm/action.yml", "scripts/install_ci_llvm.sh",
                         "crates/lang-codegen/src/native_generated_owner_tests.rs"))
        for path in sorted(required):
            with self.subTest(path=path):
                self.assertTrue(any(fnmatch.fnmatchcase(path, pattern) for pattern in patterns), path)

    def test_existing_m4a_linux_gate_and_evidence_remain_required(self):
        run = step(self.job, "Linux native sanitizer contracts")
        self.assertRegex(run, r"(?m)^        if: runner.os == 'Linux'$")
        self.assertIn('run: python3 scripts/check_native_sanitizers.py --linux --artifacts "$RUNNER_TEMP/koven-sanitizers"', run)
        upload = step(self.job, "Preserve Linux sanitizer evidence")
        self.assertIn("if: always() && runner.os == 'Linux'", upload)
        self.assertIn("if-no-files-found: error", upload)

    def test_required_summary_rejects_missing_failed_or_skipped_targeted_job(self):
        summary = section(self.workflow, "ci-passed", "  ")
        self.assertRegex(summary, r"(?m)^    needs: \[[^\n]*\btest\b[^\n]*\]$")
        self.assertIn("run: python3 scripts/check_ci_results.py", summary)
        jobs = {name: {"result": "success"} for name in
                ("rust-size", "dependencies", "docs", "fmt", "clippy", "test")}
        jobs.update({name: {"result": "skipped"} for name in
                     ("editors", "preview-macos-produce", "preview-macos-consume",
                      "preview-linux-produce", "preview-linux-consume")})
        jobs["changes"] = {"result": "success", "outputs":
                           {"docs": "true", "rust": "true", "editors": "false", "preview": "false"}}
        self.assertEqual([], check_ci_results.check_results(jobs, "pull_request", "refs/pull/1/merge"))
        for result in (None, "skipped", "failure", "cancelled"):
            with self.subTest(result=result):
                changed = {key: value for key, value in jobs.items() if key != "test"}
                if result is not None:
                    changed["test"] = {"result": result}
                self.assertTrue(check_ci_results.check_results(changed, "pull_request", "refs/pull/1/merge"))

    def test_workflow_shell_preserves_driver_exit_and_raw_artifact_directory(self):
        command = self.command()
        # A controlled executable checks shell wiring only, not compiler acceptance.
        probe = '''import json, os, pathlib, sys
assert sys.argv[1] == "--artifacts" and len(sys.argv) == 3
root = pathlib.Path(sys.argv[2])
root.mkdir(parents=True, exist_ok=False)
(root / "raw.stdout").write_bytes(b"raw\\x00\\xff\\r\\n")
(root / "argv.json").write_text(json.dumps(sys.argv[1:]))
raise SystemExit(int(os.environ["PROBE_EXIT"]))
'''
        for status in (0, 1, 7, 86):
            with self.subTest(status=status), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                (root / "scripts").mkdir()
                (root / "scripts/check_generated_owners.py").write_text(probe)
                runner_temp = root / "runner temp with spaces"
                result = subprocess.run(["bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", command],
                                        cwd=root, env={**os.environ, "RUNNER_TEMP": str(runner_temp),
                                                       "PROBE_EXIT": str(status)},
                                        capture_output=True, timeout=10)
                self.assertEqual(status, result.returncode, result.stderr.decode(errors="replace"))
                artifact = runner_temp / "koven-generated-owners"
                self.assertEqual(b"raw\x00\xff\r\n", (artifact / "raw.stdout").read_bytes())
                self.assertEqual(["--artifacts", str(artifact)], json.loads((artifact / "argv.json").read_text()))

    def test_real_entrypoint_missing_tool_configuration_fails_with_evidence(self):
        command = self.command()
        with tempfile.TemporaryDirectory() as temporary:
            environment = {**os.environ, "RUNNER_TEMP": temporary}
            environment.pop("LLVM_SYS_211_PREFIX", None)
            result = subprocess.run(["bash", "--noprofile", "--norc", "-eo", "pipefail", "-c", command],
                                    cwd=ROOT, env=environment, capture_output=True, timeout=10)
            self.assertEqual(1, result.returncode)
            artifact = Path(temporary) / "koven-generated-owners"
            failure = json.loads((artifact / "failure.json").read_text())["first_failure"]
            self.assertEqual("setup", failure["stage"])
            self.assertEqual("tool_or_harness_failure", failure["kind"])
            self.assertEqual("LLVM_SYS_211_PREFIX", failure["witness"])
            self.assertFalse((artifact / "acceptance.json").exists())


if __name__ == "__main__":
    unittest.main()
