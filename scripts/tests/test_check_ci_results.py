"""CI event/path policy must never turn an unexecuted required gate green."""

import importlib.util
import re
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location(
    "check_ci_results", Path(__file__).resolve().parents[1] / "check_ci_results.py"
)
CI = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(CI)


def needs(docs_changed="false", rust_changed="false", **results):
    jobs = {name: {"result": results.get(name, "skipped")}
            for name in ("docs", "fmt", "clippy", "test")}
    jobs["changes"] = {"result": results.get("changes", "success"),
                       "outputs": {"docs": docs_changed, "rust": rust_changed}}
    return jobs


class CheckCiResultsTests(unittest.TestCase):
    def test_docs_only_pr_can_skip_rust(self):
        self.assertEqual([], CI.check_results(
            needs("true", "false", docs="success"), "pull_request", "refs/pull/1/merge"))

    def test_rust_pr_requires_both_matrix_jobs(self):
        jobs = needs("false", "true", fmt="success", clippy="success", test="success")
        self.assertEqual([], CI.check_results(jobs, "pull_request", "refs/pull/1/merge"))
        for name in ("fmt", "clippy", "test"):
            for result in ("skipped", "failure", "cancelled"):
                with self.subTest(name=name, result=result):
                    altered = {**jobs, name: {"result": result}}
                    self.assertTrue(CI.check_results(altered, "pull_request", "refs/pull/1/merge"))

    def test_main_and_manual_dispatch_require_all_gates(self):
        for event, ref in (("push", "refs/heads/main"), ("workflow_dispatch", "refs/heads/fix/test")):
            jobs = needs("false", "false", **dict.fromkeys(("docs", "fmt", "clippy", "test"), "success"))
            self.assertEqual([], CI.check_results(jobs, event, ref))
            self.assertTrue(CI.check_results(needs(), event, ref))

    def test_feature_push_preserves_existing_cost_policy(self):
        jobs = needs("false", "true", fmt="success")
        self.assertEqual([], CI.check_results(jobs, "push", "refs/heads/feature/test"))

    def test_failed_or_missing_change_detection_cannot_pass(self):
        for result in ("failure", "cancelled", "skipped", None):
            self.assertTrue(CI.check_results(needs(changes=result), "pull_request", "refs/pull/1/merge"))
        self.assertTrue(CI.check_results({}, "pull_request", "refs/pull/1/merge"))

    def test_workflow_filters_all_guide_litmus_inputs_and_gate_scripts(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        filters = workflow.split("            rust:\n", 1)[1].split("            frontend:\n", 1)[0]
        source = root / "crates/lang-frontend/tests/guide_litmus.rs"
        inputs = re.findall(r'include_str!\("([^"]+)"\)', source.read_text())
        self.assertTrue(inputs, "Litmus input discovery must not silently match zero files")
        for name in inputs:
            path = (source.parent / name).resolve().relative_to(root).as_posix()
            self.assertIn(f"- '{path}'", filters)
        for path in ("scripts/check_stage_integration.sh", "scripts/check_guide_litmus.sh",
                     "scripts/install_ci_llvm.sh", "scripts/check_ci_results.py",
                     "scripts/tests/test_check_ci_results.py", ".github/actions/setup-llvm/**"):
            self.assertIn(f"- '{path}'", filters)

    def test_workflow_preserves_both_matrices_and_required_summary(self):
        root = Path(__file__).resolve().parents[2]
        workflow = (root / ".github/workflows/ci.yml").read_text()
        self.assertEqual(2, workflow.count("os: [macos-14, ubuntu-24.04]"))
        self.assertEqual(2, workflow.count("fail-fast: false"))
        self.assertIn("needs: [changes, docs, fmt, clippy, test]", workflow)
        for command in ("cargo test --locked -p lang-codegen", "cargo test --locked -p lang-cli",
                        "bash scripts/check_stage_integration.sh", "bash scripts/check_guide_litmus.sh"):
            self.assertIn(command, workflow)

    def test_invalid_or_missing_filter_outputs_cannot_pass(self):
        for value in ("", "unknown", None):
            self.assertTrue(CI.check_results(needs(rust_changed=value), "pull_request", "refs/pull/1/merge"))


if __name__ == "__main__":
    unittest.main()
